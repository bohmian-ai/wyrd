# TASK-004 R2 Security, Tenancy, and Audit Domain Review

## Result

**PASS**

No material security, tenancy, or audit finding remains in the immutable
base-to-candidate subject. The remediation closes prior
`FIND-TASK-004-4`: reserved verification-result writes now require the exact
signed Verifier scope inside Gate's one canonical authorization decision,
before the decision is audited and before Scribe receives the frame.

## Reviewed boundary

- Base: `9431906eeb1c7b67a0efcec09487fd1d848f70a8`
- Candidate: `2af4cc3ff95a609d1df682be4f633345f96934e1`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 33
- Original task: `changes/active/verified-change-contract/tasks/TASK-004-generic-verification-runtime-and-results.md`
- Prior verdict and ledger: `changes/active/verified-change-contract/review/TASK-004-r1/{verdict,findings-validation}.md`
- Remediation task: `changes/active/verified-change-contract/review/TASK-004-r1/TASK-004-R1-close-validated-runtime-gaps.md`

The review traced tenant SYSTEM provisioning and public exclusion, internal
token issuance and verification, native gRPC authentication, Gate's closed
principal/table matrix, per-row Verifier-scope admission, canonical audit
cardinality, Scribe's repeated scope check and trusted UID stamping, and the
role-separated result/detail identity proof. It also checked the cumulative
diff for regressions in those boundaries.

## Authority and source coverage

| Boundary | Governing authority | Source and proof inspected | Result |
|---|---|---|---|
| Tenant identity and SYSTEM lifecycle | `AGENTS.md` §§2, 9; `architecture/wyrd-design.md` identity doctrine; `architecture/wyrd-security-posture.md` principal lifecycle; `REQ-086`, `AC-023` | `20260601000028_system_principal.sql`; `wyrd-sql` service-account queries; provisioning; issuer and verifier; SQL, issuance, and public-route tests | PASS |
| Tenant binding and token verification | Security posture trust boundaries; `REQ-086`, `INV-007` | `gate/auth.rs` derives the expected tenant from the untrusted payload only as verifier input, then verifies the signed tenant; SYSTEM claims require UUIDv7 identity, exact permission, no role/credential/root/delegation, and one UID-bearing Verifier scope | PASS |
| Reserved-table authorization | `architecture/bifrost-design.md`; `architecture/references/domain/olap-serving.md`; `REQ-086`, `REQ-119`, `REQ-121`, `AC-023` | `Gate::authorize_record_write`, `is_verification_result_table`, `require_frame_card_scope`, and Scribe `require_card_scope`/`validate_card_scope`/`resolve_card_uids` | PASS |
| Canonical audit cardinality | `AGENTS.md` §§2, 9; `architecture/agent-rules.md`; security posture audit rules; `AC-030` | Gate computes permission, kind, reserved-table, and exact-scope outcome once; `PostgresGateAudit` commits exactly one allowed or denied `bifrost.record.write` event before admission/refusal; Scribe adds no authorization audit | PASS |
| Durable attribution and tenant isolation | Bifrost managed-envelope and tenant rules; `REQ-119`, `REQ-121`, `AC-023` | Scribe derives tenant and principal from verified authority, resolves `card_ref` against signed identity, and stamps the signed member UID; gRPC WAL proof and role-separated Oracle journey assert SYSTEM/Verifier/subject/owner/binding/run identities | PASS |

## Prior finding closure

### `FIND-TASK-004-4` — CLOSED

- **Violated obligation previously observed:** a SYSTEM result frame missing or
  nulling `card_ref` could receive an allowed Gate audit before Scribe admitted
  unattributed rows or refused a foreign value.
- **Correction evidence:**
  `crates/vala/vala-bifrost-redux/src/gate/mod.rs:457-500,895-1000` passes the
  native frame into the single combined Gate decision. For the SYSTEM/result
  cell, it decodes every Arrow batch and calls Scribe's shared mandatory check
  before selecting `AuditOutcome`. `execution_lanes.rs:755-788` requires exactly
  one `card_ref` field, no null row, UTF-8 values, successful parsing, and scope
  authorization. Scribe repeats optional scope validation and stamps only the
  UID from the verified signed scope.
- **Observable closure:** absent, null, duplicate, malformed, foreign, empty,
  and undecodable frames never reach Scribe and record one denial; an exact
  scope records one allow. The real gRPC proof observes four denied decisions,
  one allowed decision, and exactly one durable WAL row stamped with the signed
  Verifier UID.
- **Focused proof rerun:**
  `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=gate::tests::gate_denies_system_result_writes_outside_the_exact_card_scope) | test(=gate::tests::gate_confines_verification_result_tables_to_the_system_writer)'`
  passed 2/2 on the reviewed candidate.

## Security Audit

### Critical

None.

### High

None.

### Medium

None.

### Low / Defense In Depth

None required by the approved task.

### Positive Controls

- SYSTEM tokens are short-lived normal tenant tokens with a closed claim shape
  and cannot be refreshed, delegated, credentialed, role-bearing, or used as a
  public principal.
- Gate reserves the three result tables to SYSTEM and denies SYSTEM every other
  table, including denial of wildcard administrators on reserved tables.
- Exact Verifier correlation is checked before the canonical audit append and
  repeated by Scribe before trusted UID stamping.
- Audit failure fails closed; allowed and denied decisions use the single
  tenant-scoped staging path, while issuance and engine mechanics emit no
  duplicate authorization audit.
- Tenant and publisher identity come only from the verified token; Arrow bytes
  cannot select tenant, `principal_id`, or managed `card_uid`.

## Verification limits

- This reviewer reran the two focused Gate unit tests and inspected the complete
  cumulative diff and supplied implementation evidence. It did not rerun the
  Postgres-backed gRPC test, the role-separated ignored journey, or the full
  repository verification matrix.
- The supplied implementation evidence records the Postgres gRPC scope/audit/WAL
  proof, role-separated identity journey, principal integration lanes, tenant
  isolation check, Vala/Bifrost/server lanes, formatting, linting, codegen, and
  `git diff --check` as passing on the candidate.
- `git diff --check` was independently rerun for the immutable base-to-candidate
  range and passed. `HEAD` remained
  `2af4cc3ff95a609d1df682be4f633345f96934e1` throughout this review.

## Material findings

None.
