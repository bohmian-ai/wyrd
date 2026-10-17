# Independent findings validation

**Status: COMPLETE.** Three bounded findings remain: **FIND-TASK-001-2, FIND-TASK-001-7, FIND-TASK-001-9**. No specification revision is required. The disputed identity path is reachable, but its correction does not require another tenant-identity API or platform authority.

## Subject and evidence

Root is this worktree. Base `bc3bf25e3f78b0c429110350a3de4d58290e632f` → candidate `38dbc023cd71292e1badd3abe90e87c69b76fa96`; HEAD matched during validation. Approved spec revision 1 and original `tasks/TASK-001-guided-local-development.md` govern. Reviewed the cumulative subject and current owners, using the prior-candidate remediation range only to locate closure. Inputs include both task reviews, standards, maintainer, system, security, data, claim comparison, focused follow-up, prior validated ledger/verdict and R1 evidence. Source evidence resolves disagreements; report counts do not.

Applied AGENTS, agent-rules, task-review validation role, maintainer/spec workflow guidance and applicable design/doctrine, protected-configuration, security and testing obligations. No CodeGraph index exists. No source edit, Cargo/build/test/lint/codegen execution, credential operation or sibling-repository inspection occurred. Recorded passing results were checked against proof bodies, not independently reproduced.

## Every proposal resolved

| Proposal or disputed closure | Disposition | Stable ledger result |
|---|---|---|
| BEH-R2-1, INV-R2, FUP-R2-IDENTITY; contrary security/system/maintainer closure claims | **REVISED**: both reachable failures confirmed; choose refusal of an unproven no-dev retained-key selection plus exact-key preflight for saved-dev resumes, rather than new requested-slug identity lookup | FIND-TASK-001-2 retained |
| INV-R9: listener-only saved restart repeats offers | **CONFIRMED**: transport equality is the incorrect shared offer-policy producer | FIND-TASK-001-9, next unused ID |
| REPO-R2-001: cumulative MCP catalog test panic contract | **CONFIRMED**: changed test body lacks required `# Panics` | FIND-TASK-001-7 retained |
| Prior FIND-1/3/4/5/6/8 closure claims | **CONFIRMED**, with source/proof boundaries below | Closed |

No rejected material proposal is carried as optional advice. Non-blocking import placement/qualification observations in standards are not findings under the review's consequence threshold. No DRIFT-only requirement is introduced.

## Deduplicated retained ledger

### FIND-TASK-001-2 — REVISED — INCORRECT: preflight accepts the wrong credential/selection

Sources: BEH-R2-1, INV-R2, FUP-R2-IDENTITY; prior FIND-2. Obligations: REQ-002/008 and INV-003/004 preserve unrelated identity and selection before replacement or unsavable issuance.

**Locations and reachability.** `crates/wyrd/wyrd-cli/src/server/dev.rs:307-327` checks saved dev slug only if present, and otherwise compares saved origin. `:535-551` invokes `saved_client`, then saves settings and provisions. The full `saved_client` body at `:1269-1304` leaves `ClientConfig.credential` unset. `crates/shared/wyrd-client/src/config.rs:221-249` legitimately chooses a saved human login before the default key; `SavedLogins::save` preserves that default key. Login selection stores origin/slug, not database/tenant UUID.

Two concrete producer-to-consumer paths remain:

1. An existing same-origin default B key/client selection with no `[dev]` is producible by the ordinary `GlobalConfig::save_local` owner (`global_config.rs:213-257`). Request absent A. B's key authenticates and reads B successfully. `save_local_dev` changes DSNs/dev selection to A. Server `main.rs::setup:261-302` can create A with legitimate platform authority. `ProvisionedTenant::credential` (`dev.rs:916-933`) prefers newly issued A, so its prefix check passes; final `save_local` correctly refuses to replace B. A's key is then lost and selection was already changed. Without platform authority, the later refusal still follows selection mutation.
2. Keep A's default key/dev selection and a valid B human login at the same origin and slug, then select B's database. The slug-only login guard passes; ordinary saved-client precedence uses B's login and masks A's invalid key. Settings change before setup returns B's identity and the later key-prefix check rejects A. A revoked default key can likewise be masked by a valid login. The current two-database test has no competing login and proves only default-key rejection.

**Smallest complete correction.** Keep this within existing CLI resolution/orchestration and shared client interfaces:

