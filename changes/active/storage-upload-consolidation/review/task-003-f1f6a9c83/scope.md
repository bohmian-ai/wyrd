# TASK-003 first review

- Root: `/Users/stevenforrester/Documents/GitHub/wyrd`.
- Base: `3a99eb05fd5ea78a21fd732212bf2c5d3672f27f`.
- Candidate: `f1f6a9c833061ec6f0a781247b762ec3c0de4bea`.
- Authority: `changes/active/storage-upload-consolidation/spec.md`, approved revision 2.
- Original task: `changes/active/storage-upload-consolidation/tasks/TASK-003-canonical-evidence.md`.
- Round: first review, complete `git diff <base>...<candidate>`; no prior TASK-003 acceptance carried forward.
- Source remains immutable; only review reports may be written. No implementation, commit, push, or deployment.
- No `.codegraph/` directory exists at the repository root; use normal repository navigation.

## Starting navigation map

This map is a starting point, not complete coverage or a conclusion.

| Changed owner | Callers / consumers to trace | Proof owners |
|---|---|---|
| `verification/hosted/evidence.rs::TenantEvidence` metadata memo, inventory, file_object, admit, read, select | hosted execution, workspace staging, citations, direct service and queued runner | `pg_hosted_artifacts`, `pg_hosted_files`, `pg_verification_runtime` |
| `components/cards/routes.rs::{get_card_by_ref_for,get_card_objects_by_ref_for,authorize_card_ref}` and `service.rs::{get_card_objects_by_ref,card_files,hydrate_card}` | all exact Card reads and Artifact metadata consumers, authorization and audit | Card regressions and hosted Artifact tests |
| `components/storage/inventory.rs::ObjectInventory::{executed_by,query}` | File lookup and Card hydration, Caller identity producers | hosted inventory/authorization tests |
| Canonical readback tests in Rust/Python/TypeScript/MCP and `pg_verification_routes` | unchanged `InputResolver`, verification service, groups and runner | V3–V7 and existing runtime/group proof |
| Verifier evidence authority paragraph | architecture consistency and public failure contracts | approved spec and focused behavior evidence |

## Selected reviewers

- `behavior-rev`: comprehensive task acceptance, metadata ownership/reuse, caller paths, invariants, verification sufficiency, and comparison to existing owners.
- `repo-rev`: cross-tier Rust/Python/TypeScript/MCP test and documentation rules, changed Rust-item rustdoc, SQL capabilities and mandatory gates.
- `system-rev`: materially changed admission failure propagation and bounded direct/queued readiness/recovery.
- `domain-rev-security`: changed executor identity reuse and repeated Card authorization/audit boundaries, tenant scope.
- No separate invariant reviewer: state is execution-local memoization; behavior and system reviewers cover lifecycle effects.
- No separate maintainer reviewer: no substantial new abstraction/ownership hierarchy; behavior reviewer covers existing-owner reuse and maintainability.
- Fresh `ponytail-rev` after discovery: independently validate every proposal and missing proof, deduplicate and recommend final ledger.

## Evidence handling

Review the actual Git range and authoritative original task before reading its appended Implementation Evidence section. That section supplies available results, not independent conclusions. No implementation completion summary is supplied to discovery reviewers. Benchmark methodology lives in `benchmarks/README.md` and the existing capacity verification owner. Reviewers must distinguish invalid/no-work runs from measured performance. Do not run simultaneous Cargo or benchmark workloads. Full language suites are reserved for change review; carry credible recorded checks and inspect focused gaps.
