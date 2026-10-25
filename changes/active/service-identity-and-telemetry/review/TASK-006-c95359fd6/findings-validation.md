# TASK-006 independent findings validation

Recommendation: **FIX_REQUIRED**. No source-proven runtime defect or architectural drift survives review. Three mandatory execution/documentation findings remain; their correction boundaries are evidence recovery and three rustdoc sections, not a production redesign.

## Subject, independence, and evidence

- Base: `5fe833547c66916c8c4868afc8e07d985dcba7d7`.
- Candidate and checked HEAD: `c95359fd608fa65d4f0422820751490f61c99202`.
- First review, cumulative range, no prior ledger or remediation IDs.
- Read the task-review skill, scope, all four selected discovery reports, and `verification.md`. Independently inspected the cited actual source, relevant cumulative diff, original task/evidence, approved revision 16 REQ-045/046 and AC-019/020, and applicable execution/rustdoc authorities. No `.codegraph` exists.
- All selected reports exist and cover their assigned boundaries. Maintainer/domain specialists were not selected; no missing report is inferred for them. The invariant report's pending-consumer note is superseded by the later serial rerun in `verification.md`.
- No source, authority, tests, or original task evidence was changed. This report is the only assigned write. No Cargo command or additional execution was performed by this validator.

## Proposal decisions

| Discovery proposal | Decision | Reconciled disposition |
|---|---|---|
| B-006-01, REPO-01: skipped observed RED | CONFIRMED, deduplicated | One execution-proof finding, FIND-TASK-006-1. Both reports identify the same missing evidence and chronology, not two defects. Narrow recovery to existing mapped scenarios; do not manufacture failures for behavior already correct at the base. |
| B-006-02: absent required independent diagnostician | REVISED | FIND-TASK-006-2 retains the original process violation but narrows recovery. Fresh behavior review already independently validates the four recorded causes against source. Reuse that report as retrospective assessment rather than commissioning a duplicate assessment or restoring invalid assertions. Original pre-fix review and unavailable traces cannot be retroactively claimed. |
| REPO-02: three missing panic sections | CONFIRMED | FIND-TASK-006-3. This is the explicit changed-item rustdoc gate, including test helpers; its small burden does not waive it. |
| Invariant/system empty ledgers | Validated | Their source paths and covered obligations reconcile with behavior/standards coverage. No additional runtime finding is retained. |

There is no material contradiction requiring focused follow-up. This validator discovered no new blocker. No optional improvement or rejected proposal becomes remediation work.

## Reconciled acceptance and execution coverage

The discovery matrices supply detailed source/test mappings; this table reconciles them without repeating comprehensive discovery. PASS below distinguishes reviewed implementation and recorded execution from independently repeated execution.

