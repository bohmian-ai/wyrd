#!/usr/bin/env python3
"""Reject fixed-port and setup-only Postgres task regressions."""
from pathlib import Path
import sys
import tomllib

root = Path(__file__).resolve().parents[2]
mise_path = (
    Path(sys.argv[1])
    if len(sys.argv) == 2 and sys.argv[1] != "--negative-test"
    else root / "mise.toml"
)
tasks = tomllib.loads(mise_path.read_text())["tasks"]
removed = {"build:postgres", "setup:db-roles", "setup:postgres", "impl:setup", "pg:teardown"}
if removed & tasks.keys():
    raise SystemExit(f"removed lifecycle tasks remain: {sorted(removed & tasks.keys())}")
compose = (root / "docker-compose.yml").read_text()
if "55432" in compose or '"55432:5432"' in compose:
    raise SystemExit("fixed Postgres host port remains")
if "docker ps" in (root / "mise.toml").read_text() or "docker rm" in (root / "mise.toml").read_text():
    raise SystemExit("global Docker enumeration/deletion remains")
empty = {
    "db:migrate", "db:migrate:all", "test:sql", "test:sql:forge-scale",
    "test:bifrost", "test:wyrd",
}
migrated = {"test:vala"}
pre = {
    "test:bifrost:journey",
    "test:e2e", "py:test:integration", "ts:test:integration", "identity:e2e",
    "test:storage:e2e", "test:storage:s3:cloud", "test:storage:gcs:cloud", "test:storage:azure:cloud",
    "bench:bifrost:smoke",
}
aggregates = {"test:unit", "gate", "test:storage:matrix", "test:storage:cloud:matrix"}
for name in empty | migrated | pre:
    task = tasks.get(name)
    if task is None or "with-test-postgres.sh" not in str(task.get("run", "")):
        raise SystemExit(f"{name} is not routed through the lifecycle wrapper")
    deps = set(task.get("depends", []))
    if deps & {"setup:postgres", "db:migrate"}:
        raise SystemExit(f"{name} still has setup-only Postgres dependencies")

def check_bifrost_benchmark_lifecycle(task_map):
    """Require reference-profile parity on every database-backed Bifrost bench lane.

    Under the tiered-runner taxonomy the database-backed lanes are the smoke
    suite and the four per-family qualification lanes. Each must attempt the
    canonical open-file raise, run under the reference profile and compose file,
    and migrate before the benchmark binary starts.
    """
    lanes = (
        "bench:bifrost:smoke",
        "bench:bifrost:qualification:ingest",
        "bench:bifrost:qualification:oracle",
        "bench:bifrost:qualification:distributed",
        "bench:bifrost:qualification:mixed",
    )
    for name in lanes:
        if task_map[name].get("env", {}).get("WYRD_BIFROST_REFERENCE_PROFILE") != "1":
            raise SystemExit(f"{name} must enable the canonical reference profile")
        expected_raise = "ulimit -n 8192 2>/dev/null || true;"
        if expected_raise not in task_map[name].get("run", ""):
            raise SystemExit(f"{name} must attempt the canonical open-file limit before Rust preflight")
        if task_map[name].get("env", {}).get("WYRD_POSTGRES_COMPOSE_FILE") != "benches/bifrost/docker-compose.reference.yml":
            raise SystemExit(f"{name} must use the canonical reference compose file")
        if "mise run db:migrate:inner" not in task_map[name].get("run", ""):
            raise SystemExit(f"{name} must migrate before benchmark startup")


check_bifrost_benchmark_lifecycle(tasks)

smoke_inner = tasks["bench:bifrost:smoke:inner"]
if not smoke_inner.get("hide"):
    raise SystemExit("the Bifrost smoke inner task must remain hidden")
smoke_inner_run = str(smoke_inner.get("run", ""))
for fragment in (
    "--bench bench_bifrost_scribe",
    "--bench bench_bifrost_oracle",
    "--tier smoke",
    "smoke_suite_seconds",
):
    if fragment not in smoke_inner_run:
        raise SystemExit(f"the Bifrost smoke inner lane must drive both binaries under the smoke budget: missing {fragment}")

def dependencies(task):
    raw = task.get("depends", [])
    return [raw] if isinstance(raw, str) else raw

