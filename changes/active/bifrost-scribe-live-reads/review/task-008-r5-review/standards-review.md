# Independent repository standards confirmation review

Overall result: **FAIL**, for one bounded leftover-reference proposal below. FIND-007-11 and FIND-007-12 are closed. No runtime standards defect was established; the approved shutdown decision stands.

## Immutable subject and limits

Candidate `1a66d8a7b4583c9798c2f1573dc6ab1f3eadb027`; correction parent `ca99db0af5a0d898ef67834699405c1c73719f56`; cumulative TASK-007 base `a7582db587c6170a290760f1741673125612b797`; TASK-008 base `f7bebf704d6f3b1dd20d041e70c6ca512c0da307`. HEAD was rechecked unchanged. Inputs: subject, cumulative/correction diffs, revision-20 spec, original TASK-008, R5 task/evidence, prior standards report and governing authorities. This is a confirmation review under the user's constrained scope, preserving prior findings and fixed decisions. No current sibling report was read.

Static only: no cargo, nextest, mise, build, test, benchmark, source modification or commit. Supplied verification is attributed evidence, not fresh verification. No CodeGraph directory exists. Only this assigned report is written.

## Authority coverage

The reference router is `architecture/references/README.md`; its hierarchy makes the current user decision and owning Bifrost design controlling. All surfaces retain AGENTS.md, agent-rules and spec-driven-development governance. The cumulative inventory includes adjacent TASK-006 benchmark/harness work; its unaffected behavior is not reopened by this confirmation review.

| Cumulative changed surface / owner | Governing authority and routed rules | Confirmation source coverage |
|---|---|---|
| Pure provider cut, assignment digest, audit enum and generated schema (`wyrd-spec`) | AGENTS §§2–6,9,11–12,16; wyrd-design/doctrine; bifrost-design tenant/source contract; architecture/patterns, doctrine/architecture-constraints, rust-core, errors | `vala/api.rs`, `assignment_authority.rs` v6 cut encoding, managed-column exports, audit enum/schema copies; R5 changes none of these |
| Private protobuf/conversion (`wyrd-tonic`) | Same contract authority; agent-rules artifact rules; testing-workflows | Removed cap fields remain reserved in proto; private conversions agree; generated descriptor unchanged by R5 |
| Shared published/hot/staged scan and live/follower/resource/error paths (`vala-bifrost-redux`) | bifrost-design planning/tenant/terminal/resource authority; security-posture; datafusion, olap-serving, iceberg, arrow-analytical-interop, analytical-operations-reliability; errors/rust-core | Cumulative scan/footer/projection and live ownership inventory; shared remote classifier source and exact selector; no R5 decoder/tenant/error changes |
| Memory, seal, staging, writer, schemas and Forge tenant-footer producers (`vala-bifrost-redux`) | AGENTS §§3–6,9–12,16; bifrost-design authority switches; analytical domain references | Envelope deletion and footer producers remain in cumulative candidate; R5 shutdown retains flush/drain, lane closure, WAL finalization, stage restoration and claim reconciliation owners |
| Server topology, root ownership and peer transport (`wyrd-server`, `wyrd-tonic`) | AGENTS §§3–6,9–12,16; bifrost-design lifecycle; architecture/patterns, rust-core; security-posture; operations/reliability-and-recovery | `BoundServer` peer/public listener calls, `build_peer_grpc` role/service composition, supervisor cancellation ordering, `StoppingIo` full body/connect info, shutdown/startup/tick callers |
| Shared client HTTP default and schema copies (`wyrd-client`) | AGENTS client-tier/public-contract rules; wyrd-design client composition; testing-workflows | Cumulative default/config/schema/docs inventory; R5 leaves this surface unchanged |
| Real server/peer journeys and retained benchmark/testing topology (`wyrd-testing`, nextest, manifests/mise) | AGENTS §11; TESTING.md; testing-workflows/spec-driven-development | Exact four focused names exist under recorded targets; server peer suite evidence; Scribe lifecycle restart assertions; supplied redux/Scribe/server lanes. No new language boundary or harness |
| Architecture, reference prose, user documentation, task evidence | AGENTS §§1–2,12,16; spec-driven-development; current user decision and bifrost-design | Authoritative shutdown paragraph updated; explicit flush documentation updated; remaining shutdown references inspected across architecture/docs/Scribe/tests; historical packet records remain historical |

No Python, TypeScript, PyO3, N-API or UI source/declaration change occurs in R5. The cumulative subject does not introduce an alternative SDK implementation. The user-fixed error, Postgres tenant columns, no-cap decision, FIND-007-3 and no-shutdown-publication decision are not proposals for revision.

## Applicable rule results

