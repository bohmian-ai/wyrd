#!/usr/bin/env python3
"""Reject fixed-port and setup-only Postgres task regressions."""
from pathlib import Path
import tomllib

root = Path(__file__).resolve().parents[2]
tasks = tomllib.loads((root / "mise.toml").read_text())["tasks"]
removed = {"build:postgres", "setup:db-roles", "setup:postgres", "impl:setup", "pg:teardown"}
if removed & tasks.keys():
    raise SystemExit(f"removed lifecycle tasks remain: {sorted(removed & tasks.keys())}")
compose = (root / "docker-compose.yml").read_text()
if "55432" in compose or '"55432:5432"' in compose:
    raise SystemExit("fixed Postgres host port remains")
if "docker ps" in (root / "mise.toml").read_text() or "docker rm" in (root / "mise.toml").read_text():
    raise SystemExit("global Docker enumeration/deletion remains")
empty = {"db:migrate", "db:migrate:all", "test:sql", "test:sql:forge-scale", "test:vala", "test:vala:integration"}
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
for name in aggregates:
    if "with-test-postgres.sh" in str(tasks.get(name, {}).get("run", "")):
        raise SystemExit(f"aggregate {name} owns a nested lifecycle")
for name, task in tasks.items():
    if ":inner" not in name and "55432" in str(task):
        raise SystemExit(f"fixed database endpoint remains in task {name}")
print("postgres task inventory: PASS")
