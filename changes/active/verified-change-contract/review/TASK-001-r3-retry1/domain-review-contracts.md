# TASK-001 r3 retry 1 — Domain review: public contracts and schemas

## Subject and reviewed boundary

- Repository: `/Users/stevenforrester/Documents/GitHub/verified-change-contract`
- Immutable subject: base `5293546f33b3a5fd9de529098e23ea70d472c412` → candidate `9d7b6266206f15136f306b66c06571066fc6bd13`
- Approved authority: `changes/active/verified-change-contract/spec.md`, revision 32
- Original task: `changes/active/verified-change-contract/tasks/TASK-001-verifier-contract-and-registration.md`
- Remediation authority: the round-1 and round-2 verdicts, validated ledgers, and remediation tasks under `review/TASK-001-r1/` and `review/TASK-001-r2/`
- Boundary reviewed: the Verifier/binding/Trigger/Operator authored contract; JSON Schema and OpenAPI generation; recursive OpenAPI component closure; Card-kind and Rust/Python/TypeScript/CLI projections; and live Eval/Drift registration documentation owned by TASK-001.

The candidate was `9d7b62662` at review start. `.codegraph/` is absent, so source discovery used `rg` and direct reads as instructed by `AGENTS.md`.

## Authority and source coverage

| Boundary | Governing authority | Source and artifact evidence | Result |
|---|---|---|---|
| One native Verifier Card and closed Drift/Eval implementation union | REQ-045/046/047/056/109/110/111, INV-006/012/014; `AGENTS.md` §2; `architecture/wyrd-design.md` kind catalog and Verifier section; doctrine/vocabulary reference | `wyrd-spec/src/{envelope.rs,card/{mod,verifier,drift,eval}.rs}`; Verifier YAML fixtures; `card_kind.json`, `verifier_spec.json`, `verifier_implementation.json` | PASS |
| Binding and activation public shape | REQ-090/091/093/094/102/103, INV-001/013, AC-004/018 | `card/{agent,service,trigger,operator,verifier}.rs`; generated `verification_binding.json`, `trigger_spec.json`, `trigger_activation.json`, `operator_spec.json` | PASS |
| JSON Schema closure and strictness | REQ-045/046/091/093/109/114, AC-004/021; generated artifacts must be source-owned | `wyrd-spec/examples/gen_schemas.rs`; source derives/manual schema owners; generated and golden schemas. The Card-kind enum has exactly 15 values; Verifier implementations are exactly `drift` and `eval`; bindings require `verifier` and `runs_on`; both Trigger branches reject unknown fields. | PASS |
| OpenAPI registration and transitive component closure | REQ-045/109/114, AC-021; language-agnostic HTTP contract | `wyrd-server/src/http/openapi.rs`; `VerifierImplementation::{schema,schemas}`; generated `openapi.yaml`; `verifier_contract_component_references_all_resolve` | PASS |
| Rust/Python/TypeScript client projection | REQ-109/114, AC-021; no symmetry-only TypeScript CardKind abstraction | `wyrd-client::WyrdState::verifier`; Python native wrapper and generated public stubs; UI/native kind projection; TypeScript generated stable errors | PASS |
| CLI and live public Eval documentation | REQ-109/114, AC-021; Scenario 3 requires the retired Card story to be unreachable | `wyrd-cli/src/{error.rs,eval/run.rs}`; `wyrd-spec/src/card/eval.rs`; `wyrd-spec/src/vala/eval/{mod,spec}.rs`; `vala-eval` public crate/type docs | **FAIL — CR3-C1** |
| Retired pull/schema projections | REQ-116/120/144, AC-022 | Deleted Eval pull request/response schemas and route/client surfaces; removed alert-router schemas; current generators and schema catalog | PASS |

## Round-2 finding closure in this domain

| Finding | Contract-domain result | Evidence |
|---|---|---|
| `FIND-TASK-001-2` | PASS | The real-server nested inline-Agent refusal now crosses HTTP decoding and stable error mapping and proves no registration writes. |
| `FIND-TASK-001-4` | PASS | Referenced duplicates use `CardRef::identity_key`; equal inline Operators use existing typed equality; focused tests cover UID presentation and the denial code. |
| `FIND-TASK-001-5` | **INCOMPLETE** | The named `error.rs` and three `wyrd-spec` module headers are corrected, but other live TASK-001 CLI and public Eval docs still call the accepted Verifier an Eval Card; see CR3-C1. |
| `FIND-TASK-001-14` | PASS | `TriggerActivation`, `VerificationBinding`, and `VerifierImplementation` are registered through the existing OpenAPI owner. `VerifierImplementation::schemas` forwards `DriftSpec` dependencies, and the recursive test walks every `$ref` reachable from the five Verifier/binding/Trigger roots. |

