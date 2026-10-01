# Focused follow-up: shutdown residue descriptions

**RESOLVED.** There is one bounded leftover-reference correctness proposal. It concerns false current descriptions of shutdown, not durability, replay, restart publication, or a reason to restore the deleted sweep.

## Subject and scope

Candidate `1a66d8a7b4583c9798c2f1573dc6ab1f3eadb027`, parent `ca99db0af5a0d898ef67834699405c1c73719f56`; cumulative TASK-007 base `a7582db587c6170a290760f1741673125612b797`, TASK-008 base `f7bebf704d6f3b1dd20d041e70c6ca512c0da307`. Read the immutable subject, claim comparison, supplied skill, original task inputs, discovery reports and governing shutdown/reference authority. This pass investigates only the disagreement about leftover descriptions after the approved sweep removal. It does not reopen fixed decisions or independently repeat the complete discovery audit.

Static source inspection only. No cargo, nextest, mise, builds, tests, benchmarks, commits or source changes. No CodeGraph index exists. Supplied passing evidence is not fresh execution. HEAD was rechecked unchanged; only this assigned report was written.

## Source and caller evidence

| Current path or description | Source/callers inspected | Resolution |
|---|---|---|
| Shutdown stages and drains accepted generations | Full `ScribeImpl::shutdown`, `begin_shutdown`, lane closure and finalizer in `scribe/mod.rs:1315–1508`; correction diff | The removed branch alone called `publish_staged_residue -> publish_residue(Drain)`. Current shutdown no longer enumerates ready keys or starts the forced residue sweep. Its comment at :1369–1371 accurately assigns preserved stage to startup/tick. |
| Normal publication can occur during accepted-work drain | Full `PersistenceWorker::persist_once` at `persistence.rs:1890–1908`, `publish_due_claims` at :2006–2013, persistence queue drain at :905–917; server scanner at `app/server.rs:551–578` | A newly durable generation still drives target/dwell publication. A scanner publication already in flight may finish before Scribe shutdown. Keeping lanes open for admitted work to finish file-list publication is valid; it does not require sweeping every below-target ready member. |
| Explicit residue flush remains real | Full `ScribeImpl::flush_staged` at `mod.rs:2450–2469`; `PersistenceRuntime::publish_residue` at `persistence.rs:937–948`; full worker method at :2036–2058 | Explicit flush drains accepted staging, resumes retryable claims, enumerates ready keys and publishes residue using the existing fenced owner. This caller must remain. The newly corrected worker rustdoc at :2018–2022 accurately distinguishes it from shutdown. |
| Ready-key consumer chain | Full `StagingAssembler::ready_keys` at `assembly.rs:987–993`; `ScribeStagingRuntime::ready_keys` at `staging_runtime.rs:678–684`; repository caller search | Production enumeration is the explicit residue-flush chain. The second worker enumeration belongs to the partition-scoped test-support path; unit tests also inspect the index. No shutdown caller remains. |
| Restart and idle publication | Startup restore/reconcile/resume sequence at `mod.rs:2489–2518`; server lifecycle tick at `app/server.rs:551–578`; discovery recovery traces | The preserved stage is a normal restart input, and tick publication does not require another write. None of the stale descriptions removes this route. |
| Deleted helper references | Search across current `crates`, `architecture` and `docs` | No current `publish_staged_residue` symbol/reference remains. Historical task/review evidence is historical and requires no rewriting. |

## Disputed descriptions

Three descriptions are unambiguously incorrect after this candidate:

- `architecture/references/domain/analytical-operations-reliability.md:40–43` says shutdown “closes residue claims.” This current operational authority assigns shutdown the deliberately deleted sweep. Its adjacent statement about draining admitted publication remains valid and should be preserved.
- `scribe/assembly.rs:982–985` says that at shutdown every remaining ready key is claimed as residue rather than left for a dwell that will never expire. The sole surviving production enumeration belongs to explicit flush. Restart now supplies the runtime in which retained dwell expires.
- `wyrd-testing/tests/bifrost/scribe/lifecycle.rs:106–110` says graceful stop sweeps every held staged member into published objects and leaves nothing to rediscover. The comment at :149 says stopping owes staged rows publication. Neither is a guarantee the current shutdown provides. The actual test at :166–169 already permits zero published rows after restart, followed by exact public readback; therefore the claim is a false account of the journey's proof, not a failed executable assertion.

The remaining associated wording needs a narrower interpretation:

- `assembly.rs:437`, `ClaimCause::Drain`, describes settlement before shutdown. Its surviving producer is explicit flush (plus test-support), so it should describe that cause rather than promise a shutdown step. Keep the enum value and all callers.
- The worker field description at `persistence.rs:575` says drain can publish residue after the queue empties. A drained queue is still an explicit-flush precondition, so this is imprecise owner wording rather than an independent false runtime guarantee. Clarifying explicit flush would keep it consistent with the corrected worker method.
- `mod.rs:1471`, “instead of waiting ... for shutdown,” does not itself promise a sweep. The positive tick contract in that rustdoc is correct. This phrase is not sufficient primary evidence for a blocking proposal.
- Ordinary residue, `Drain` names, file-list publication during accepted-work drain, and the test's all-or-none publication assertion are not obsolete executable behavior. A search-and-delete correction against these would break valid adjacent behavior.

## Scope resolution and proposed finding

**FUP-R5-1 — REGRESSION (documentation): active shutdown descriptions promise the removed forced residue sweep.** This is the same issue as BEH-R5-1, INV-R5-001 and RSTD-R5-1, narrowed to the definite false guarantees above; it is not an additional independent defect.

The user explicitly requested correctness-regression review including “leftover references.” That term does not restrict review to executable references. Current operational guidance and workflow/test documentation are therefore in scope when they state a false shutdown guarantee directly caused by this deletion. AGENTS.md §16 treats documentation as implementation correctness, and owning `bifrost-design.md:323–327` now assigns residue preservation to shutdown and publication to the next process. The specialist reports correctly establish no executable consequence; that resolves their domain question without making these documentary guarantees true.

Observable consequence: a maintainer following the current reliability reference or the lifecycle journey's stated contract is told that graceful stop has settled all staging and that restart has nothing to recover. The candidate intentionally guarantees neither. This misstates the accepted durable boundary and what the retained test proves. No lost-row, duplicate-row, failed publication, broken test, or runtime availability consequence is claimed.

Smallest correction: remove the false guarantees or align their existing prose with accepted staging preservation and restart/tick publication; describe `Drain` as explicit residue flush. Preserve normal admitted publication, explicit flush, enum values, all runtime code, existing assertion outcomes and historical records. No new helper, abstraction, test, check or shutdown publication is required. Static source/caller comparison is sufficient closure proof for this documentary correction.

**RESOLVED:** false sweeping guarantees are within the expressly named leftover-reference scope; valid normal publication/flush references remain. Final independent validation must still adjudicate this single proposed finding against source and authority.
