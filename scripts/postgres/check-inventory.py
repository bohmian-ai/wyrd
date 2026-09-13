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
    "db:migrate", "db:migrate:all", "test:sql",
    "test:bifrost:integration:redux", "test:bifrost:integration:sql",
}
migrated = {"test:bifrost", "test:vala", "test:wyrd", "test:shared"}
pre = {
    "test:bifrost:integration:server",
    "test:bifrost:journey",
    "test:bifrost:journey:python", "test:bifrost:journey:typescript",
    "test:cards:integration", "test:cli:journey", "test:wyrdstate:journey",
    "py:test:cards:integration", "py:test:wyrdstate:integration",
    "py:test:integration", "ts:test:integration", "test:identity:journey",
    "test:storage:e2e", "test:storage:s3:cloud", "test:storage:gcs:cloud", "test:storage:azure:cloud",
}
aggregates = {"test:rust", "gate", "test:storage:matrix", "test:storage:cloud:matrix"}
for name in empty | migrated | pre:
    task = tasks.get(name)
    if task is None or "with-test-postgres.sh" not in str(task.get("run", "")):
        raise SystemExit(f"{name} is not routed through the lifecycle wrapper")
    deps = set(task.get("depends", []))
    if deps & {"setup:postgres", "db:migrate"}:
        raise SystemExit(f"{name} still has setup-only Postgres dependencies")

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


def check_bifrost_migrated_lane(task_map):
    """Require one migrated lifecycle around the complete Bifrost runner."""
    outer = task_map["test:bifrost"].get("run")
    expected_outer = (
        "scripts/postgres/with-test-postgres.sh -- bash -lc "
        "'mise run db:migrate:all:inner && mise run test:bifrost:inner'"
    )
    if outer != expected_outer:
        raise SystemExit("test:bifrost must run the exact complete migrated lane")

    inner = task_map["test:bifrost:inner"].get("run")
    if inner != "bash scripts/run-bifrost-tests.sh":
        raise SystemExit("test:bifrost:inner must preserve the complete Bifrost runner")


check_inner_lifecycle_dependencies(tasks)
check_vala_migrated_lane(tasks)
check_bifrost_migrated_lane(tasks)

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
if wyrd.get("run") != (
    "scripts/postgres/with-test-postgres.sh -- bash -lc "
    "'mise run db:migrate:all:inner && mise run test:wyrd:inner'"
):
    raise SystemExit("test:wyrd must run the exact complete migrated family lane")
if tasks["test:wyrd:inner"].get("run") != "bash scripts/run-family-tests.sh wyrd":
    raise SystemExit("test:wyrd:inner must preserve the canonical family test command")
shared = tasks["test:shared"]
if shared.get("run") != (
    "scripts/postgres/with-test-postgres.sh -- bash -lc "
    "'mise run db:migrate:all:inner && mise run test:shared:inner'"
):
    raise SystemExit("test:shared must run the exact complete migrated family lane")
if tasks["test:shared:inner"].get("run") != "bash scripts/run-family-tests.sh shared":
    raise SystemExit("test:shared:inner must preserve the canonical family test command")
for name in aggregates:
    if "with-test-postgres.sh" in str(tasks.get(name, {}).get("run", "")):
        raise SystemExit(f"aggregate {name} owns a nested lifecycle")
for name, task in tasks.items():
    if ":inner" not in name and "55432" in str(task):
        raise SystemExit(f"fixed database endpoint remains in task {name}")
print("postgres task inventory: PASS")
