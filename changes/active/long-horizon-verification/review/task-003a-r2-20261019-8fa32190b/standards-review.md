# Repository standards review

Result: **FAIL** — one generated public-documentation gap; no implementation redesign is requested.

## Subject and method

Reviewed cumulative `22335c2c40a5ad24c27bfa0d4686f4da49f922c5...8fa32190b26b11588721e8db44b3eb94139e3739`, using `8376bb2aa..8fa32190b` to locate R2 corrections. HEAD remained the candidate. Read the original task, revision 20 authority, R2, AGENTS, agent rules, reference router and applicable references. Used CodeGraph first for GroupPoller/Scheduler/queue/InputSlot, then actual diffs, surrounding owners and consumers. Prior review reports are historical evidence, not authority to preserve superseded behavior.

This is the independent repository-standards pass, not another acceptance or resilience review. No source edits, Cargo builds, tests or PostgreSQL lanes were run by this reviewer. Root's current `verification.md` supplies fresh focused test/check results. A read-only rendering comparison below independently establishes the documentation defect.

## Authority and changed-surface coverage

| Changed surface | Governing authority inspected | Source and consumers inspected |
|---|---|---|
| Rust declarative contracts/decoders | AGENTS §§2–6,9,16; agent rules; `references/architecture/patterns.md`, `languages/rust-core.md`, `languages/errors.md`, `doctrine/positioning-and-vocabulary.md`; active design Verifier/Trigger/reference contracts | `trigger.rs`, `card/verifier/mod.rs`, `reference.rs`, `verification.rs`, `verification/execution.rs`, `error.rs`; InputSlot remote decoder and source wrapper, TriggerDuration parsing/schema/test, inline reference callers |
| Server grouping/registration/runtime | AGENTS §§3–6,9–10,15–16; agent rules; patterns/rust-core; active design §§Verifier, Runtime identity, Coordination clock; `references/domain/evaluation.md` | `components/cards/service.rs`, `verification/{groups,scheduler,authority,claims,runner,engines,mod,hosted/mod,operators,fault}.rs`, `config.rs`; builder composition, shared execution semaphore, accepted execution authority, scheduled query caller |
| Outbox and analytical reads | AGENTS §§2,6,9–10,15; agent rules SQL/audit rules; active design audit/Bifrost; `bifrost-design.md` Durability and visibility and Read audit/terminal contract; `references/domain/{olap-serving,analytical-operations-reliability}.md` | `scribe_outbox.rs`, boot composition, gateway capture/peer-audit test callsites, `query/scheduled.rs`; typed row extraction, Scribe ACK scheduling transaction, normal query admission/terminal handling |
| SQL, migrations and persistent-state proof | AGENTS §§3,11,15–16; agent rules TenantConn/OperatorPool/caller-owned transaction/RLS; patterns/rust-core; active design Coordination clock; security posture Tenant/data isolation | `queries/{verification,verifier_executions}.rs`; modified binding/execution migrations and new pending/refusal migrations; SQL integration tests, row/status decoder, group enqueue/refuse/consume/settlement writers |
| Security/identity/audit projection | AGENTS §§2,9; agent rules; active design runtime accepted authority/internal reads; `wyrd-security-posture.md` principal lifecycle, authorization, peer identity, tenant isolation and audit integrity | BackgroundAuthority rejection/unavailability branches; SystemReadAuthority resolve/serving; GroupPoller table grants and exact source predicates; scheduler pre-admission refusal; source execution producer identity and status consumers |
| Rust journeys, server/SQL/CLI/MCP tests and testing exports | AGENTS §§11–12,16; agent rules; `TESTING.md`; `references/languages/{testing-workflows,maintainer-style,agent-harness}.md` | Rust verify-in-real-time journey helpers and process lifetime boundary; server PG group tests, openapi and registration test changes; SQL P2 proof and savepoints; MCP verification tests; CLI port reservation caller; `wyrd-testing` shared reserver and local-outbox callsites |
| Python declarations and journeys | AGENTS §§7–8,11,16; `references/languages/python-api-and-stubs.md`, testing-workflows, maintainer-style | authored `verify/__init__.pyi` GroupSnapshot/ExecutionState dictionaries; support fixture rename and verify-in-real-time tests using public Python modules. No PyO3 wrapper/signature/export addition occurs in this range |
| TypeScript wire projection and journeys | AGENTS §§2,8–9,11–12; agent rules generated-source ownership; `references/languages/typescript-guide.md`, errors, testing-workflows, maintainer-style | generated `card-types.ts` GroupSnapshot/Trigger shapes, `error-codes.ts` catalog addition; verify-in-real-time typed tests and helpers. No N-API signature change occurs |
| YAML fixtures and prose/generated docs | AGENTS §§2,11–12,14; agent rules generated docs; design vocabulary; `references/languages/{spec-driven-development,agent-harness,errors}.md` | renamed group Verifiers and all referencing fixtures; invalid fixture relocation; current architecture group prose; how-verification-works/runtime-limits/error catalog docs; source generator and owning mise lane |
| Active planning and historical reports | AGENTS §14; spec-driven-development | current revision 20 task packet, original task and remediation evidence; cumulative historical reports retained as evidence and not imposed over revisions 19/20 |