- At input resolution, refuse a preexisting protected default key when there is no saved `[dev].tenant` establishing the local deployment selection, including same-origin keys. Explain that the existing selection is preserved and use a separate `WYRD_CONFIG_HOME`. Retain the existing remote-selection refusal with or without a key. Do not adopt a preexisting selection based on a loopback origin or a user-supplied slug.
- For a saved dev selection, retain the existing requested-slug conflict guard and authenticate the **exact** protected retained key against the selected ready server before `save_settings` or `provision`. Reuse `ClientConfig.credential`, no tenant selector, `TokenCacheMode::InMemory`, and existing `WyrdClient` exchange/bounded read. Explicit credential selection prevents saved login substitution; memory caching prevents the preflight from updating protected files. This is a dev preflight concern, not a change to shared credential precedence.
- Preserve normal post-setup `saved_client` behavior, explicit recovery/no automatic rotation, final protected key conflict/race protection, and same-deployment DSN/password/listener overrides.

**Why no server identity extension is needed.** The new dev workflow's only settings writer is `save_settings` → `GlobalConfig::save_local_dev`, which must successfully commit `[dev].tenant` before `provision` can issue a key; only later does `save_tenant` call `save_local`. Thus a supported key-before-endpoint partial save retains its dev selection. A failure between protected DSN save and dev-config save occurs before issuance. The no-dev retained-key state is an existing client selection requiring preservation, not a successful dev partial-save state requiring adoption. REQ-002 explicitly authorizes refusal rather than adoption; REQ-003's no-saved-selection provisioning does not authorize overwriting an existing unrelated key. Refusing this unproven selection plus the existing saved-dev slug guard and exact-key server authentication closes both reported paths without a requested-slug lookup, SQL in the client, platform-credential prerequisite, token decoder, new command or public API semantics. Arbitrary manual fabrication of inconsistent `[dev]` identity is not proof of a required new recovery/identity protocol.

Existing owners remain authoritative: `CredentialsFile` locks/replaces protected files, server setup owns active-tenant no-reissue and creation, platform recovery owns authorized rotation, and normal shared clients own human-login precedence. Do not merely reorder setup before persistence: setup can issue the unsavable key.

**Focused closure proof.** Extend existing P9, not a new harness: seed a usable same-origin B key/endpoints without `[dev]`, request absent A with/without platform authority, and assert refusal, byte-preserved files, no A/admin issuance and settled child. Seed A dev/key plus valid B same-origin/same-slug login; selecting B must refuse before file mutation/issuance despite B login success. Include invalid/revoked exact key with valid login. Retain A resume, partial key-before-endpoint resume with `[dev]` intact, same-deployment listener/DSN overrides, and normal post-setup login precedence. Use the existing P9 exact selector and managed-Postgres wrapper; use nearest shared-client checks only if that owner changes.

### FIND-TASK-001-9 — CONFIRMED — INCORRECT: listener equality controls completed-setup offers

Source: INV-R9. Obligations: REQ-004 normal saved restart does not repeat offers; REQ-008 overrides affect corresponding settings; AC-006.

**Location and path.** `dev.rs:341-345` computes `configured = key.is_some() && saved_http == selected_endpoint`; `offer_gateway` inherits it. `DevInputs::configure_gateway:1124-1141` consumes that flag; `DevServer::run:567-573` separately consumes `configured` for hosts. A saved same-deployment restart on a tty changing only HTTP bind makes `configured` false. Its valid key authenticates at the new listener, setup returns the existing tenant, then gateway/provider and detected-host offers recur. Existing P8 uses `--non-interactive`, hiding both consumers. This is a reachable policy regression, not credential identity drift or a second per-consumer defect.

**Minimum correction.** Derive resumed-setup offer policy from the retained local deployment selection/key, with FIND-2's resolution/preflight rejecting an unproven or different deployment. Endpoint equality is not completion identity. Correct the one producer shared by gateway/hosts; no durable completion marker is needed. Preserve true first use, missing-key recovery, key-before-endpoint partial resume, explicit gateway-file application, and explicit hosts on restarts/unattended runs.

**Focused closure proof.** Extend existing guided P1 PTY journey with a completed setup stopped and resumed on a new HTTP bind, same database/tenant/key, detected host and known provider variable present. Require complete ready output without gateway/host questions, unchanged saved key/provider credential state, updated readable endpoint and settled child. Retain explicit gateway-file/host checks. Use existing P1 exact pytest selector and managed wrapper.