| Rule | Evidence | Result |
|---|---|---|
| Correct owner/dependency boundary; no new foundational cost (AGENTS §§2–3) | Listener selection uses existing Bifrost capability at `app/server.rs:681`; both existing runners stay in tonic; shutdown remains an inherent Scribe owner operation; no new feature/dependency | PASS |
| Cohesive owner and real async IO (AGENTS §§5–6) | `StoppingIo` owns socket/cancellation future; listener branch awaits actual serving IO; no zero-sized utility or duplicate workflow | PASS |
| Public and Oracle-only transport preserved | Public task still uses graceful runner; Oracle-only branch uses that same runner; Scribe-containing branch retains stopping IO; grpc composition mounts Scribe service from same role owner | PASS statically |
| mTLS identity preserved | `ConnectInfo = TcpConnectInfo`; `connect_info()` delegates accepted socket identity; cancellation does not alter TLS authority | PASS statically |
| Required per-item rustdoc and Errors (AGENTS §16, agent-rules, rust-core) | All five IO fallible methods have substantive Errors sections; shutdown delegation exception is stated; associated type documents TLS wrapping | PASS; FIND-007-11 CLOSED |
| Exact named test command evidence (AGENTS §11, spec-driven-development) | R5 records exact classifier lib selector (1 pass) and exact three oracle journey selectors under Postgres wrapper (3 pass); names/targets match source | PASS; FIND-007-12 CLOSED |
| Durable transition/recovery preservation | Shutdown removes only forced residue sweep; shard flush/drain and persistence drain precede lane close; finalizer closes WAL streams without deleting stages. Startup restores stages/reconciles claims before WAL readiness. Restored ready times drive `take_claim` dwell and supervised publish tick | PASS statically; supplied recovery-related lanes |
| Explicit flush retains publication behavior | `flush_staged` still drives `publish_residue(Drain)`; worker documentation now distinguishes explicit flush from shutdown. Deleting those remaining methods would break a legitimate caller | PASS |
| Authoritative design reflects maintainer decision | bifrost-design.md:323–328 explicitly preserves staging for restart and forbids shutdown deletion | PASS |
| Leftover documentation accurately describes new lifecycle | Secondary reliability reference and lifecycle-test commentary still assert shutdown residue publication | FAIL; RSTD-R5-1 |
| Verification/constraint integrity | Supplied fmt/lints, focused checks, peer 11/11, redux 872/872, Scribe 20/20, server 26/26 and both whitespace bases; no weakening/skip/allow added by R5; static-only limit respected | PASS as supplied evidence |

## Proposed finding

### RSTD-R5-1 — leftover shutdown publication descriptions contradict the approved lifecycle

Classification: REGRESSION (documentation). Governing obligation: the user's item-2 explicitly includes leftover references; current Bifrost shutdown authority and AGENTS documentation-correctness principles require descriptions to reflect the selected durable boundary. This proposal does not challenge shutdown leaving staged members on disk and does not demand publication during shutdown.

Exact source evidence:

- `architecture/references/domain/analytical-operations-reliability.md:42` says shutdown "closes residue claims". That claim belonged to the removed forced residue sweep; shutdown now only stages/drains admitted work.
- `crates/wyrd/wyrd-testing/tests/bifrost/scribe/lifecycle.rs:150` says "A pod told to stop owes its staged rows a publication." Its later commentary attributes publication-versus-replay to the bounded shutdown drain, though post-restart tick publication is now the intended owner.
- The private worker-field comment at `scribe/persistence.rs:575` still describes the retained worker as permitting drain residue publication. The real surviving use is explicit flush. `scribe/mod.rs:1471` still refers to waiting "for shutdown" in due-publication documentation.

Producer-to-consumer trace: R5 deletes `shutdown -> publish_staged_residue -> publish_residue(Drain)`. Shutdown still reaches durable stage through flush/persistence drain; startup restores staged indexes and `BoundServer`'s scanner calls `publish_due` under target/dwell. Explicit `flush_staged -> publish_residue(Drain)` remains valid. The stale reference/test commentary presents the deleted shutdown behavior to future maintainers, creating a false guarantee and misleading interpretation of restart tests. No data-loss or restart-publication runtime failure is asserted.

Smallest correction: remove the obsolete shutdown-publication statements or replace them with the already-approved staging/restart boundary, and distinguish explicit flush from shutdown in the existing comments. Preserve all executable code, assertion semantics, durable files, claims and published behavior. No new helper, test or check is needed. Historical implementation/review records should not be rewritten as if they described today's contract.

Closure proof: static search/source inspection verifies active descriptions assign stage preservation to shutdown, stage restoration to startup, and due publication to the tick; inspect that explicit flush documentation still describes its surviving publication caller. This is specifically a leftover-reference correctness correction, not speculative documentation cleanup outside item 2.

## Disposition

No new runtime standards finding. FIND-007-11 and FIND-007-12 are closed; FIND-007-10's Oracle-only selection correction is present at the producer boundary and remains for behavioral/system acceptance. **FAIL** solely for RSTD-R5-1, subject to independent finding validation and reconciliation of the user's correctness-only scope.