## OpenAPI remediation assessment

The `openapi.yaml` increase of 765 lines is reproducibly owned by the existing
utoipa source registration, not a hand-authored parallel model. It adds 25
components: the three named roots and the Operator/Trigger/Drift dependency
closure. Every newly added component except `DriftSpec` is referenced from the
new roots or their transitive closure. `DriftSpec` itself is defined but
unreferenced because the Drift branch inlines its `PartialSchema`, while the
same `ToSchema::schemas` implementation forwards the nested dependencies that
the inline body needs.

That extra `DriftSpec` definition is harmless, source-owned schema output, not a
material task extension: it adds no route, accepted payload, Card kind,
compatibility path, or alternate contract, and deleting it is optional OpenAPI
tidying explicitly outside the round-2 remediation. It is therefore not a
finding. The recursive closure test is correctly bounded to the newly required
roots and would fail for a missing transitive dependency, as its recorded RED
on `DriftSignal` demonstrates.

## Material finding

### CR3-C1 — DRIFT — live CLI errors and public Eval engine docs still advertise an Eval Card

- **Violated obligation:** REQ-109 requires authored Cards and CLI surfaces to use `kind: Verifier` with `implementation.kind: eval`, with no competing Eval Card story. REQ-114 and AC-021 require public documentation and projections to describe that same model. TASK-001 Scenario 3 makes the retired surface unreachable rather than retaining vocabulary that sends callers back to it.
- **Exact location:**
  - `crates/wyrd/wyrd-cli/src/eval/run.rs:77,83,110,118`
  - `crates/vala/vala-eval/src/lib.rs:1`
  - `crates/vala/vala-eval/src/orchestrator/state.rs:121`
  - `crates/vala/vala-eval/src/results.rs:79`
- **Evidence:** `load_eval_card` now accepts only `Spec::Verifier(VerifierImplementation::Eval(_))` and returns a `CardRef` with `CardKind::Verifier` (`run.rs:90-125`), but parse diagnostics still label the input "eval card JSON/YAML" and missing metadata errors say "run an eval card." The reusable engine's public crate docs and public `CardRef` fields likewise say "Eval card" even though TASK-001 changed representative engine references to `CardKind::Verifier` (for example `results.rs`'s aggregation fixture and `executor.rs`'s end-to-end fixture). The corrected `WyrdCliError::NotEvalCard` display and remediation already state the real `kind: Verifier` contract, so one command can emit both models depending on which failure occurs.
- **Observable consequence:** a user following a parse or metadata diagnostic, or generated Rust documentation for the retained Eval engine, is told that a registrable Eval Card exists. Authoring `kind: Eval` is then rejected by the same command and server. This is the same retired public story that round-2 finding 5 was meant to close, not an internal variable-name preference.
- **Required testable correction:** change only these user-facing strings and rustdoc descriptions to “eval-backed Verifier Card” (or equivalent precise Verifier wording). Preserve `load_eval_card`, `eval_ref`, `EvalResults`, engine semantics, stable error codes, and the TASK-002-owned observation contract. Prove closure with a scoped case-insensitive search for `eval card` over these live CLI/engine files, then run the existing CLI unit/journey and documentation checks; no new compatibility alias, type rename, test harness, or broad UI cleanup is required.

## Verification limits

- This was a read-only static review. No Cargo-backed command was started because Wave-1 agents share the checkout and repository rules require Cargo work to run sequentially.
- The implementation packet records green `fmt`, `lints`, `test:shared`, `test:cards:integration`, `test:wyrd`, all nine `test:bifrost` lanes, `codegen:check`, `docs:check`, `check:client-tier`, `check:pyo3-scope`, and `git diff --check`, plus focused OpenAPI closure and repeated no-retry restart/Oracle proofs.
- Python and TypeScript runtime lanes were not rerun in round 2. Static inspection found no round-2 SDK-source or generated-projection change and their cumulative projections expose `Verifier`, not Drift/Eval Card kinds.
- The OpenAPI closure test proves reference resolvability, while `codegen:check` proves the committed artifact is reproducible from source. This review did not require unrelated pre-existing OpenAPI roots to become closed.
- TASK-002-owned `drift_ref`/`eval_ref` observation-record migration and UI observation wording were not promoted into this finding.

## Overall result

**FAIL.** The schema and OpenAPI remediation is sound, including the deeper
Drift dependency closure, and the unreferenced `DriftSpec` component is harmless.
TASK-001 still leaves one bounded public-vocabulary gap: the accepted CLI path
and retained public Eval engine docs continue to call the Verifier an Eval Card.
