# TASK-002-R1 remediation review subject

Repository `/Users/stevenforrester/Documents/GitHub/wyrd`.
Cumulative base `451aa0c4d332af3bf8f334ea9eb1bc9d0679a34a`.
Prior reviewed candidate `c8cbfeeb0d5cf3a4fa63bb386cbf460f009ead24`.
Candidate `52f69ed2da3846733be0d314c6627c95d308e8dd`.
Initial working tree clean, HEAD equals candidate; no `.codegraph/` directory.
Read `git diff c8cbfeeb0 52f69ed2d`; cumulative diff remains available.

Contract: changes/active/stored-secret-encryption/spec.md, approved revision 1;
original tasks/TASK-002-standardize-secret-owners.md.
Prior verdict/findings and original remediation:
review/task-002-c8cbfeeb0/{verdict.md,findings-validation.md,
TASK-002-R1-secret-owner-binding-and-runtime-safety.md}.
The R1 task now records RED/GREEN assertions, commands and handoff at this candidate.
Review original task and approved requirements independently; carry unchanged
source-backed acceptance forward, reopening only specifically affected paths.
No rebase: direct ancestry, five new commits. Keep reviewed source immutable.

Read root AGENTS, nested SQL AGENTS, changes/spec-driven-development.md,
agent_docs/index.md and relevant rust/errors/maintainer guides, TESTING.md.
Relevant architecture: security posture stored-secret/tenant rules; Wyrd Operator;
deployment configuration and recovery authority. Review reports only are writable.

## Navigation and assignments

Fresh ponytail validator owns prior finding 1 auth connection binding including
candidate staging, callback decode, SQL maintenance projection and candidate CAS;
prior finding 2 per-row Operator rewrap progress/deadlines/cancellation/fences;
prior finding 4 lost replacement fence/conflict and downstream credential delivery;
all changed-path rules, proof strength and historical evidence reconciliation.
It also validates system report and checks unaffected acceptance carry-forward.
Exact report: findings-validation.md in this directory.

System specialist owns materially changed Gateway failure propagation: new pending
credential source, resolver/error class, billable/admission/breaker/fallback effects,
HTTP 503 mapping, same-process recovery, unrelated resources, snapshot lifetimes,
and added/moved external wait audit. Source map: wyrd-gateway src credential.rs,
engine.rs, snapshot.rs; server components/gateway service.rs/invocation.rs;
wyrd-testing src/kms.rs and tests/gateway/compatible.rs; error catalog.
Exact report: task-review-system.md in this directory. New findings need a separate
focused independent source check before retention; notify root.

## Explicit user addition for next remediation

User requires shared credential resolution for GCP storage and KMS, including
GOOGLE_ACCOUNT_JSON_BASE64 and GOOGLE_APPLICATION_CREDENTIALS_JSON. Pending note:
review/task-002-c8cbfeeb0/gcp-credential-parity-addendum.md.
Inspect current storage factory/mod.rs::service_account_json, cloud.rs::gcs and
kms gcp.rs::new. Record this as a USER-REQUESTED SCOPE ADDITION, not FIND-5 or
an independently discovered defect. Scope includes existing input/precedence,
shared implementation at a suitable existing dependency owner, both callers and
focused proof/docs. No new auth flow, dependencies or schema authorized. Root
will write the cohesive next remediation task through this invoked review.

## Execution and process

Root coordinates all Cargo commands; do not overlap builds/tests/lints/codegen.
Reviewers inspect source/assertions/recorded results and request any needed execution.
Root will rerun focused closure tests and address the specifically noted Operator
suite freshness after assertion-only correction, without broad aggregates.
Original TASK-002 scenarios 2/3 TDD violations remain; R1 claims TDD PASS all four.
Investigate the recorded JSON-byte assertion correction as a diagnosis claim,
not automatic evidence of a production defect.
User again accepts live AWS/GCP and Linux-only production startup as CI limits.
Root AGENTS and spec withdraw automatic benchmark requirements absent explicit
current request; enumerate hot-path waits, do not require a benchmark.