def check_inner_lifecycle_dependencies(task_map):
    """Reject inner-task dependency paths that re-enter a lifecycle wrapper."""
    lifecycle_owners = {
        name
        for name, task in task_map.items()
        if not name.endswith(":inner")
        and "with-test-postgres.sh" in str(task.get("run", ""))
    }
    for inner_name in (name for name in task_map if name.endswith(":inner")):
        pending = [(inner_name, [inner_name])]
        visited = {inner_name}
        while pending:
            current, path = pending.pop()
            for dependency in dependencies(task_map[current]):
                next_path = [*path, dependency]
                if dependency in lifecycle_owners:
                    raise SystemExit(
                        f"{inner_name} reaches nested lifecycle owner via {' -> '.join(next_path)}"
                    )
                if dependency in task_map and dependency not in visited:
                    visited.add(dependency)
                    pending.append((dependency, next_path))


def check_vala_migrated_lane(task_map):
    """Require one Vala migration setup followed by the canonical family run."""
    outer = task_map["test:vala"].get("run")
    expected_outer = (
        "scripts/postgres/with-test-postgres.sh -- bash -lc "
        "'mise run db:migrate:all:inner && mise run test:vala:inner'"
    )
    if outer != expected_outer:
        raise SystemExit("test:vala must run the exact Wyrd+Vala migrated lane")

    inner = task_map["test:vala:inner"].get("run")
    expected_inner = "bash scripts/run-family-tests.sh vala"
    if inner != expected_inner:
        raise SystemExit("test:vala:inner must preserve the canonical family test command")


check_inner_lifecycle_dependencies(tasks)
check_vala_migrated_lane(tasks)

if len(sys.argv) == 2 and sys.argv[1] == "--negative-test":
    fixture = {name: dict(task) for name, task in tasks.items()}
    fixture["test:wyrd:inner"] = dict(fixture["test:wyrd:inner"])
    fixture["test:wyrd:inner"]["depends"] = ["test:vala"]
    try:
        check_inner_lifecycle_dependencies(fixture)
    except SystemExit as error:
        if "test:wyrd:inner reaches nested lifecycle owner" not in str(error):
            raise
        print("postgres inventory negative test: PASS")
    else:
        raise SystemExit("inventory nested-lifecycle negative test unexpectedly passed")

    vala_fixtures = {
        "removed": (
            "test:vala",
            "run",
            "scripts/postgres/with-test-postgres.sh -- mise run test:vala:inner",
            "exact Wyrd+Vala migrated lane",
        ),
        "wrong": (
            "test:vala",
            "run",
            "scripts/postgres/with-test-postgres.sh -- bash -lc "
            "'mise run db:migrate:inner && mise run test:vala:inner'",
            "exact Wyrd+Vala migrated lane",
        ),
        "nested": (
            "test:vala",
            "run",
            "scripts/postgres/with-test-postgres.sh -- bash -lc "
            "'mise run db:migrate:all && mise run test:vala:inner'",
            "exact Wyrd+Vala migrated lane",
        ),
        "override": (
            "test:vala:inner",
            "run",
            "bash scripts/run-family-tests.sh vala --skip "
            "pg_tests::vala_migrations_apply_and_are_idempotent",
            "preserve the canonical family test command",
        ),
    }
    for case, (task_name, field, value, expected_error) in vala_fixtures.items():
        fixture = {name: dict(task) for name, task in tasks.items()}
        fixture[task_name][field] = value
        try:
            check_vala_migrated_lane(fixture)
        except SystemExit as error:
            if expected_error not in str(error):
                raise
        else:
            raise SystemExit(f"inventory Vala {case} negative test unexpectedly passed")
    print("postgres inventory Vala lane negative tests: PASS")

wyrd = tasks["test:wyrd"]
if wyrd.get("run") != "scripts/postgres/with-test-postgres.sh -- mise run test:wyrd:inner":
    raise SystemExit("test:wyrd must delegate exactly once through with-test-postgres.sh")
if tasks["test:wyrd:inner"].get("run") != "bash scripts/run-family-tests.sh wyrd":
    raise SystemExit("test:wyrd:inner must preserve the canonical family test command")
for name in aggregates:
    if "with-test-postgres.sh" in str(tasks.get(name, {}).get("run", "")):
        raise SystemExit(f"aggregate {name} owns a nested lifecycle")
for name, task in tasks.items():
    if ":inner" not in name and "55432" in str(task):
        raise SystemExit(f"fixed database endpoint remains in task {name}")
print("postgres task inventory: PASS")