| Obligation | Implementation evidence | Verification evidence and limit | Result |
|---|---|---|---|
| REQ-043/045, AC-017/019: whole authorized exact-ID trace, identity/order/projection/ceiling | `CallerTraces::statement/fetch` retains explicit columns, exact trace predicate, start/span order, ordinary caller query and 10,001 probe; removes only time predicates | J1 source and implementation-recorded GREEN; independent source/diff confirmation here | PASS |
| One collection per trace per attempt, concurrent/later checks, separate trace IDs and fresh later runs | `VerificationEngine::run` creates fresh `TraceReads`; `read` shares `Result` in a TraceId-keyed OnceCell and releases map lock before IO; `trace` consumes that result | U1 independently 1/1; engine module 24/24. Sequential-stage/error-sharing behavior additionally source-proven in invariant report | PASS |
| REQ-046: queued absence only, three attempts, 30/60-second PostgreSQL due times, release waiting worker | `trace` defers only NotYetLanded when enabled; runner supplies `attempt < max_attempts`; existing retry/claim owner retained | I1/I2 real engine tests inspect due times and attempts; recorded GREEN. No new durable retry owner or deadline introduced | PASS |
| Accepted identity/authority/inputs/attribution/idempotency survive retries/restart; no intermediate result; fenced final publication | Existing accepted Caller/claim/settlement path unchanged; retry branch does not build a result; final attempt uses ordinary decide and completion | I1/I2 and delegated-history proof; invariant/system reports trace producers, sibling consumers and publication fence | PASS |
| Exhausted absence produces normal requirement-level inconclusive; preserve mixed decisions, direct absence, invalid/auth/oversized and timeout behavior | Direct constructor disables deferral; only NotYetLanded enters it; existing normal aggregation and per-attempt timeout retained | I2/I3 and existing exhaustion/deadline tests; exact deferral unit independently 1/1. I3 proves defaults rather than injecting a new slow-query experiment | PASS |
| Nonempty evidence judged immediately; late spans cannot change completed results | Successful read never defers; existing completion state unclaimable; later execution gets fresh map | J2 source and recorded GREEN | PASS |
| INV-001/002/007: tenancy, accepted caller query authorization/audit | Reader reuses ScheduledQueryCaller and statement caller, not a second SQL/auth owner | J1 foreign-tenant/refusal and delegated-history assertions; invariant and standards review | PASS |
| INV-006: Rust/HTTP, MCP, Python, TS public projections; changed fixture consumers | Server owns behavior; SDK changes are journeys and existing canonical fixtures | J/P/T/M inspected and recorded GREEN. Independent Rust fixture run 21/23 plus unchanged focused serial 2/2; all 23 have passed across runs, not one full serial rerun | PASS |
| Existing owners, non-goals, architecture, tiers/generated contracts | Complete scope covered by behavior/standards; production diff extends existing engine/reader/runner only; no new persisted/wire contract/dependency/feature | Source/diff and owner comparisons; formatting/lints/types/boundaries are implementation-recorded passes | PASS except explicit rustdoc row below |
| Mandatory RED/GREEN execution proof | New behavioral implementation preceded tests; no observed RED supplied | Task footer explicitly admits U1/J1/I-test omission; source inference is not an observed failure | FAIL, FIND-TASK-006-1 |
| Independent pre-fix diagnosis before failure-driven assertion/test repairs | Four self-diagnoses recorded, no original independent diagnostician report | Fresh behavior report and this validation now substantiate causes; this recovers retrospective diagnosis, not original order/traces | FAIL, bounded record correction FIND-TASK-006-2 |
| Mandatory changed-item rustdoc | Three new helpers omit `# Panics` while delegating to panicking fixture owners | Exact bodies/callees/callers checked below; prose gate needs static closure | FAIL, FIND-TASK-006-3 |

The initial two Rust consumer failures do not establish a source regression: both use named policy-review code checks with optional trace omitted, so new trace-fetch/deferral paths are not reached; both passed unchanged under the required serial invocation. `TESTING.md:265` explicitly requires serialization for shared telemetry. No test/assertion change cleared those review-time failures. Their initial concurrency sensitivity remains a stated execution limit, not a fourth finding or a requirement to rerun the full suite.

## Final deduplicated ledger

### FIND-TASK-006-1 — CONFIRMED — VIOLATION: missing observed behavioral RED proof

**Discovery sources:** B-006-01, REPO-01.

**Obligation/location:** `changes/spec-driven-development.md:293–314` requires an exact test to fail for the intended missing behavior, then GREEN; implementation-execution's integrity section adopts that rule. Original TASK-006 scenarios prescribe these observations. Its evidence at `tasks/TASK-006-verification-trace-evidence.md:270–273` states that production preceded tests and U1/J1/I-test RED was not run.

**Source proof and consequence:** The base-to-candidate diff independently confirms actual new behavior: base `VerificationEngine::trace` fetched on every check, base `CallerTraces::statement` imposed seven-day/one-day bounds, and base runner did not configure not-yet-landed trace deferral. Candidate `run/read/trace`, reader statement and runner dispatch change those exact behaviors. U1 checks fetch counts, J1 includes a 30-day-old and future span, I1 requires retry/restart, and I2 requires the three-attempt final decision. Current GREEN establishes candidate behavior but cannot observe those tests discriminating the original missing behavior. This is a mandatory proof violation, not a conjectured product failure.

