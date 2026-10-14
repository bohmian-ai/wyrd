---
id: sealing-keyring-R1
kind: remediation
status: ready
spec: SPEC-sealing-keyring
spec_revision: 1
requirements: [REQ-007, INV-001, INV-002]
parent_task: null
remediates: [FIND-sealing-keyring-1, FIND-sealing-keyring-2]
---

# Correct the changed documentation contracts

Use `$wyrd-implement` for this bounded remediation. Implement only the independently validated corrections below.

## Subject and authority

- Approved specification: `changes/active/sealing-keyring/spec.md`, revision 1.
- Original implementation task: none; the human explicitly requested review of the specification and implemented commits only.
- Original cumulative base: `526355fb8c58dae6d67d516f98dddcd5c02b0e79`.
- Reviewed candidate: `202e5bab768cb998a01f04555c39f43b53c09929`.
- Prior verdict: `changes/active/sealing-keyring/review/r1/verdict.md`.
- Validated diagnoses: `changes/active/sealing-keyring/review/r1/findings-validation.md`.
- Nothing has shipped. There is no deployed-data compatibility or migration requirement.

Follow AGENTS.md, agent rules, the active design and security authority, and the maintainer/documentation rules. This task changes documentation only; no material behavior or architecture decision is required.

## Diagnosis and selected correction

### FIND-sealing-keyring-1: changed fallible functions omit their error contracts

`crates/wyrd/wyrd-server/src/boot/mod.rs:1321–1356` materially changes `attach_config_fields` but has no required `# Errors`. Its documentation also says “Pure/sync (no I/O)” although `GatewayConfig::credential_resolver` can synchronously load a backend CA file. The helper propagates operator-key construction failures and maps resolver/HTTP-dispatch construction failures to `ServerBootError::GatewayTransport`. Its caller, `build_state`, aborts composition on these errors. The current documentation therefore omits the actual failure contract and misidentifies the IO boundary.

`crates/wyrd/wyrd-gateway/src/managed.rs:161–170`, `tests::open`, is also materially changed and fallible, but omits `# Errors`. It delegates directly to `ManagedSecretBinding::open`, whose unavailable result covers envelopes that cannot be opened, malformed payloads and tenant/name/provider mismatch. Negative tests consume that result; their helper's contract should state it.

Add substantive error documentation to these two existing functions, using the actual lower-level owners' contracts. Correct the boot helper's no-IO assertion. Preserve both bodies, signatures and error propagation. This closes the explicit AGENTS.md §16 gate; no new runtime test or checker is justified.

### FIND-sealing-keyring-2: optional rotation guidance skips reader preparation

The newly added sealing-key configuration row at `docs/src/content/docs/operate/deploy-wyrd-for-a-team.md:72` instructs operators to install a new write key and retain the old key, then rely on startup rewrap. It omits the existing keyring prerequisite: every serving replica must already hold the next key as a reader before any writer uses it.

Two K1-only replicas expose the consequence. Restarting the first with K2-write/K1-retained immediately rewraps shared database secrets under K2 before readiness. The other replica still holds its boot-loaded K1-only ring; file replacement does not reload it. Fresh calls on that replica cannot open the new ciphertext and fail before provider dispatch. Sibling stored identity secrets use the same rewrap owner. This is an introduced inaccurate instruction, not a runtime cipher defect or a released-format upgrade concern.

Delete only the optional sentence beginning “To rotate” and ending “at startup” from the new configuration row. Keep the configuration settings, common-replica key requirement, purpose and missing-key behavior. Deletion is sufficient because the approved task does not require a new rotation runbook. Do not add a runbook, key reload, rollout coordination or readiness behavior; the existing keyring already supports safe rotation.

## Preserved behavior and non-goals

Preserve deployment-key seal/open, authenticated tenant/name/provider binding, existing boot rewrap and CAS, local key provisioning/persistence, environment precedence, redaction and all test assertions. No executable Rust, SQL, schema, dependency, fixture or public API changes are required. Do not implement compatibility, refactor owners, expand older guidance, or incorporate unrelated placement/style notes. Batch both findings in this correction rather than creating a separate style-only round.

## Acceptance and focused proof

| Finding | Required outcome | Proof |
|---|---|---|
| FIND-sealing-keyring-1 | Both changed fallible functions have accurate `# Errors`; boot documentation acknowledges its actual composition/CA-loading behavior. | Read each complete rustdoc block against its unchanged body and existing lower-level contracts. |
| FIND-sealing-keyring-2 | The optional incomplete rotation sentence is absent; the remaining row correctly explains configuration and credential protection. | Review the final row and verify the diff deletes the recipe without changing runtime semantics. |

This is verification-only prose correction: no executable scenario is introduced, so no Red/Green runtime test or new harness is needed. Run only the narrow checks covering this write set:

```bash
mise run fmt:check
mise run docs:check
git diff --check
```

Record exact results and the final diff. Do not run full journey suites or broad aggregates for this remediation. Full lints, codegen and required journey acceptance remain obligations for final change verification; this task does not claim those already passed. A later review must reassess the complete cumulative implementation against the original specification, preserving both finding IDs.

## Implementation evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-sealing-keyring-1: `attach_config_fields` documents `# Errors` and its real IO | `crates/wyrd/wyrd-server/src/boot/mod.rs` rustdoc: drops "Pure/sync (no I/O)", names the CA-bundle reads, `ServerBootError::OperatorKeys` (from `OperatorKeys::for_role`/`KeyError` via `#[from]`) and `ServerBootError::GatewayTransport` (resolver CA/backend-client failures per `GatewayConfig::credential_resolver`; `HttpProviderDispatch::new` transport failure); body unchanged | Read against body and lower-level `# Errors` contracts; `mise run fmt:check` rc=0 | PASS |
| FIND-sealing-keyring-1: `tests::open` documents `# Errors` | `crates/wyrd/wyrd-gateway/src/managed.rs` rustdoc mirrors `ManagedSecretBinding::open` `Unavailable` contract; body unchanged | Read against delegate | PASS |
| FIND-sealing-keyring-2: incomplete rotation sentence removed | `docs/src/content/docs/operate/deploy-wyrd-for-a-team.md` sealing-key row: only the "To rotate … at startup" sentence deleted | `mise run docs:check` rc=0; `git diff --check` clean | PASS |

Commit: `81203db8e`. Non-goals held: no executable Rust, SQL, schema, dependency, fixture, test-assertion or public API change; no runbook, reload or readiness behavior added. Lints, codegen and journeys remain final change-verification obligations and were not run here.

Status: `IMPLEMENTED`.