### FIND-TASK-001-7 — CONFIRMED — VIOLATION: residual changed verification item lacks mandatory panic documentation

Source: REPO-R2-001; existing prior FIND-7 root cause. AGENTS §16 and agent-rules require panic documentation on materially changed Rust tests; the task-review skill explicitly retains missing rustdoc as blocking.

`crates/wyrd/wyrd-server/src/mcp/gateway.rs:633-685`, `gateway_catalog_annotates_every_administration_operation`, is materially changed in the cumulative subject: the new collection schema is checked at `tools[9]` and the final required-field check shifts to `tools[14]`. Its full body indexes, expects descriptions/annotations, and asserts catalog order/schema/effects. Purpose rustdoc exists, but no `# Panics` section does. Ordinary server-library test registration makes this a live verification item; prior enumerated docs being fixed does not close the cumulative omission.

Add one accurate panic contract for mismatched/missing catalog/schema/annotation data to this existing test; preserve assertions and behavior. Source inspection and formatting suffice; no new behavioral test. Batch with the substantive corrections above, not a style-only remediation round.

## Explicit prior-finding closure

| Prior stable ID | Current source and proof assessment | Result |
|---|---|---|
| FIND-TASK-001-1 | `mcp/mod.rs::offer_local` now receives resolved interactive policy; empty implicit hosts return before prompting when false, explicit hosts still reach `InstallArgs`. Installed PTY unattended/detected-host proof plus explicit host restart exercises both branches. | CLOSED |
| FIND-TASK-001-2 | Origin refusal and preflight order fix narrower paths, but exact-key substitution and same-origin/no-dev adoption remain as traced above. | OPEN, REVISED |
| FIND-TASK-001-3 | Full `local_sealing_key`/`supplied_sealing_key`/`sealing_bytes` bodies retain validated effective file/inline bytes in the existing no-clobber owner-only file; every child uses it. Existing secret-file reader and server keyring remain owners. Generated/file/inline test invokes managed-provider calls before/after no-export cwd restart, verifies unchanged key and conflict refusal. | CLOSED |
| FIND-TASK-001-4 | CLI `configure_gateway`, route 503 and setup/troubleshooting now allow uncertain commit. `LosingProxy` reads the real committed answer before discarding it; installed journey asserts failure/settlement, durable readback and safe named reapply. No compensation/retry owner added. | CLOSED |
| FIND-TASK-001-5 | Biased interruption future is polled before `resolve`, across install and run. Cancelling `Terminal::conceal` drops retained `EchoGuard`; PTY INT/TERM proof observes exit 130, restored ECHO and absent setup persistence. | CLOSED |
| FIND-TASK-001-6 | `GatewayAdministration::apply_deployments` still composes shared credential/deployment writes on one caller-owned TenantConn and commits once. P4 now first rotates `managed-key`, then changes referenced `shared` provider, reaching the existing immediate FK rejection before deployment validation. It compares full rows/envelopes/deployments, opens the old envelope to SUBMITTED, and retains permission audit accounting. | CLOSED |
| FIND-TASK-001-7 | Prior listed helpers/constants now have role/panic docs; cumulative MCP catalog test above remains. | OPEN, bounded residual |
| FIND-TASK-001-8 | Full `free_bind` body uses `TcpListener::bind("127.0.0.1:0")` and `local_addr`; procfs/range/atomic selection is removed. Shared LocalDev callers retain address/cleanup behavior. Source portability closed; macOS execution not claimed. | CLOSED |

R1's guided-ready failure diagnosis matches the source: `converse` previously returned at the first ready-summary line, before its only caller's later gateway-summary assertion; it now waits for the final stop-guidance line and retains the assertion. No timing relaxation or hidden test skip was introduced. Direct utils dependency reuses the existing secret reader; no new dependency family or durable owner is needed. Composite apply still shares individual put validation/sealing/reference/FK owners and ordinary HTTP/MCP permission/audit boundaries; retained sources and restricted SDK mutation boundary are unchanged.

## Limits and handoff

No unresolved proposal, missing identity API, material spec decision or sibling-state blocker remains. Recommendations are within approved behavior and use existing owners. The final ledger is the three IDs above; prior IDs are preserved and only the independent offer-policy root cause receives new ID 9.

Skipped independent execution, broad suites and macOS/arm64 runtime verification. Recorded Linux focused passes do not exercise the newly validated identity/interactive-listener cases; those focused proofs must accompany correction.