No manifest/lockfile dependency addition, new Cargo feature, UI implementation, provider integration, public URL-fetching behavior, or Python/Node native implementation is introduced by this range. Their specialized boundary changes are therefore not independently implicated.

## Rule results

| Rule | Result and evidence |
|---|---|
| Server owns durable behavior; contracts stay declarative and IO-free | PASS. Pending activity, claim, refusal and settlement remain server/SQL methods. TriggerDuration/InputSlot/reference decoding remains synchronous pure Rust in wyrd-spec. SDKs project resulting wire shapes |
| Concrete cohesive ownership; no ad hoc runtime | PASS. GroupPoller retains AppState/deadline; Scheduler retains PG/operator/authority/capacity; queue retains policy. Pure row selection stays synchronous. Foreign runtime work starts actual Python/Node processes rather than embedding their lifetime in Rust |
| Tenant capability types in actual production signatures/fields | PASS. `GroupPoller::poll`, queue activity/claim/consume/refuse/enqueue/status methods take `&mut TenantConn<'_>`. Due-tenant/depth methods take `&OperatorPool`; Scheduler stores `OperatorPool` and WyrdPostgres. Scribe scheduling opens through WyrdPostgres. No raw pool/connection/transaction library parameter was added |
| Caller owns tenant transaction; RLS owns tenancy | PASS. GroupPoller and queue methods do not commit/rollback supplied TenantConn. Scheduler and sink own connections they open and commit their own handoff/activity transactions. Pending table enables and forces RLS, tenant isolation and operator policy. Tenant queries do not add a parallel tenant predicate |
| PostgreSQL coordination clock | PASS. Activity deadline uses SQL statement time plus extracted epoch seconds; claims/overdue use statement time; Rust does not compare coordination timestamps against Utc::now. New DST test calls real activity owner; root P2 passes |
| Existing migration identity preserved in R2 | PASS. New `20261019000000_verification_refused_shape.sql` replaces the existing constraint without rewriting the already-applied 20261018 migration. Normal authority-present alternative remains unchanged |
| Stable derive-backed errors and SDK fields | PASS for runtime/TS projection. New SnapshotIncomplete has metadata in WyrdError and generated TS union; GroupSnapshot fields match Rust, Python authored dictionaries and TS. No language-specific durable error contract added. Generated documentation has the failure below |
| Missing rustdoc closure | PASS. New Slot visitor associated Value and expecting/visit_str/visit_map hooks now explain behavior; all three fallible hooks have Errors sections. Changed group/SQL/test helpers inspected carry docs, including error and relevant panic/cancellation descriptions |
| Permission/audit paths retained | PASS within standards boundary. Internal table read authority uses existing resolver/query service; execution retains recovered owner authority. Audit remains staged on the shared outbox; no audit publisher or request-blocking audit write added |
| Test/runtime tier and exact verification discipline | PASS. Relevant Rust PG tests use real managed dependencies; Python and Node behavior executes in own runtimes; integration tests remain credential-gated. Root sequentially reran P2 (1 selected/pass), P1 (2 selected/pass), fmt/lints/tenant-isolation/diff check. This reviewer did not claim fresh full SDK/MCP suites |
| Generated public documentation matches source | FAIL. New public snapshot error is omitted from generated errors page, as independently reproduced below |

