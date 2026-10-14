# Independent behavior review

Result: FAIL — one bounded documentation-rule violation; no functional defect found.

## Immutable subject and scope

Base `526355fb8c58dae6d67d516f98dddcd5c02b0e79`; candidate
`202e5bab768cb998a01f04555c39f43b53c09929` (HEAD remained equal at completion).
Authority is approved `changes/active/sealing-keyring/spec.md` revision 1.
There is no original task, explicitly waived by the human. Nothing has shipped:
no compatibility or deployed-data migration requirement is inferred.

Reviewed the complete cumulative diff, applicable AGENTS and agent rules,
spec-driven-development and maintainer guides, design, security posture, testing
workflow, and relevant surrounding callers. CodeGraph was used before source
navigation. Coverage includes CLI installation/provisioning and child lifetime,
server boot/config/state, administration writes and read views, admission
snapshots, resolver, SQL persistence and rewrap, changed harness and tests,
schema documentation, manifests, and changed user documentation.

## Acceptance matrix

`PASS` below means source satisfies the obligation; execution evidence is
identified separately and is never inferred from the presence of a test.

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-007: one deployment keyring seals and opens managed credentials | `service.rs:69,774` obtains `state.auth.sealing_key`; `boot/mod.rs:1331-1346` supplies that same state keyring to the resolver; `managed.rs:98-143` delegates encryption/decryption to `SealingKeyring` | Changed administration and invocation tests exercise stored envelopes and real dispatch ownership; not executed in this review | PASS |
| REQ-007: envelope binds tenant, name, and provider | `managed.rs:99-108` includes all three in the authenticated payload; `managed.rs:137-142` checks all three before returning the secret; resolver uses verified invocation tenant and admitted credential identity | `sealed_values_open_only_for_their_own_binding`; administration copied-name test; invocation copied-tenant test | PASS |
| REQ-007: retained keys and same boot rewrap | `human_connections.rs:436-492` extends the existing closed table inventory and exact-byte CAS; `sealing.rs:85-96` walks that inventory; ordinary `build_state` calls rewrap at `boot/mod.rs:1207` | Unit rotation test plus integration test calling production boot rewrap and then invoking using only the replacement key | PASS |
| REQ-007: remove separate Gateway keyring, no compatibility | Removed config map, tenant keyring types/state, per-tenant loader, snapshot components and fixture key provisioning; new migration drops obsolete nonce/version columns | Search of crates, SDKs and docs finds no remaining removed keyring symbols/configuration; SQL source retains source/ciphertext-presence constraint | PASS |
| REQ-007: dev provisions key before first boot and reuses it | `dev.rs:93` provisions before `DevServer::run`; `local_sealing_key` respects explicit file/inline environment, generates 32 random bytes, stages owner-only file, syncs it, persists without replacement; `command` passes the path to children | New CLI journey starts a fresh database and installed real binary without sealing env, writes credential via stdin, invokes provider, stops, restarts and invokes again | PASS |
| REQ-007: quickstart credential form succeeds without restart | CLI journey uses the quickstart managed-secret JSON form and real credential/deployment CLI commands against the first running server | `server_dev_keeps_the_gateway_managed_secret_key`, registered in existing local-dev lane; execution unavailable | PASS |
| INV-001: tenant isolation | Administration obtains tenant connection through existing audited owner; binding takes `caller.data_tenant_id`; snapshots and credential rows remain RLS scoped; privileged rewrap CAS identifies tenant and row | Supporting `check:tenant-isolation` PASS; existing HTTP journey covers unauthorized write/read and another tenant; changed integration tests exercise copied tenant envelope | PASS |
| INV-002: keys and secrets never logged or returned | Managed plaintext remains wrapped in redacted `SecretString`; temporary encoded plaintext is zeroized; responses use source views, not secret write payloads; CLI prints no generated key; error messages are static or secret-free | Redacted payload unit assertion; stored-row/view assertions; CLI output sentinel assertion; execution unavailable | PASS |
| AC-001: required journey exists at actual defect boundary | New CLI journey boots the ordinary server instead of injecting keyring state. Existing Rust/HTTP Gateway journey and Python/TypeScript CLI-helper journeys exercise shipped management surface; no new SDK write API was invented | Source coverage present. Historical failing-base/passing-candidate evidence not supplied | PASS for test presence; historical execution unproven |
| AC-003: format, lints, codegen and owning lanes pass | Existing mise lane includes new CLI test | `verification.md`: fmt:check and tenant-isolation PASS; dependency check in progress when inspected; lints, codegen, docs and journeys not executed | FAIL to establish full execution obligation; evidence limit, not a source defect |
| Repository hard rustdoc constraint | Materially changed test helper `managed.rs:163` returns `Result` without required `# Errors` | Direct source inspection; BH-001 | FAIL |

