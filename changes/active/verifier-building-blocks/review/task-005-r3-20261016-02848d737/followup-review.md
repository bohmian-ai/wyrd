# Focused evidence-admission follow-up

**RESOLVED.** The individual Deployment file ceiling is an input-admission obligation, not solely a byte-consumption guard. The candidate's complete-set admission omits it for both File declarations and selected Artifact files. The reported no-read counterexample is reachable through the existing direct, queued, and MCP owners. One correction at `TenantEvidence::admit` covers them all.

This is a discovery conclusion for independent Ponytail validation, not the task verdict. Proposal **FUP-R3-01** corroborates BEH-R3-01, INV-R3-01 and MNT-R3-001. I recommend **new FIND-TASK-005-14**, rather than reopening FIND-10 or FIND-11, for the precise reasons below.

## Immutable subject and narrow scope

All source, authority and Git inspection used detached `/tmp/wyrd-task005-review-02848d737`, HEAD `02848d737a621938ec0b5f817e1aa84d5f3df1fd`, base exclusive `f2a726c90edbf8956c7b91c843379175ee20eac0`. No `.codegraph/` exists there. The complete cumulative range identifies the subject; the cumulative hosted evidence diff and current callers establish this particular uncertainty. This follow-up did not repeat the whole task audit.

Read this round's subject and claims comparison; behavior, invariant and maintainer proposals; frozen prior R2 verdict and findings validation; the committed original TASK-005, approved spec r9 and committed R2 remediation. Applied the frozen task-review role, agent rules, spec-development and maintainer guidance. Peer PASS results for other checks do not establish closure of this counterexample.

## What the authority requires

- `spec.md:133–153` (REQ-002) describes media `max_bytes` as an optional **lower ceiling**, refuses values outside declared size constraints with structured errors, and keeps server validation authoritative.
- `spec.md:928–950` defines FILE_MAX as “Maximum bytes in one input file,” FILES_MAX as the combined execution bound, and says slots may lower, never raise, Deployment limits. These are separate limits. Neither is conditional on an action choosing to read the file. Omitting `max_bytes` cannot waive FILE_MAX.
- `spec.md:872` gives hosted admission/evidence resolution to the server, and INV-007 bounds inputs. Original TASK-005 owns final server admission/evidence planning (`tasks/TASK-005-server-integration.md:23`) and composes startup limits into the runtime (Scenario 6).
- `docs/src/content/docs/reference/verification-runtime-limits.md:93` explicitly promises a file above its slot or FILE_MAX causes the existing 413 **before anything runs**. That current implementation documentation corroborates the approved size contract; it is not the sole authority for a new requirement.

The spec does not require admission to fetch every object's body or prove all digests before independent assertions. This finding requires only refusal of an already-known invalid size, using the metadata the existing owner already has. It makes no unbounded-memory claim: `stored` still protects actual byte consumption.

## Producer-to-consumer proof

| Stage / sibling path | Current source and effect |
|---|---|
| File producer | `components/verification/service.rs:317–374::evidence` checks per-file and per-plan total before issuing upload plans; `upload_local_evidence:388–414` bounds the local body. Neither creates a required upload session consumed by execute/enqueue. Raw typed declarations can be supplied directly. Previously valid tenant evidence can also remain after a restart lowers current limits; no migration is required or proposed. |
| Independent Artifact producer | `pg_hosted_artifacts.rs:85–125::Registered::start` uses `build_registration_input` and public `Cards::register` for real `notes.txt` and `check.py` inventory/bytes. This is the Card registration saga (`wyrd-client/src/cards/handle.rs:298–304`), not verification evidence planning. `components/cards/service.rs:233–265::list_card_artifacts` returns the server inventory. A valid registered Artifact need not fit every later verification Deployment's file ceiling. |
| Direct entry | `components/verification/routes.rs:345–352` and `service.rs:449–506::execute` reach `HostedBlocks::engine` with `request.inputs`, then `VerificationEngine::run`. There is no preceding effective per-file check on these declarations. |
| Queued entry | `service.rs:581–635::enqueue` freezes the declarations in Submission; it deliberately performs no registry/byte reads or request-path Postgres write. `verification/runner.rs:409–450` restores submitted inputs and calls the same hosted engine. This gap affects execution admission, not the asynchronous enqueue receipt contract. |
| MCP entry | `src/mcp/verification.rs:191–236::mcp_verification_write` decodes typed arguments, then calls the same control `execute` or `enqueue`. It does not require a preceding evidence-plan operation. This is a reachable primary headless path, not an unused helper. |
| Shared composition | `verification/hosted/blocks.rs:107–153::engine` installs one TenantEvidence as the engine's admission reader and shares it with Agent/media/tools/code/extract consumers. |
| Slot conformity | `vala-eval/src/blocks/engine.rs:921–974::conform` rejects slot mismatches and authored File format/max_bytes violations. It has no Deployment limits. `run:190–203` invokes hosted `admit` before `drive`. |
| Hosted admission | `hosted/evidence.rs:333–379::admit` obtains declared File size, or resolves true selected Artifact metadata via `artifact_file:192–244`. Artifact format and authored max_bytes are checked. Both sizes then go directly into overflow-safe FILES_MAX accounting. There is no FILE_MAX comparison. |
| No-read consumer | `scope.rs:198–231::resolve` returns JSON directly; whole non-JSON inputs can also resolve as declarations. `engine.rs:855–862::json` serializes those declarations for assertions without reading bytes. Independent JSON assertions need not touch a supplied media slot at all. |
| Byte consumers and guard retained | `evidence.rs::read`, `media`, `located/cited` and extract `ActionExecutor::execute` use `stored`; `workspace.rs:68–102` materializes through `read`; `sandbox.rs:253–270::CodeExecutor::run` loads Artifact code through `artifact`. `stored:255–292` refuses size above FILE_MAX before reading and preserves exact length/digest checks. These later consumers do not repair an earlier no-read pass. For consuming graphs their terminal BlockError becomes block-level failure, rather than execution-level input refusal. |
| Existing refusal mapping | `service.rs:533–540` maps EngineError::InputTooLarge to the existing public 413. `runner.rs:871–873::engine_failure` maps it to terminal errored/input_invalid. No new error, queue rule, transport or SDK validator is needed. |