## Proposed finding

### STANDARDS-1 — INCORRECT: generated error reference omits the new public snapshot error

- **Obligation:** AGENTS §12 requires public contracts to regenerate cleanly; agent rules prohibit maintaining generated outputs separately from their sources. `references/languages/errors.md` and `agent-harness.md` require derive-backed public catalogs. The existing docs lane explicitly enforces generated reference drift.
- **Producer:** `crates/wyrd-spec/src/error.rs:1930–1950` defines `VerificationSnapshotIncomplete`, code `WYRD_VERIFICATION_409_SNAPSHOT_INCOMPLETE`, status 409 and remediation. Scheduler emits it for an incomplete snapshot past the approved five-minute bound.
- **Consumer/location:** `docs/src/content/docs/api/errors.md:502–520`, WYRD_VERIFICATION table (missing sorted row immediately before line 514), lacks that code. `docs/scripts/generate_api_docs.py:44–73` reads all derive metadata and `render_errors()` renders this exact missing row. `mise.toml:1294–1301` runs docs generation and rejects resulting tracked drift.
- **Evidence:** Read-only source rendering comparison returned `catalog_matches: False`, with exactly this missing generated line:

  `| WYRD_VERIFICATION_409_SNAPSHOT_INCOMPLETE | 409 | Verification snapshot incomplete | Check Bifrost publication for the named source executions, then submit a new turn to the group. |`

- **Consequence:** A caller receiving the newly shipped terminal error cannot find it in the advertised complete public error reference; the existing docs drift gate cannot pass the candidate as recorded. This is generated/public contract documentation drift, not a speculative prose preference.
- **Minimum correction:** Run the existing docs generation owner and commit its changed consumed outputs, including this catalog row; do not hand-edit a second error list, add a generator or change runtime behavior. Combine with any independently confirmed current-revision architecture/how-it-works corrections in the same documentation-only remediation boundary.
- **Focused proof:** Repeating the read-only comparison must return true. The owning `mise run docs:check` must pass after committing generated outputs. No runtime/journey rerun is needed for this correction alone.

Read-only inspection used (no files written):

```bash
/usr/bin/python3 - <<'PY'
import runpy
from pathlib import Path
module = runpy.run_path('docs/scripts/generate_api_docs.py')
actual = Path('docs/src/content/docs/api/errors.md').read_text()
expected = module['render_errors']()
print('catalog_matches:', actual == expected)
print('missing_generated_lines:')
for line in expected.splitlines():
    if line not in actual.splitlines():
        print(line)
PY
```

## Non-blocking notes and limits

Security posture's descriptive internal-read paragraph at lines 116–122 still mentions only Drift observations while approved revision 19 and active design now include narrow group snapshot reads. This is stale description of the same approved authority, not evidence of a runtime security violation or a request for another permission mechanism. The security specialist independently agrees the paragraph does not prohibit extending the existing narrow table-read owner. It fits any consolidated documentation alignment correction.

Small remaining source wording/import-placement inconsistencies (for example a function-scoped test import in reference.rs and the C13 reference in a group test helper comment) have no established behavior/contract consequence and are not blocking findings or a reason for another style round.

The three R2 corrections themselves satisfy their repository-standards obligations. Full SDK/MCP journeys, docs generation/build, codegen and benchmarking were not rerun by this reviewer. Root's verification log and task evidence remain the dynamic proof sources; no PASS for an unrun aggregate is implied.
