# Data, Tenancy, Transaction, and Bifrost Audit Review

## Reviewed Boundary

Fresh review of candidate `96a1bd81b1d028fa81d6e0f385e3cd9b62080e30`
against base `c5c20754a167e8f4d74a555a720bd51df6179a6f`, scoped to
tenant isolation, persistent state, transaction ownership, provisioning and
initialization durability, canonical audit staging/publication, and the
retained `vala.system.audit_log` credential evolution.

The approved authority was `SPEC-admin-principals` revision 10 at SHA-256
`05982825655110b7a514f16ffdd953e4b1e1df4b7422e9e77461f2e514a10837`, together
with all eight original task packets. I also re-reviewed the whole-branch-03
verdict, its R3 remediation packet, and the appended implementation evidence.
I did not inspect other whole-branch-04 Wave 1 reports.

## Authority and Source Coverage

- `AGENTS.md`, `architecture/agent-rules.md`,
  `architecture/v1/00-foundations/tenancy.md`,
  `architecture/v1/00-foundations/service-identity.md`,
  `architecture/wyrd-security-posture.md`, and
  `architecture/bifrost-design.md`.
- The complete candidate diff was inventoried. Detailed review followed the
  changed SQL migrations and query owners, `TenantConn`, `OperatorPool`,
  initialization, provisioning, recovery, tenant admission, platform and
  tenant authorization, audit append/staging/publication, Bifrost catalog
  registration/evolution, audit projection/schema, and their reachable test
  callers.
- The `TenantConn`/`OperatorPool` remediation is correctly narrowed: live
  provisioning and tenant-admin recovery own `OperatorPool`, acquire the
  tenant-bound transaction for the exact target, and leave transaction commit
  to their workflow owner. Same-plane allowed decisions and effects reviewed in
  this boundary share the audited transaction.
- The canonical audit path remains singular: authorization decisions append to
  `vala.audit_staging`; publication freezes a tenant range, derives a stable
  batch identity, uses Scribe, and advances the watermark while retiring the
  staged prefix. No second audit sink or publisher was found.
- Migration 27 adds nullable `credential_id`; the hash encoder preserves the
  exact legacy preimage when it is absent. The catalog recognizes the exact
  prior audit fingerprint, appends one nullable Iceberg column idempotently,
  preserves legacy field IDs, and reconciles physical-new/control-old retry
  state. The remaining defects are below.

## Review Findings

### Critical

None.

### Important

