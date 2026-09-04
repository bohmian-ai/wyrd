---
id: TASK-018
title: Exclusive Wyrd server runtime targets
kind: implementation
status: proposed
spec: SPEC-wyrd-ui-foundation
spec_revision: 6
requirements: [REQ-102, REQ-103, REQ-104, REQ-105, INV-001, INV-002, INV-009, AC-006]
depends_on: []
parent_task:
remediates: []
---

# Outcome and operator value

Make `WYRD_TARGET` closed and exclusive so all-in-one runs the complete system
while split deployments do not duplicate Oracle, Scribe, or Forge workers in
the core server pod.

# Owner and write set

- Own the smallest required `wyrd-server` config/boot/route/gRPC/readiness code,
  focused `wyrd-testing` support, and directly contradicted operations docs.
- Support exactly `all`, `server`, `oracle`, `scribe`, and `forge-worker`; reject
  unknown values before runtime startup.
- Derive constructed owners, listeners, routes/services, background work,
  storage requirements, shutdown, and readiness from one typed target.

| Target | Public surface | Runtime ownership |
|---|---|---|
| `all` | yes | core, Scribe, Oracle, Forge coordinator and worker |
| `server` | yes | core and Forge coordinator |
| `oracle` | yes | Oracle query/read |
| `scribe` | yes | Scribe ingest/OTLP |
| `forge-worker` | no | internal Forge work |

# Locked decisions and non-goals

- One binary owns every target; disabled owners are not constructed and cannot
  claim readiness. `/healthz` is liveness and `/readyz` is target-aware.
- Public Oracle/Scribe requests retain normal auth; internal work retains mTLS
  and purpose-bound peer contracts.
- No new role, Gate target, client type, compatibility alias, gateway, manifest,
  or Bifrost redesign. New/material Rust follows struct-centered style.

# Ordered test scenarios

1. Default/valid/unknown parsing proves the closed vocabulary.
2. Each target constructs exactly the owners and public services in the table.
3. `server` excludes local data-plane owners; `forge-worker` opens no listener.
4. Readiness reports only applicable dependencies with stable reasons.
5. Shutdown drains only owners that were started and preserves accepted work.

# Red-Green-Refactor

Drive one target at a time. Move all repeated target decisions onto one cohesive
runtime owner instead of scattering string checks.

# Exact verification

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=config::tests::target_parses_only_closed_values)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=boot::tests::server_target_excludes_data_plane_owners)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=boot::tests::forge_worker_has_no_public_listener)'
mise run fmt
mise run lints
mise run test:wyrd
```

# Evidence and stop conditions

Record the target/owner/route/readiness matrix and exact test results. Stop if
current architecture cannot separate an owner without changing a durable public
contract; return that conflict to `$wyrd-spec`.

# Execution skills

Use `$wyrd-implement`.
