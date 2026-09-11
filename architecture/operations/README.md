# Wyrd Operations Architecture

This directory defines the production operating contract for Wyrd. A supported
deployment must satisfy these documents before readiness is reported.

| Document | Responsibility |
|---|---|
| [`deployment-and-release.md`](deployment-and-release.md) | Supported topologies, configuration, network boundaries, database roles, migrations, rollout, rollback, and version skew |
| [`reliability-and-recovery.md`](reliability-and-recovery.md) | Service objectives, capacity, backup, restore, failure handling, incident response, and recovery qualification |
| [`runbooks.md`](runbooks.md) | Provider-neutral containment, evidence, recovery, and stop/go procedures for critical incidents |

Operations cannot weaken the protocol in `../wyrd-design.md`, Bifrost behavior
in `../bifrost-design.md`, or controls in `../wyrd-security-posture.md`.

## Readiness rule

A process is live when it can continue its supervised work. A replica is ready
only when it can safely receive the traffic assigned to its enabled roles.
Readiness therefore requires:

- valid typed configuration and resolved secrets;
- required schema versions and database-role separation;
- reachable authoritative Postgres and object storage;
- loaded authentication, signing, and peer keys;
- active audit path, including the Oracle local acceptance WAL and relay;
- healthy outbox publication into retained `vala.system.audit_log`, including
  bounded retry and retirement state;
- initialized resource governors and writable role-owned durable volumes;
- registered peer identity where a distributed role is enabled; and
- no unresolved recovery or migration condition that could make accepted work
  unsafe.

Readiness is subsystem-specific. A replica that can serve registry traffic but
cannot satisfy its enabled Scribe subsystem is not ready for Scribe traffic. The
gateway must route only to replicas ready for the requested surface.

## Readiness dependency matrix

`/readyz` reports `common`, `public-serving` when the target opens API
listeners, and one entry for each selected Bifrost runtime subsystem. The top-level
ready value is true only when every applicable entry is ready. Each entry names
the failed dependency with a stable bounded reason; liveness never substitutes
for it.

| Entry | Readiness dependencies |
|---|---|
| Common | Typed configuration, release-manifest compatibility, required Postgres role/pool, applied migration set, schema/RLS/sentinel verification, telemetry and secret-provider health |
| Public serving | Gateway/listener transport, token verifier and JWKS state, permission resolver, policy decision point, canonical audit writer, route contracts and request governors |
| Scribe | Object storage, stable node identity, WAL/staged persistent volume, resource governors, recovery reconciliation, live-tail peer identity and listener |
| Oracle | Object storage and catalog, query memory/scratch governors, persistent audit-acceptance WAL, bounded relay health, peer trust and analytical capacity |
| Forge coordinator | Object storage and Iceberg catalog, `OperatorPool`/`OperatorAudit`, durable demand/task/lease state, scheduler resources, reconciliation and cleanup cursors |
| Forge worker | Object storage and Iceberg catalog, authenticated assignment/peer trust, `OperatorPool`/`OperatorAudit`, task/lease/fence state, pod-local estimated-memory/parallelism admission, cancellation and reconciliation health |

The closed target mapping is:

| `WYRD_TARGET` | Public serving | Bifrost readiness entries |
|---|---:|---|
| `all` | yes | Scribe, Oracle, Forge coordinator, Forge worker |
| `server` | yes | Scribe, Oracle, Forge coordinator |
| `oracle` | yes | Oracle |
| `scribe` | yes | Scribe |
| `forge-worker` | no | Forge worker |

A dependency that becomes unhealthy removes readiness for every entry that
requires it. Existing accepted work follows its owner's bounded drain or
recovery state machine; readiness loss does not destroy durable evidence.
