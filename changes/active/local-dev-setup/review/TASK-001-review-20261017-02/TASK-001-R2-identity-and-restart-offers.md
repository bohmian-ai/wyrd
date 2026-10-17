---
id: TASK-001-R2
kind: remediation
status: proposed
spec: SPEC-local-dev-setup
spec_revision: 1
parent_task: TASK-001
remediates: [FIND-TASK-001-2, FIND-TASK-001-7, FIND-TASK-001-9]
depends_on: []
---

# Finish saved-identity preflight and suppress repeated restart offers

## Authority, subject and intended outcome

Execute directly with `$wyrd-implement`. Approved authority is
`changes/active/local-dev-setup/spec.md` revision 1 and original task
`changes/active/local-dev-setup/tasks/TASK-001-guided-local-development.md`.

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/local-dev-setup`.
Original cumulative base: `bc3bf25e3f78b0c429110350a3de4d58290e632f`.
R1 review candidate: `38dbc023cd71292e1badd3abe90e87c69b76fa96`.
Prior candidate: `5cfb0a3ac61434256fa97a86f3506b124158780c`.

Current review/validated ledger:
`changes/active/local-dev-setup/review/TASK-001-review-20261017-02/{verdict,findings-validation}.md`.
Prior verdict, findings and R1 task are in
`changes/active/local-dev-setup/review/TASK-001-review-20261017-01/`.
Read original task and both remediation rounds together. Review scope remains
implemented changes in this worktree; sibling repositories are excluded.

Preserve the six source-closed prior findings. Complete the existing startup
identity preflight without adopting an unrelated saved client or substituting
a human login for the exact retained key; resume the same deployment after a
listener override without automatic setup offers; batch the remaining
mandatory rustdoc correction. No specification revision is needed.

## FIND-TASK-001-2 — identity proof still accepts another credential or selection

**Diagnosis and consequence.** REQ-002/008 and INV-003/004 require preservation
before selection replacement or one-time issuance. In
`crates/wyrd/wyrd-cli/src/server/dev.rs:307-327`, the tenant guard needs an
existing `[dev].tenant`; without it, the new guard compares origin only. At
`:535-551`, preflight calls `saved_client`, then writes settings and provisions.
That client's ordinary resolver (`crates/shared/wyrd-client/src/config.rs:221-249`)
selects a saved human login before the default key.

Two supported starting states still fail:

- A same-origin default key for B with no dev selection authenticates B while
  startup requests absent A. Settings are saved for A; with legitimate
  platform authority setup creates A and issues its key. The newly issued A
  key passes its own UUID-prefix check, then `GlobalConfig::save_local`
  refuses replacing B. A's key is dropped and the selection has changed.
  Without platform authority, setup can refuse after settings were changed.
- Saved A dev selection/key plus a valid B human login at the same origin and
  slug allows the preflight to authenticate B's login when selecting database
  B. A's key is rejected only after settings mutation. A revoked default key
  can similarly be masked by a valid login.

R1's two-database proof has no competing login and exercises an invalid default
key against an empty database. Its passing result does not cover either path.
The late protected key-conflict guard remains correct and must stay.

**Selected minimal correction.** Complete this at existing CLI resolution and
startup orchestration, using the existing protected configuration and shared
client owners:

- Refuse a preexisting protected default key when no saved `[dev].tenant`
  establishes this local deployment selection, including same-origin keys.
  Preserve the existing selection and give `WYRD_CONFIG_HOME` separation
  guidance before startup or local writes. Keep the remote-origin refusal
  with or without a key.
- For saved dev selections, keep the requested-slug conflict check and prove
  the exact retained protected key against the selected ready server before
  settings replacement/provisioning. Existing `ClientConfig` supports explicit
  credential selection with no tenant selector and
  `TokenCacheMode::InMemory`; reuse that and normal `WyrdClient` exchange and
  bounded read so saved-login precedence cannot substitute another credential
  or update protected files during proof.
- Preserve ordinary post-setup `saved_client` precedence, explicitly authorized
  recovery, the final conflict/race guard and same-deployment overrides.

This does not require a new requested-slug identity endpoint, SQL in the
client, token decoder, platform-credential prerequisite or profile system.
`save_settings` commits `[dev].tenant` before `provision` can issue and
`save_tenant` can save a key. Therefore supported key-before-endpoint partial
saves already retain dev selection; a failure before that save issues no key.
A preexisting no-dev key is an unproven client selection to preserve, not a
new-dev partial save to adopt. Do not move potentially issuing setup before
persistence or change shared human-login precedence.

**Focused acceptance/proof.** Extend the existing managed-Postgres partial
failure journey with both states. With a same-origin B key and no dev section,
request A with/without real platform authority; require early refusal,
byte-identical prior files, no A/admin issuance and B still usable. With A's
dev/key and a valid B same-origin/same-slug login, require refusal before writes
or issuance despite B login success; include invalid/revoked exact-key masking.
Retain A resume, dev selection plus missing-endpoint partial resume,
same-deployment listener/connection overrides and ordinary post-setup login
precedence. Use existing saved-login/token fixture facilities rather than a
new identity-provider dependency or harness.

## FIND-TASK-001-9 — a listener override repeats completed setup offers

**Diagnosis and consequence.** REQ-004 prohibits repeated offers on saved
restart; REQ-008 limits an override to its corresponding setting; AC-006
requires no repeat setup prompts. `dev.rs:341-345` derives `configured` from
exact saved HTTP endpoint equality and derives `offer_gateway` from it. The
gateway consumer at `:1124-1141` and host consumer at `:567-573` share this
producer. An interactive restart of the same tenant/database/key with only
`--bind` changed authenticates successfully but is marked fresh, reopening
gateway/provider and detected-host offers. Existing P8 uses
`--non-interactive` and hides both consumers.

**Selected minimal correction.** Derive resumed-setup offer policy from the
retained local dev selection/key, using FIND-2's existing identity guards and
preflight to reject an unproven or different deployment. Correct that one
producer for both consumers; transport-location equality is not a completed
deployment identity. No durable completion marker is needed. Preserve true
first use, missing-key recovery, key-before-endpoint partial resume and
explicit gateway-file/host requests, including unattended restart requests.

**Focused acceptance/proof.** Extend the existing installed PTY journey:
complete setup, stop, change only HTTP bind, retain the same database/key plus
a known provider variable and detected host, and resume interactively. Require
the full ready summary without gateway/host questions, unchanged key/provider
credential state, updated usable saved endpoint and owned-child settlement.
Keep explicit gateway-file and host checks. Fixing separate downstream prompts
would leave the shared producer wrong and is not the selected correction.

## FIND-TASK-001-7 — one cumulative catalog test still misses panic rustdoc

**Diagnosis.** The prior enumerated documentation fixes are present, but
`crates/wyrd/wyrd-server/src/mcp/gateway.rs:633-685`,
`gateway_catalog_annotates_every_administration_operation`, is also materially
changed in the cumulative candidate. It checks the new collection schema and
shifted tool index; its full body indexes, expects and asserts catalog/schema/
annotation data without a `# Panics` contract. AGENTS §16 and agent-rules make
this a completion requirement, including for private tests.

