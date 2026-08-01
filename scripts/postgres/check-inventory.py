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
    "db:migrate", "db:migrate:all", "test:sql", "test:sql:forge-scale", "test:vala",
    "test:vala:integration", "test:wyrd",
}
pre = {
    "test:bifrost:journey", "test:bifrost:ingest-runtime", "test:bifrost:inspection", "test:fuzz:bifrost",
    "test:bifrost:sustained", "test:e2e", "py:test:integration", "ts:test:integration", "identity:e2e",
    "test:storage:e2e", "test:storage:s3:cloud", "test:storage:gcs:cloud", "test:storage:azure:cloud",
    "bench", "bench:workload", "bench:bifrost:slo", "bench:bifrost:preflight", "bench:bifrost:capacity",
    "bench:bifrost:otlp:ingest", "bench:bifrost:scribe:slo", "bench:bifrost:scribe:components",
    "bench:bifrost:scribe:single-tenant-single-node", "bench:bifrost:scribe:multi-tenant-single-node",
    "bench:bifrost:scribe:multi-tenant-multi-node", "bench:bifrost:scribe:sustained", "bench:bifrost:forge:slo",
    "bench:bifrost:forge:single-tenant-single-node", "bench:bifrost:forge:multi-tenant-single-node",
    "bench:bifrost:forge:multi-tenant-multi-node", "bench:bifrost:oracle:slo", "bench:bifrost:oracle:calibrate",
}
aggregates = {"test:unit", "pre-pr", "test:storage:matrix", "test:storage:cloud:matrix"}
for name in empty | pre:
    task = tasks.get(name)
    if task is None or "with-test-postgres.sh" not in str(task.get("run", "")):
        raise SystemExit(f"{name} is not routed through the lifecycle wrapper")
    if f"{name}:inner" not in tasks:
        raise SystemExit(f"{name} has no hidden inner task")
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


check_inner_lifecycle_dependencies(tasks)

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