Concrete static counterexample: reuse the valid registered spec in `pg_verification_routes.rs:1089–1198::direct_execution_refuses_nonconforming_evidence`. It declares integer JSON `x`, optional `note`, and optional PDF `pages` without max_bytes. Its two requirements inspect only x and note. Supply x=1 and a File for pages with size 21 MiB under defaults FILE_MAX=20 MiB and FILES_MAX=100 MiB. Typed decoding and conform accept it; admit accepts the 21 MiB total; both requirements pass without invoking stored. The existing test uses 200 MiB and therefore exercises only the combined guard for this unconstrained slot. The size-only refusal requires no real 21 MiB allocation or object read.

The Artifact case does not depend on a fictitious File declaration. The existing registered notes file is 11 actual bytes (`pg_hosted_artifacts.rs:72`). Under valid configured limits FILE_MAX=4 and FILES_MAX=20, a text slot with no authored maximum and an independent passing JSON assertion admits that real selected Artifact: artifact_file returns size=11, the combined check accepts 11, and no byte reader runs. Current tiny-limit tests at `:1117–1201` cover format, authored max, duplicate-slot combined total, Artifact-plus-File combined total and repeated reads; they omit this individual-ceiling interval. Their `tight_files` changes only FILES_MAX; proposed closure should set **both** limits consistently rather than copying that invalid startup relationship.

## Relationship to prior stable findings

The precise diagnoses in the frozen R2 validation, verdict and committed R2 remediation are narrower than “all evidence validation”:

- **FIND-TASK-005-10** is present declarations bypassing **selected Verifier slot** names/kinds/JSON schema/media format/authored max_bytes. It expressly distinguishes lower digest/sniff/read integrity from slot conformity. Current conform plus authorized Artifact slot checks closes that diagnosed invariant. The interval counterexample uses a conforming slot, so it is not evidence that the prior slot correction failed.
- **FIND-TASK-005-11** is absence of an **execution-wide combined** original-file budget; its example uses multiple individually legal files, and its correction accounts original inputs once before work. Current checked_add against FILES_MAX closes that precise invariant. The counterexample has one input below FILES_MAX. R2 said retain per-file restrictions; the stored guard is retained, but its preexisting consumption-only timing is a separate omission not diagnosed there.

These mechanisms share the existing TenantEvidence admission owner and should receive one small correction there. Sharing that owner does not make the three independently stated constraints one defect, and splitting by File/Artifact/direct/queued/MCP would be wrong. Accordingly FUP-R3-01 proposes one **new FIND-TASK-005-14**, with all three discovery IDs attached, while recording source closure of FIND-10 and FIND-11 as previously diagnosed. INV-R3-01's concrete counterexample is supported; its proposed residual-10 attribution is not supported by the prior precise ledger. Independent validation owns the final stable-ID assignment.

## Smallest correction and focused proof

At the existing `TenantEvidence::admit`, refuse each resolved original File/selected-Artifact size above current `limits.file_max_bytes`, returning existing EngineError::InputTooLarge before drive. The shared branch after the existing File/Artifact match is sufficient; do not add per-caller checks or require upload planning for Artifacts. Keep overflow-safe combined accounting and slot checks. Keep stored's lower guard because it owns the byte trust boundary and also protects Artifact code sources, which are not original input slots. Preserve permissions/audits, inventory resolution, digest/length/sniffing, workspaces, retries and cancellation.

The smallest credible regression proof extends existing admission fixtures: a File above FILE_MAX and below FILES_MAX with no slot max and an otherwise passing no-read requirement must yield direct 413; the same submitted shape through the queued fixture must terminally error without a verdict; a real selected Artifact in that interval must refuse from metadata alone. Include within-limit control. These expose the gap without a new harness or live provider. Do not demand a separate fix or new FIND ID for every entry point. Existing direct/queued mappings and shared MCP control explain propagation; no fresh dedicated MCP execution was performed here.

Static review only: no builds, gates, tests, live calls, source edits, migrations or archived source/diffs. HEAD remained pinned and detached source status was clean. No runtime reproduction is claimed; the reachable counterexample and limitations are source-derived.

Skipped fresh execution and unrelated audit work. Risk pending validation: conforming evidence above the configured individual-file ceiling can currently produce a passing result without a read.