**Correction and proof.** Add the concrete panic conditions for missing or
mismatched catalog/order/schema/annotation data to that existing test.
Preserve its assertions, registration and behavior. Batch with the two
substantive corrections; source inspection and formatting prove this bounded
documentation change, without a new behavioral test or style-only round.

## Preserved behavior and non-goals

Keep existing protected file locking/replacement, final key-conflict checks,
server-owned setup/issuance, no-reissue behavior and explicit recovery.
Preflight must not require platform authority for a normal saved restart.
Preserve generated/file/inline sealing retention and conflict refusal, concealed
input cancellation, unattended host suppression, honest uncertain-commit
guidance, real SQL-aborted rollback proof, portable listener fixture, optional
gateway, existing single/array/two-step/source semantics and SDK consumption.

No public API/CLI contract extension, shared credential-precedence change,
arbitrary identity adoption, new store/profile/recovery/receipt mechanism,
implicit rotation, data reset, external repository edits, broad refactor,
weakened check or test suppression. Keep same-deployment overrides and existing
partial-save behavior usable. New or changed Rust items need substantive
rustdoc under the existing gate.

## Verification and handoff

The commands below are future implementer proof, not executions by this
review. Add the diagnosed branches to the existing focused proof; if choosing
new test names, record and run their exact real selectors as well. No broad
journey suites or `gate` are required for this remediation. Run Cargo-backed
commands sequentially.

```sh
mise exec -- cargo build --locked -p wyrd-cli -p wyrd-server -p wyrd-monty-worker --bins --tests

# Identity/refusal, human-login masking and preserved partial/recovery flows.
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=pg_tests::server_dev_repeat_and_partial_failure_preserve_state)'

# Interactive changed-listener restart without repeated setup offers.
mise exec -- scripts/postgres/with-test-postgres.sh -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m local_dev tests/integration/test_local_development.py::test_installed_cli_guided_setup

# Existing same-deployment saved-settings/listener override regression.
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=pg_tests::server_dev_saved_settings_survive_working_directory_change)'
```

Run `mise run fmt` and `mise run lints` for changed Rust. If Python proof
changes, run `mise run py:format`, `mise run py:lints`, `mise run py:typecheck`
and `mise run py:test:unit`. Align existing setup/recovery guidance if the
new refusal needs explanation and run `mise run docs:check` only if docs change.
No dependency, schema or SQL capability change is prescribed; use their narrow
existing check if the actual write set reaches one. Do not introduce a new
public identity read merely to satisfy a test.

