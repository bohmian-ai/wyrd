# Maintainer review — TASK-001

Result: **FAIL**

Subject: `06a368906187be20364a2d959da311151964b617...497dd3acc8fd06567a0e0ff01baf599f9af1711b`. HEAD remained the candidate throughout this review. No implementation files were changed.

## Authority and scope

Reviewed the approved revision 3 specification, original task at the base, cumulative diff, repository AGENTS.md, agent rules, maintainer style, spec-driven development, applicable Rust/PyO3/Python/TypeScript/error/testing references, and the client/Agent/runtime boundary in the design and doctrine. CodeGraph was consulted first; bounded source reads and searches supplied the exact owners where its results were broad.

This is the maintenance and simplicity pass. It does not introduce a replacement runtime, new SDK execution convenience, transport, parser, or abstraction.

## Changed-surface coverage

| Surface | Owners, callers, and evidence inspected | Assessment |
|---|---|---|
| Shared-client execution deletion | Deleted `observe/invoke.rs` and `gateway_caller.rs`; module and root exports; `state.rs` invocation-only test; `HttpTransport::post_native`; removed integration modules | Removal follows the actual execution owner. No replacement client runner or orphaned public gateway-caller export remains. Other HTTP operations retain their owner. |
| Internal Skald preservation | Agent fields/construction, callback context, bounded-loop result construction, `RunConfig`, `FinishReason`, `AgentRun`, session types, agent/runtime/tool module exports and manifests; Vala Agent executor and simulator call sites; separate server gateway caller | Changed engine code removes Python projections and Python-only retained fields, rather than rewriting internal execution. The server/Vala execution seam remains distinct from the deleted public client caller. |
| Python native/package/type closure | `_wyrd` registration, root imports and authored root stub, removed `wyrd.agent` package/session/tool helpers, deleted native runtime/tool/provider glue, `PyRun` and authored Observe stub | Native registration, public imports, and authored typing agree on deletion. Declarative Card and Prompt registration remains. Searches found no surviving runtime-package import in these consumers. |
| Python tests and examples | Removed Agent/session/callback/registrability/execution smoke consumers; retained public parity, export and transport-example tests; modified gateway, support-desk, and installed-CLI tests | Deleted tests target the removed execution vehicle. Modified application workflows retain deployment selection and refusal assertions. The CLI helper uses the existing `http_auth` boundary. |
| TypeScript native/public/generated closure | `NativeRun::invoke`, its optional-JSON helper, public `Run.invoke`, `.d.ts` and `.d.cts` deletion; surrounding `NativeWyrdState`, hydrated Agent/Prompt projections | Native, public, and both generated declaration diffs agree. The deleted parser had no surviving caller. Prompt/Agent navigation remains a declarative projection. |
| TypeScript application consumers | Support-desk `Desk`, `deploy`, `answer`, `serve`; gateway `askAgent` and route test; support-desk denial test; actual support Prompt fixture | Execution stays in example/test application code through existing `httpAuth`. There is no TS Prompt renderer to reuse; the support example binds its checked-in text template locally. No general renderer is added. |
| Rust SDK and application consumers | Rust SDK exports/docs; gateway `Inference::{ask_openai_with, ask_agent, post}` and changed journeys; support-desk `answer`/`serve` and journey; support-desk-app `SupportDesk::answer`, `ask`, `review`, and `ApiError::internal` | New application IO lives beside its workflow. Existing dependency-owning `Inference` and `SupportDesk` owners are reused; invocation-local example functions do not earn new utility owners. One required documentation gap is recorded below. |
| Guidance and build closure | Runs/evidence docs; gateway setup README; Rust/Python example READMEs; removed example bins and smoke task; shared/Python/Skald/example manifests and lockfile deletions; route-decoder rustdoc | The diff removes runtime dependencies and examples together. Native gateway examples describe application ownership and correlation; no compatibility path or new dependency/configuration surface was introduced. |

## Material finding

### MNT-001 — VIOLATION — the newly application-owned model calls omit their partial-progress/cancellation contract

- **Changed locations:** `examples/support-desk/rust/support_desk.rs:169–181` (`answer` rustdoc) and `examples/support-desk-app/rust/lib.rs:109–120` (`SupportDesk::answer` rustdoc).
- **Governing rule:** AGENTS.md §16 requires new/materially modified Rust items to document relevant cancellation and partial progress; the maintainer guide's documentation section specifically explains that an accepted remote operation may survive local cancellation. Missing required rustdoc is explicitly blocking.
- **Source evidence and reachable path:** The public example's `serve` awaits `answer`, which writes the gateway request at lines 190–197 before awaiting its JSON response at line 199; ticket recording and Verifier submission happen later in `serve`. The HTTP example's `ask` and `review` await `SupportDesk::answer`, which sends at lines 136–144, reads at line 146, and only then records the ticket at lines 157–162. Dropping either future after dispatch can therefore leave gateway inference/capture completed without the later ticket or verification work. Recalling `answer` performs a fresh inference; the correlation headers are attribution, not a replay fence. The removed `HttpTransport::post_native` and `PublicWyrdGatewayCaller::call` documented this same ambiguity explicitly.
- **Concrete maintenance cost:** These helpers now own the externally visible side effects that the deleted execution layer previously explained. Their docs describe only successful execution and error variants, so a maintainer adding cancellation handling or retrying after a body-read failure has no local warning that inference may already have happened and later evidence may be incomplete.
- **Minimum safe correction:** Add concise cancellation/partial-progress rustdoc to these two changed functions: abandoning local IO does not roll back a dispatched model call; a missing reply does not establish non-execution; the HTTP helper can have a completed call without its ticket record, and the public helper's caller performs subsequent observation/verification separately. Do not add retries, deduplication state, cancellation machinery, or a replacement execution abstraction.
- **Closure proof:** Inspect the two function docs against their existing send/read/record order. A prose-only correction needs no new test or harness.

## Simplicity assessment

The main change spends its complexity budget correctly: it deletes the public runtime and its SDK dependency cone, removes native and language projections together, and reuses established authentication and gateway correlation in application code. No additional abstraction or optional refactor is required by this review.

## Verification limits

The task file reports format, lint, package, typing, unit, journey, and preservation results. Those outcomes were treated as claims; this pass inspected source and selectors but did not rerun Cargo, builds, generators, or runtime tests, per orchestration instructions. The support-desk-app library is identified in task evidence as documentation-only and uncompiled by the recorded lanes.

Skipped: independent runtime/build verification. Risk: cancellation or an uncertain response may leave a completed gateway call without later application evidence; the finding requires documenting that existing behavior, not changing it.