## Caller-to-result and reuse assessment

The write path remains CLI/HTTP ingress → existing audited administration →
tenant connection → shared `ManagedSecretBinding::seal` → one ciphertext column
→ redacted view. A failed seal precedes the SQL write, preserving the previous
value. Environment and external-secret sources keep their existing assignment
checks and do not require the deployment keyring.

The invocation path remains authenticated call → tenant admission snapshot →
existing deployment planner → `CredentialResolver::resolve` → binding check →
provider secret. No alternate fallback source is introduced when opening fails.
Revoked, unavailable, absent-key and mismatched values fail before dispatch.

The existing alternative to the removed custom keyring is precisely the
deployment `SealingKeyring` and `SealedSecretRewrap`. Extending their existing
table inventory supplies the missing Gateway coverage without duplicating
cryptography, rotation orchestration, SQL authority or lifecycle ownership.
The remaining binding value is necessary because the common key alone cannot
prove row identity. The CLI addition owns no second server lifecycle: it
provisions a file and supplies it to the existing child-command owner.

The new migration retains the existing independent CHECK tying
`source = managed_secret` to non-null ciphertext (original Gateway migration
line 45); dropping the obsolete column-specific checks does not remove that
invariant. No legacy-data conversion is needed or requested.

## Proposed material finding

### BH-001 — VIOLATION: changed fallible helper omits required error contract

- Obligation: AGENTS.md §16 and agent-rules require every materially changed
  fallible Rust function, including test helpers, to contain substantive
  `# Errors`; the task-review skill keeps missing rustdoc blocking.
- Location: `crates/wyrd/wyrd-gateway/src/managed.rs:161-170`, `tests::open`.
- Evidence: the cumulative diff changes this helper's keyring/binding signature
  and implementation to use `ManagedSecretBinding::open`. Its only rustdoc
  explains exposing plaintext for comparison and contains no `# Errors`.
- Consequence: this is a hard repository documentation-gate violation, not a
  runtime security or functional failure. The rejection cases central to the
  negative tests are omitted from the helper's error contract.
- Smallest correction: add a concise `# Errors` documenting that the helper
  propagates `CredentialError::Unavailable` when the key cannot open the
  envelope, the payload is malformed, or the binding differs. Reuse the actual
  `ManagedSecretBinding::open` contract; change no executable behavior.
- Closure proof: source review of the helper's complete rustdoc is sufficient;
  no new test or harness is necessary. Batch this with any other confirmed
  corrections, rather than creating a separate style round.

No independently established functional, security, tenant-isolation, rotation,
compatibility or speculative-hardening finding is proposed.

## Verification limits

I ran no Cargo commands or test suites, preserving shared build ownership.
Supporting execution evidence is only that recorded in `verification.md`.
Unavailable historical Red/Green evidence does not establish a code defect and
does not justify checking out an old baseline. Full AC-003 acceptance cannot be
claimed from this static audit; the orchestrator must retain that limit in the
verdict. No broad suites or source edits were performed.