- **`DATA-04-1` — INCORRECT / durability** —
  [`crates/wyrd/wyrd-server/src/boot/init.rs:109`](../../../../../crates/wyrd/wyrd-server/src/boot/init.rs#L109),
  [`crates/wyrd/wyrd-server/src/boot/init.rs:145`](../../../../../crates/wyrd/wyrd-server/src/boot/init.rs#L145),
  [`crates/wyrd/wyrd-server/src/main.rs:83`](../../../../../crates/wyrd/wyrd-server/src/main.rs#L83),
  [`crates/wyrd/wyrd-server/src/main.rs:89`](../../../../../crates/wyrd/wyrd-server/src/main.rs#L89).
  **Violated obligation:** `REQ-021`, `REQ-023`, and TASK-003 require one
  transactional initialization whose credential plaintext is exposed once, and
  require any failed initialization to leave the deployment uninitialized and
  retryable rather than retaining a credential whose plaintext was never
  exposed. **Evidence:** `initialize_platform_root` commits the principal,
  grant, and verifier at line 145 and returns the secret. Only afterward does
  `main::init` attempt four infallible `println!` calls; the first two occur
  before the credential line. A closed or failed stdout therefore terminates
  the command after the database commit and before disclosure. The unique root
  then makes the next `init` return `AlreadyInitialized`. **Consequence:** a
  failed operator command can leave a live, undisclosed root credential and a
  deployment that cannot retry the promised initialization path. The separate
  recovery command does not make this satisfy the approved initialization
  invariant and intentionally leaves the undisclosed credential live.
  **Testable correction:** make terminal delivery an explicit fallible writer
  step owned by the initialization workflow and do not commit the initialization
  transaction until the credential output has been successfully written and
  flushed; on output failure, roll back without persisting the secret anywhere.
  Add a failing-writer test that proves no root principal, grant, or credential
  commits and that a subsequent initialization succeeds.

- **`DATA-04-2` — REGRESSION / Bifrost schema validation** —
  [`crates/vala/vala-bifrost-redux/src/catalog/bifrost_catalog.rs:1156`](../../../../../crates/vala/vala-bifrost-redux/src/catalog/bifrost_catalog.rs#L1156),
  [`crates/vala/vala-bifrost-redux/src/catalog/bifrost_catalog.rs:1490`](../../../../../crates/vala/vala-bifrost-redux/src/catalog/bifrost_catalog.rs#L1490),
  [`crates/vala/vala-bifrost-redux/src/catalog/bifrost_catalog.rs:1513`](../../../../../crates/vala/vala-bifrost-redux/src/catalog/bifrost_catalog.rs#L1513).
  **Violated obligation:** the R3-5 remediation permits only the exact legacy
  `audit_log` additive evolution, requires every unrelated physical mismatch to
  remain a refusal, and expressly forbids widening generic built-in evolution;
  the catalog's own contract says field order remains exact. **Evidence:** every
  existing physical table is validated through `schema_shape_matches` at line
  1176. When positional comparison fails, the new generic fallback indexes all
  top-level fields by stable ID and accepts them in any order. This is reachable
  beyond `audit_log`: canonical log and trace schemas assign stable IDs to every
  top-level payload and envelope field. Consequently an arbitrarily reordered
  `vala.logs.records` or `vala.traces.spans` physical schema with unchanged
  names, types, nullability, and IDs now passes validation even though it is not
  the canonical ordered physical recipe. **Consequence:** reconciliation no
  longer fails closed on top-level order drift for unrelated canonical tables,
  weakening the catalog guard that prevents a control registration from
  silently accepting altered physical state. **Testable correction:** restore
  strict positional comparison as the generic validator. Permit the one
  evolved `audit_log` ordering only at the already fingerprint-gated audit
  migration seam, checking the exact expected legacy IDs and the one appended
  credential field. Add a regression test that swaps two stable-ID fields in a
  non-audit canonical table and requires `MetadataMismatch`, while both the
  fresh and exact evolved audit layouts still validate.

- **`DATA-04-3` — MISSING / retained-history compatibility proof** —
  [`changes/active/admin-principals/review/whole-branch-03/TASK-001-008-R3-close-validated-findings.md:268`](../whole-branch-03/TASK-001-008-R3-close-validated-findings.md#L268),
  [`crates/vala/vala-bifrost-redux/src/catalog/bifrost_catalog.rs:2293`](../../../../../crates/vala/vala-bifrost-redux/src/catalog/bifrost_catalog.rs#L2293),
  [`crates/wyrd/wyrd-testing/tests/bifrost/server/audit_publication.rs:798`](../../../../../crates/wyrd/wyrd-testing/tests/bifrost/server/audit_publication.rs#L798).
  **Violated obligation:** the accepted R3-5 correction requires one proof that
  seeds the exact pre-change physical table, registration, staging row, chain
  head, and retained row; upgrades; verifies the legacy hash; publishes both
  credential shapes; restarts/replays; and reads uninterrupted history with old
  field IDs. This is also the user-journey durability proof required by the
  repository test taxonomy. **Evidence:** the catalog test creates only an
  empty legacy physical table/control row, evolves it, checks IDs, and simulates
  physical-new/control-old reconciliation. The publication journey starts a
  fresh current-schema server and publishes two newly staged rows; it seeds no
  pre-change retained object or audit-chain state and performs no restart or
  replay. The appended evidence runs those two tests but does not supply the
  required combined fixture. **Consequence:** the green evidence can pass while
  old Parquet data is unreadable after evolution, legacy chain continuity is
  broken, or a retry across restart duplicates or loses the frozen publication
  range. Those are the compatibility risks the prior validated finding required
  the candidate to close. **Testable correction:** add the single focused
  Postgres/object-store integration journey specified by R3-5, using the
  existing catalog and publisher fixtures: seed an old-schema retained row and
  matching old control/staging/head state, boot the new code, assert legacy
  hash and IDs, publish null/non-null credential rows, interrupt after durable
  publication but before settlement, restart, replay, and query each historical
  and new row exactly once.

- **`DATA-04-4` — MISSING / provisioning transaction proof** —
  [`changes/active/admin-principals/tasks/TASK-004-tenant-provisioning.md:76`](../../tasks/TASK-004-tenant-provisioning.md#L76),
  [`changes/active/admin-principals/spec.md:614`](../../spec.md#L614),
  [`crates/wyrd/wyrd-server/tests/platform_admin_e2e.rs:3003`](../../../../../crates/wyrd/wyrd-server/tests/platform_admin_e2e.rs#L3003),
  [`crates/wyrd/wyrd-server/tests/platform_admin_e2e.rs:3051`](../../../../../crates/wyrd/wyrd-server/tests/platform_admin_e2e.rs#L3051).
  **Violated obligation:** `AC-007` and TASK-004 require injected failure at
  each provisioning stage, proving no usable tenant and successful retry to one
  tenant/admin. **Evidence:** the sole real stage-failure journey installs one
  trigger on `wyrd.auth_service_accounts`, so it tests failure while inserting
  the administrative principal. It does not inject failures at the directory
  claim/audit transaction, builtin-role seed, administrative grant, credential
  insert, tenant transaction commit, or final active promotion. The abandoned
  row and concurrent-slug cases exercise adoption and serialization, not those
  transaction failure boundaries. **Consequence:** rollback and retry behavior
  at the other durable boundaries remains unproved; a partial grant/credential
  or failed promotion regression can ship while the named acceptance journey
  remains green. **Testable correction:** parameterize the existing Postgres
  journey over the actual durable provisioning stages and inject a failure at
  each owner boundary. For every case assert failed/non-admitted directory
  state, no usable credential, then retry and assert the original tenant ID,
  exactly one tenant-admin principal and grant, exactly one usable credential,
  and no live orphan.

### Suggestions

None.

## Open Questions

None.

## Verification Notes

- This was a static, review-only audit; no candidate source was modified and no
  test or build command was rerun.
- I reviewed the appended candidate evidence for the focused catalog upgrade,
  audit-staging hash tests, current-schema retained publication journey, and
  broad Bifrost/SQL suites. Those results support the implementation portions
  described above but do not substitute for `DATA-04-3` or `DATA-04-4`.
- Spec SHA-256 was rechecked as
  `05982825655110b7a514f16ffdd953e4b1e1df4b7422e9e77461f2e514a10837`.
- Candidate HEAD was rechecked as
  `96a1bd81b1d028fa81d6e0f385e3cd9b62080e30` before report creation and again
  after it; pre-existing dirty `README.md`, spec, and whole-branch-03 verdict
  edits were not touched.

## Overall Result

**FAIL**

The candidate closes the prior narrow SQL ownership, audit attribution/hash,
and exact legacy-fingerprint recognition defects, but initialization still has
a committed-secret disclosure gap, the audit fix weakens physical validation
for unrelated canonical tables, and the two required durability proofs are not
present.