**Smallest correction and burden:** Recover the missing executable negative evidence in an isolated disposable comparison using the existing focused tests and repository harness; keep the reviewed candidate immutable. Compare the recorded base behavior with candidate behavior, adapting only private test/constructor plumbing when necessary and recording the exact adaptation. If a full base build is incompatible, use a clearly identified temporary reversal of the specific production behavior outside the candidate, rather than building a new harness. Observe intended assertion failures, not setup/compile failures. Preserve the same semantic assertions on the candidate and record exact selectors, selected counts and outcomes. Start at the mapped missing-behavior proofs U1/J1/I1/I2; classify I3/J2's already-correct cases as regression proof instead of demanding invented failures. Account explicitly for the task's P1/T1/M1 mirrored projections in the recovery traceability, retaining their existing focused public proof rather than substituting a broad suite.

Deletion of production code, assertion weakening, a new service, fixture framework, dependency or broad journey run is neither needed nor authorized. Source-only argument is insufficient because the missing obligation is observed execution. Recovery cannot change the original sequence: retain its disclosed deviation, and never call retrospective counterfactual execution an original pre-implementation RED.

**Focused closure proof:** An evidence record mapping the affected scenarios to actual intended negative observations and corresponding focused candidate GREEN, with baseline/reversal identity, any private plumbing adaptation and chronology stated. If a scenario cannot be executed equivalently, record the precise incompatibility and unresolved gate; do not claim PASS from source inference or silently exempt it. No specification/public behavior revision is needed to obtain this proof.

### FIND-TASK-006-2 — REVISED — VIOLATION: original failure-driven test repairs lack the prescribed independent diagnosis record

**Discovery source:** B-006-02.

**Obligation/location:** `.agents/skills/wyrd-implement/SKILL.md`, “Diagnose unexpected failures,” requires a fresh read-only diagnostician before a test/assertion repair and its report beside the diagnosis; task-review's implementation reviews explicitly preserve this integrity gate. The four repairs are documented at TASK-006 evidence `:283–303`, without an original diagnostician report. The orchestrator confirmed that neither original failure logs nor an original diagnostician artifact were supplied beyond that footer and committed diffs.

**Independent source proof of the four causes:**

1. `pg_verification_runtime.rs::sent_result_id` (`:1548`) explicitly asserts one distinct result per submitted batch. The shared completion owner supports group completion (`verification/completions.rs:104–125,169–209`); the three I2 submissions need not produce three batches. Current `Harness::verdicts` (`:4972`) decodes every row and selects its result ID, and each I2 run asserts exactly its expected verdict. The revised all-writes-to-RESULTS assertion preserves the destination invariant without imposing an unrelated one-batch-per-execution assumption. Existing sent_result_id callers remain unchanged for their original single-result cases.
2. `WyrdTestServer::retained_records` (`server.rs:2012–2041`) deliberately maps TableNotFound to an empty vector. It cannot prove a missing-table precondition by returning an error. `Harness::start_with(false)` (`pg_verification_runtime.rs:105`) passes false to `seed_tenant` (`:504`), which skips built-in creation; I1 creates the table explicitly only after its first absence attempt. Removing that invalid observation preserves the actual missing-table scenario and does not justify adding a second inspection API.
3. `QueryAuthority::admit_capability` (`query/service.rs:84`) admits BifrostQuery/Read capability. `trace_reader` (`pg_verification_runtime.rs:4848`) grants that capability, whereas the original seed used by the denied I3 branch does not. Current I3 uses the latter, asserts first-attempt completion, and checks an inconclusive verdict. No authorization rule should be tightened to make a mistakenly privileged test caller fail.
4. J1's Verifier authors expected names including retrieve (`verification_runtime.rs:5913–5933`); an echoed expected value is not leaked observed evidence. Current denial assertions (`:5952–5972`) require both requirements inconclusive and each reason naming BifrostQuery/Read refusal. Restoring the old substring assertion would confuse authored expected data with query output and weaken the diagnosis.

**Consequence/materiality and revision:** The original pre-fix independence gate was missed; this is factual and mandatory even though the causes and corrections are sound. However, B-006-02's proposed fresh retrospective diagnostician would repeat substantive work now performed by the separate fresh behavior reviewer in its “Failure diagnosis validation” section and independently checked here. There is no evidence that production semantics were changed to hide a test failure, nor justification to revert these corrections.