Record exact RED/GREEN results and actual verification, plus unavailable
platform limits. No macOS execution was supplied by this review; do not claim
it. Before handoff inspect `git diff --check` and the complete changed source,
record the new candidate identity, and map closure to findings 2, 7 and 9.
The next review retains the original cumulative base, original task, both
prior ledgers/verdicts and R1/R2; a fix-only diff does not replace acceptance.
No commit, branch operation, merge, push or deployment is implied.

## Implementation evidence

Candidate: commits `bd09965ad`, `fd32c95df`, `ca8273c18` on `38dbc023c`
(evidence commit follows). Platform: Linux only; macOS was not executed.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-2: a preexisting key with no saved `[dev].tenant` is refused before startup or writes, with or without real platform authority; remote-origin refusal kept | `dev.rs` `DevArgs::resolve` (refusal after the remote-origin check) | `server_dev_repeat_and_partial_failure_preserve_state`: same-origin key, `[dev]` removed, `--tenant another`, without and with a real `recover-root` platform credential → "not created by local setup", files byte-identical, listener free, no `another` tenant (operator-session read); RED on old code: reached setup after saving settings | PASS |
| FIND-2: saved selection proves the exact retained key, no login/ambient/disk-cache substitution, before settings or provisioning | `dev.rs` `DevServer::run` preflight → `saved_key_config` + `proven` (explicit credential, empty `Environment::Fixed`, `TokenCacheMode::InMemory`); `saved_client` keeps ordinary precedence after setup | same test: B deployment on the same listener/slug, B session saved as a same-origin `local` login in A's config, A selects B's database/storage → "does not accept the saved key", files byte-identical, listener free; RED on old code: login passed preflight, settings rewritten, late guard refused | PASS |
| FIND-2: final conflict guard, A resume, remote refusals, database-switch refusal, lost-key recovery, same-deployment overrides kept | unchanged `save_tenant`/`GlobalConfig::save_local`; R1 segments retained | same test (all segments) and `server_dev_saved_settings_survive_working_directory_change`; `first_run`, `keeps_the_gateway_managed_secret_key`, `selected_mcp_host_reads` | PASS |
| FIND-9: one producer — `configured` is the proven saved key plus a saved endpoint, not endpoint equality | `dev.rs` `DevArgs::resolve` `configured` (feeds `offer_gateway` and the host offer) | `test_installed_cli_guided_setup` → `restart_on_another_listener`: interactive PTY restart with only `--bind` changed, provider keys and detected host present → full ready summary, no provider/host question, key unchanged, gateway deployment/credential unchanged via fresh CLI on the saved new endpoint, `[client].http_url`/`[dev].bind` updated, listener settled; RED on old code: blocked at "Configure which providers" | PASS |
| FIND-7: panic rustdoc on `gateway_catalog_annotates_every_administration_operation` | `crates/wyrd/wyrd-server/src/mcp/gateway.rs` | `mise run fmt`, `mise run lints`, source inspection | PASS |

Commands (all passed, sequential):

```sh
mise exec -- cargo build --locked -p wyrd-cli -p wyrd-server -p wyrd-monty-worker --bins --tests
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=pg_tests::server_dev_repeat_and_partial_failure_preserve_state)'
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=pg_tests::server_dev_saved_settings_survive_working_directory_change)'
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=pg_tests::server_dev_first_run_is_client_ready) | test(=pg_tests::server_dev_keeps_the_gateway_managed_secret_key) | test(=pg_tests::server_dev_selected_mcp_host_reads)'
mise exec -- scripts/postgres/with-test-postgres.sh -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m local_dev tests/integration/test_local_development.py::test_installed_cli_guided_setup
mise run fmt; mise run lints; mise run py:format; mise run py:lints; mise run py:typecheck; mise run py:test:unit; mise run docs:check
git diff --check bc3bf25e3f78b0c429110350a3de4d58290e632f..HEAD
```

Decisions and limits:

- R1's "no tenant in the other database" check counted `platform.tenants`
  without an operator session; row security hides every row there, so it
  could never fail. The new `tenant_slugs` helper reads in an operator
  session (it also sees the built-in `wyrd-system` tenant), and both checks
  now assert the specific slug is absent.
- The login-masking proof saves the other deployment's real access token as
  a same-origin, same-slug saved login (existing `SavedLogin::from_token`);
  both servers use the fixed test signing key so that token is valid on the
  selected database. It also covers an invalid exact key masked by a valid
  login, since A's key is invalid on B.
- Key-before-endpoint partial resume (key saved, endpoint not) stays
  unconfigured, so its offers still occur.
- Docs: `get-started/setup.md` restart paragraph names the new refusal and
  the listener-only resume.
- No public API, credential precedence, schema, or dependency change.
