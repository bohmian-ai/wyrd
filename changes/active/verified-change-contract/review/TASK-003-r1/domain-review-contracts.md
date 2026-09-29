# TASK-003 Wave-1 Domain Review: Public Contracts and Card Composition

## Immutable Subject

- Base: `1609e102881dd55b12154248834a8aedbb7a36b2`
- Candidate: `467ea07d94a5a6d665d24f56afc0bc0a12532304`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 33
- Original task: `changes/active/verified-change-contract/tasks/TASK-003-binding-projection-and-runtime-activity.md`
- Reviewed boundary: `BindingId`; effective verification-binding resolution, validation, UID/digest freezing, and projection identity; unique Service component occurrences; server-derived Card status; generated Card/GetCard schemas; served OpenAPI coverage; existing Card GET behavior; and the tests that claim those public behaviors.

## Authority and Source Coverage

| Area | Authority reviewed | Source and consumer coverage |
|---|---|---|
| Binding identity and projection | Spec REQ-095, REQ-102, REQ-104, REQ-112; INV-001, INV-010; task scenarios 1 and 5 | `crates/wyrd-spec/src/ids.rs`; `crates/wyrd/wyrd-sql/migrations/20260601000027_verification_bindings.sql`; `crates/wyrd/wyrd-sql/src/queries/verification.rs`; `crates/wyrd/wyrd-server/src/components/cards/service.rs`; SQL integration tests |
| Composite resolution and validation | `architecture/wyrd-design.md` Doctrine 8, 9, 19, 21 and Registry lifecycle; spec REQ-090 through REQ-095, REQ-102, AC-018; `AGENTS.md` server/contract rules | `crates/wyrd-spec/src/graph/composition.rs`; `crates/wyrd/wyrd-server/src/components/cards/resolve.rs`; reference binding in `service.rs`; registration-route tests |
| Card status and HTTP read | Spec REQ-134 and binding-ID portion of AC-028; task scenario 4; `architecture/wyrd-doctrine.mdx`; positioning/vocabulary and architecture-pattern references | `crates/wyrd-spec/src/card/verifier.rs`; `crates/wyrd-spec/src/envelope.rs`; all three server Card read paths and `hydrate_card`; `wyrd-client::cards::Cards::{get,get_response}`; Python and TypeScript Card projections |
| Schemas and OpenAPI | `AGENTS.md` contract/codegen rules; testing-workflows OpenAPI rule; spec REQ-114, REQ-134, AC-020, AC-028 | generated Card and GetCard response schemas plus goldens; `crates/wyrd/wyrd-server/tests/pg_openapi_contract.rs` |
| Authorization/tenancy regression boundary | Spec REQ-145, INV-007, AC-030; `architecture/wyrd-security-posture.md`; agent SQL/RLS rules | Card route test's under-privileged and foreign-tenant cases; tenant-scoped status lookup; surrounding Card read handlers and shared client consumers |

Repository-wide authorities read were `AGENTS.md`, `architecture/agent-rules.md`, `architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`, `architecture/wyrd-security-posture.md`, the reference router, positioning/vocabulary, architecture patterns, error and testing references, the spec-driven-development workflow, `TESTING.md`, the approved spec/task, and the locked verification control-flow artifact. The complete base-to-candidate diff and surrounding callers/consumers in the reviewed boundary were inspected.

## Verification Limits

- This was a static, review-only audit. I did not rerun the candidate's reported `mise` commands.
- The task artifact reports passing Card, principal, SQL, Wyrd, journey, boundary, codegen, format, lint, and focused-test commands, but no raw command transcript is present in this review packet.
- Existing SQL unit coverage proves that `BindingSchedule::next_after` rejects a never-occurring schedule, but the public registration test exercises only a syntactically malformed cron and therefore does not cover the reachable defect below.
- No candidate test uses the real shared Cards client or a first-class language SDK to read the newly served binding IDs. No served-OpenAPI assertion covers the new status shape.

## Material Findings

### CONTRACT-001 — INCORRECT: registration accepts schedules that can never arm