**Smallest correction and burden:** Append one compact recovery entry to the implementation/remediation evidence, linking the existing `task-review-behavior.md` independent diagnosis section and this source validation beside the four diagnoses. State explicitly that both assessments are retrospective, original traces were unavailable, and the original requirement to diagnose independently before fixing was not followed. Reuse the focused corrected tests' recorded GREEN and the focused proof obtained for FIND-TASK-006-1; rerun only where needed for that recovery or a further actual edit. Do not demand a new agent/report merely to change its title, reconstruct unavailable trace text, add production guards, or restore the disproven assertions.

**Focused closure proof:** Truthful linked recovery evidence for all four pairs, with current independent source assessment, available corrected-test results and original chronological limits. This closes the missing retrospective assessment/record; it cannot certify an original pre-fix review. If policy disposition of that historical ordering is required, identify that unresolved historical issue explicitly rather than asserting that a later report changes the past. No new product or architecture decision is implied.

### FIND-TASK-006-3 — CONFIRMED — VIOLATION: mandatory panic contracts omitted on three new test helpers

**Discovery source:** REPO-02.

**Obligation:** Root AGENTS Code says changed Rust items need `# Panics` when they can panic and missing rustdoc blocks merge. `agent_docs/rust.md:234–241` expressly covers private/test helpers. These helpers are reachable in the required tests; test-only placement does not exempt this explicit obligation.

**Locations/source paths:**

- `crates/wyrd/wyrd-testing/tests/bifrost/server/verification_runtime.rs:5684`, full `trace_batch` body: forwards supplied schema/trace/spans to `canonical_signals::trace_spans`. J1 and J2 call it for real native writes, including 10,000/10,001-span cases.
- `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/verification.rs:2351`, full `write_spans` body: describes the real table, builds span tuples, calls that same canonical producer, then encodes/ingests/flushes; M1 is its caller. Its existing `# Errors` does not describe panics.
- `crates/vala/vala-verify/src/blocks/engine/tests.rs:1763`, full `shared_trace_verifier` body: builds the four-check spec and calls existing `verifier` (`:200`), whose deserialization/validation expect calls and `# Panics` define the valid-fixture invariant. U1 and the deferral unit are its consumers.

Both span wrappers call `canonical_signals::trace_spans` (`:545`), which documents schema assembly panic and delegates to `batch` (`:128`). `batch`/`column` (`:145`) panic for schema/cell mismatch or batch assembly. The wrappers inherit these invariants without documenting them. For the constant spec, fixture validity is the named invariant: there is no claim of a supported public input causing a crash. The explicit documentation gate is the material basis.

**Smallest correction and burden:** Add truthful `# Panics` documentation to these three existing helpers describing their current fixture/schema validity invariants. Reuse the existing lower-level panic contract. Do not delete required helpers/tests, add runtime guards, convert fixture panics to product errors, or redesign assembly to avoid a documentation change. Batch the correction with the same evidence remediation, not a separate style-only round.

**Focused closure proof:** Static inspection of all three complete helper rustdocs and their existing callees, plus the owning Rust format check and diff check. No runtime test, new lint mechanism or broad suite is justified for these prose additions.

## Verdict boundary and limits

Recommended ledger: **FIND-TASK-006-1, FIND-TASK-006-2, FIND-TASK-006-3**, all VIOLATION. No runtime, security, tenancy, durability or availability defect was established; no SPEC_REVISION_REQUIRED decision is identified. Findings 1–2 concern recoverable evidence/records while preserving the truthful unrecoverable original chronology. They must not be rewritten as a claim that historical pre-fix work occurred.

Independent execution is exactly `verification.md`: U1 1/1, engine module 24/24, deferral unit 1/1, server build, diff check, and all 23 Rust fixture consumers passing across the original suite and unchanged serial failure rerun. There was no full serial rerun. Other journeys/lints/types are implementation-recorded results, not independently rerun here. Recovery must add the missing focused negative proof; it must not repeat every already-green journey or broad aggregate.

Skipped: broad rediscovery, extra Cargo runs, optional test variants and production changes. Risk: current GREEN/source validation does not replace missing observed RED or establish the original pre-fix review order.
