# Implementation tasks

[Specification revision 16](spec.md) and all three supplements record the
maintainer-approved Service-only configured application model. Workloads are
Services; Agents remain component subjects. One immutable configuration follows
observations, verification and gateway work through the shared Bifrost tables.

## Telemetry setup paths

Revision 16 records the approved Wyrd-managed configuration/startup path and the
application-managed standard OpenTelemetry resource/OTLP path. The latter
requires no WyrdState or new exporter API. Both use the same validation and
canonical projection; labels never establish Card ownership. Physical defaults
are unchanged. AC-026 belongs to TASK-009 provider ownership and TASK-003 OTLP
projection/readback, with TASK-008 publishing the tested usage.

## Completed baseline and supplemental work

TASK-001 is complete. The maintainer instructs this plan to assume TASK-002 is
implemented and TASK-004 completed, including revision 13's table-version
foundation. Those assumptions do not rewrite historical review verdicts or claim
fresh tests. Their task files retain historical instructions/evidence with an
explicit revision-15 boundary. TASK-009 is the only added supplemental task.
No TASK-004 supplement is needed: the generic engine already accepts the scalar
fields, four-key sorts and complete Bloom declarations; TASK-003 owns definitions.

| Task | Current outcome / boundary | Dependencies |
| --- | --- | --- |
| [TASK-001](tasks/TASK-001-flat-card-identity.md) | Completed flat Card identity baseline | None |
| [TASK-002](tasks/TASK-002-stable-accounts-and-auth.md) | Implemented; reviewed candidate requires the bounded remediation below | TASK-001 |
| [TASK-004](tasks/TASK-004-shared-table-storage.md) | Assumed completed generic storage/table-version baseline | None |
| [TASK-009](tasks/TASK-009-configured-service-context.md) | Implemented supplemental context; reviewed candidate requires the bounded remediation below | TASK-001, TASK-002 |
| [TASK-005](tasks/TASK-005-tenant-table-configuration.md) | Proposed tenant edits over completed single-table-version engine | TASK-004 baseline |
| [TASK-003](tasks/TASK-003-system-schemas-and-attribution.md) | Proposed shared schemas, canonical production, common layouts, query/SDK closure | TASK-009, TASK-004, TASK-005 |
| [TASK-006](tasks/TASK-006-verification-trace-evidence.md) | Proposed whole-trace availability/retry | TASK-003 |
| [TASK-007](tasks/TASK-007-verification-query-performance.md) | Proposed exploration/evidence query and writer-cost measurements | TASK-003, TASK-004, TASK-005, TASK-006 |
| [TASK-008](tasks/TASK-008-docsite.md) | Final shipped-contract docs | All runtime tasks |

## Current review and next work

The cumulative TASK-002/TASK-009 candidate is `b1ff72dc3`, reviewed from
`036fc48fe`. The original revision-15 review and its source evidence remain
historical. Revision 16 resolves the external-exporter public-contract stop;
it does not claim any runtime finding fixed.

[The authority-resolution review](review/TASK-002-and-009-b1ff72dc3-rev16/verdict.md)
and [TASK-002-R1](review/TASK-002-and-009-b1ff72dc3-rev16/TASK-002-R1-execution-context.md)
carry the five retained findings. The former tenant-connection finding is
rejected: the existing isolated single-statement VerificationTargets path remains
an explicitly permitted hot-path exception. Execute that remediation with `$wyrd-implement`,
then review the complete cumulative candidate against TASK-002 and TASK-009.
Do not repeat either original implementation task or treat its evidence as PASS.

TASK-005 remains an independent outcome. After remediation and TASK-005 are
reviewed, execute TASK-003, TASK-006, TASK-007 and finally TASK-008. Task003 proves
both telemetry paths through canonical OTLP projection/readback; no new exporter
API is required. No overlapping Cargo-backed commands may share a checkout or
target directory.

## Requirement and proof ownership

| Obligations | Behavioral owner and proof |
| --- | --- |
| REQ-001–017,030,033; AC-001–005,011 | Assumed TASK-002 baseline plus TASK-009 Service-only/configure follow-up; issuer, SDK journey and accepted-work negatives. |
| REQ-018–020,029; AC-006 | Completed TASK-001 identity baseline; no new identity implementation. |
| REQ-021–028,031–032,034–036,039–040,042,044,048,051–053; AC-007–010,012,014,016,018,024 | TASK-003 exact schema/producer/admission/lifecycle readback; TASK-009 supplies configured context. |
| REQ-035–038,047–048; AC-013,021–023 | Completed TASK-004/single-version baseline and TASK-005 public edit/CAS/recovery proof; no replacement engine. |
| REQ-043,045–046; AC-017,019–020 | TASK-006 trace/availability semantics and TASK-007 measured retrieval/exploration/writer proof. |
| REQ-049–050,052–053; AC-024 | TASK-009 public context and accepted execution proof, TASK-003 all-table readback, TASK-007 exploration measurements. |
| REQ-054–056; AC-025 | TASK-009 adapter/provider-client proof, TASK-003 gateway row/refusal proof and TASK-008 tested example publication. |
| REQ-041; AC-015; all invariants | TASK-008 documentation and integrated change review over every shipped surface. |

## Stock provider SDK update

Revision 15 adds approved Python/TypeScript examples and exact automatic header
semantics in [auth](supplements/auth-system.md#stock-provider-sdk-context-and-approved-examples).
TASK-009 adds state.http_auth/httpAuth over existing adapters and current Run
context; TASK-003 validates and captures canonical gateway rows for both auth
modes; TASK-008 publishes tested examples. No new task, schema column, partition,
sort or Bloom change is required. Completed TASK-002/TASK-004 remain baseline.

## Exact revision decisions

- Service-only roots/families; no workload_kind. Subject kind stays mandatory
  in name-based selectors because Card identity includes kind.
- Six common nullable Utf8 dimensions alongside the seven universal fields on
  nine system tables; no duplicate telemetry Service/environment fields.
  Signal schemas become 65 spans / 46 logs / 58 metrics columns.
- Immutable configure uses bundle name/version and explicit environment, then
  WYRD_DEPLOYMENT_ENVIRONMENT, then null. Service/Agent Runs change subject only.
- All eight sort E/S/V/time; time-only hourly telemetry/gateway and daily
  verification/drift partitions remain. The common five attribution Blooms plus
  existing signal lookup keys produce ten on results. No new environment/kind/
  version Bloom; tenant replacement/empty sets remain supported.
- Audit retains its existing layout defaults; dynamic tables retain the universal
  envelope and existing layout defaults.
- Verification inputs/results retain accepted correlation tags unchanged across
  direct/queued work and restart. Preserve whole-trace evidence and schedule behavior.

The UI packet revision 11 consumes this platform contract; its dependent tasks
wait for TASK-009/TASK-003 rather than inventing UI-owned identity behavior.
Full language/capability regression runs once at integrated change review.
Planning modifies documents only and does not implement, commit or certify tests.