- **Violated obligation:** TASK-003 scenario 2 requires the first qualifying exchange to initialize a null schedule cursor to the next future boundary, scenario 1 requires validation/write failures to leave no projection, and the task evidence claims an unarmable schedule is refused before writes. Spec REQ-112 requires the exchange to set the next future cron boundary. AC-018 requires fail-closed registration behavior.
- **Exact location:** `crates/wyrd/wyrd-server/src/components/cards/resolve.rs:214-221`; `crates/wyrd/wyrd-sql/src/queries/verification.rs:166-181,309-342`; `crates/wyrd/wyrd-server/tests/pg_card_registration_route.rs:2840-2857`.
- **Evidence:** registration calls only `BindingSchedule::parse(cron, tz)` and never calls `next_after`. `BindingSchedule::parse("0 0 30 2 *", None)` succeeds, while the existing unit test at `verification.rs:486-503` proves `next_after` returns `ScheduleError::NoOccurrence`. At the later qualifying exchange, `record_machine_authentication` catches that error, logs it, and continues, leaving `next_run_at` null. The route test named as proving an “unarmable schedule” sends only `"not a cron"`, so it cannot detect this path.
- **Observable consequence:** a Service or Agent can be successfully registered with a projected schedule binding and can authenticate successfully, yet the binding remains permanently unarmed and silently creates no scheduled work. The public Card status still exposes a valid stable binding ID, making the accepted but inert subscription appear usable.
- **Testable correction:** in the pre-write effective-binding validation owner, validate both parsing and existence of a future occurrence using the same `BindingSchedule` implementation and server clock semantics used for arming; return the existing stable invalid-card-spec refusal before opening the registration write transaction. Extend the real registration test with a syntactically valid never-occurring cron such as `0 0 30 2 *`, assert the stable 400 code and no Card, principal, binding, or operation write, and retain the malformed-cron case.

### CONTRACT-002 — MISSING: the new Card status contract has no real SDK-to-server journey

- **Violated obligation:** `AGENTS.md` section 11 and the testing-workflows reference require every new user-facing capability to ship a real SDK -> real server -> SDK journey; TASK-003 scenario 4 explicitly requires Card GET journeys for owner, component, reapply, authorization, and tenant isolation; AC-020 says integration tests support but do not replace the user journey.
- **Exact location:** `crates/wyrd/wyrd-server/tests/pg_card_registration_route.rs:2699-2716,2749-2768`; shared public consumer at `crates/shared/wyrd-client/src/cards/handle.rs:274-293`.
- **Evidence:** the only test that reads `card.status.verification.binding_ids` constructs an Axum request and calls `WyrdTestServer::oneshot_authenticated` directly. It never invokes `wyrd_client::cards::Cards::get`/`get_response` or a first-class SDK Card surface. Repository search found no Rust, Python, or TypeScript SDK test asserting this status field.
- **Observable consequence:** the handler and database seam are exercised, but serialization/deserialization and projection through the actual public client surfaces can regress while the claimed acceptance test remains green. In particular, a client could drop or mis-project the nested typed UUID list without this test detecting it.
- **Testable correction:** add the smallest gated real-client journey for each shipped Card GET surface that registers the composite through the public client, reads the owner Card back through that same SDK, asserts UUIDv7 binding IDs remain stable after reapply, and proves under-privileged and foreign-tenant reads retain their stable refusals. Keep SQL freeze-detail assertions in the existing integration test; they do not belong in the user journey.

### CONTRACT-003 — MISSING: served OpenAPI does not prove the new status shape

- **Violated obligation:** the `AGENTS.md` contract verification rule and `architecture/references/languages/testing-workflows.md` require OpenAPI changes to be proved against the assembled server's `/openapi.json`; spec REQ-134 makes `card.status.verification.binding_ids` public HTTP behavior.
- **Exact location:** `crates/wyrd/wyrd-server/tests/pg_openapi_contract.rs:457-521`; contract source at `crates/wyrd-spec/src/envelope.rs:459-476` and `crates/wyrd-spec/src/card/verifier.rs:186-200`.
- **Evidence:** the only candidate edit in the Card OpenAPI test changes the expected number of `Spec` alternatives from 16 to 15. It does not assert that `Status.verification` references `VerificationStatus` or that `VerificationStatus.binding_ids` is an array of UUIDs. JSON Schema goldens do contain the field, but the repository explicitly treats the runtime OpenAPI document as a separate proof surface.
- **Observable consequence:** the assembled HTTP documentation can omit or mis-shape the only public locator for inline bindings while codegen goldens and all current OpenAPI tests still pass, leaving non-Rust clients with a stale machine-readable contract.
- **Testable correction:** extend the existing assembled-server Card contract test to traverse `Card -> Status -> verification -> VerificationStatus -> binding_ids` in `/openapi.json` and assert the expected nullable/object and UUID-array shape. No new test harness is needed.

## Overall Result

**FAIL**

The typed `BindingId`, natural-key persistence, UID/digest freezing, unique component occurrence enforcement, tenant-scoped status lookup, and generated JSON schemas are otherwise consistent within the reviewed boundary. The accepted-but-never-armable schedule is a reachable behavior defect, and the required SDK journey and served-OpenAPI proof are absent.
