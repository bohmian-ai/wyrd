# TASK-001 review subject and coverage

- Round: first task review; complete base-to-candidate range. No prior acceptance
  or closure evidence is carried forward.
- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/variant-file-reads`.
- Base: `5234961d51798581a026834cfffd5efaadc226b4`.
- Candidate: `9e8f1d9d0d38fa4f33bbcdffc9d656fa08bb3994`.
- Subject diff: `git diff 5234961d51798581a026834cfffd5efaadc226b4...9e8f1d9d0d38fa4f33bbcdffc9d656fa08bb3994`.
- Authority: `changes/active/variant-file-reads/spec.md`, approved revision 2;
  original task `tasks/TASK-001-memory-variant-execution.md` beside that spec.
  Their untracked state does not permit modifying them during review.
- Authority SHA-256: spec
  `6709bdcd455c6b669ebd335af83412d5065622825795ac229dd2e2b6a01dfa9f`;
  original task
  `e49724f93a33de8ff6b0f808ba4125ad4742f579188085230898ff79557e55d8`.
- Available verification: appended task Implementation Evidence and its command
  results. Treat diagnoses and pass claims as assertions to verify against source.
- Source worktree has no tracked production modifications. Existing untracked
  change packet and unrelated benchmark-scaling deletions are outside the source
  subject. Reports are the only permitted writes. No patches/diffs/archives here.
- No `.codegraph/` exists in this checkout.

## Navigation map

This is a starting map; reviewers must independently trace actual producers,
consumers and sibling paths rather than assume coverage from this inventory.

- `oracle/live.rs`: `LiveScribeExec` absorbs projections and exact static filters;
  `LiveUnionBoundary` moves computed projections through live-containing unions.
  Consumers include source routes, physical optimizer rules and fragment dispatch.
- `oracle/codec.rs`: `RemoteScanPayload`, `RemoteScanOutputs`, placeholder output
  schema, `with_requirements`, `preflight_extension`, codec version 2. Producers
  are live/remote placeholders; consumers are follower preflight/resolution/decode
  and peer integrity tests.
- `oracle/follower.rs`: `signed_requirements`, placeholder payload discovery,
  `ScribeTailResolver`, `live_leaf`, schema validation, staged wrappers. Both
  in-process and remote execution consume the same authenticated requirement path.
- `scribe/memtable.rs`: `ReadableSource::project`, `logical_column/logical_batch`,
  `ReadRequirements` and `AdaptedRequirements` evaluate physical expressions with
  logical fallback. Retained captured generations and statistics feed the memory
  source. Test-only reconstruction counter is process-wide.
- `scribe/tail_rpc.rs`: requirements travel into retained capture reads;
  `scribe/shards.rs` and persistence tests update read callers.
- `wyrd-testing/tests/bifrost/oracle/distributed.rs`: real learned-memory query
  journey, local and role-separated cluster variants, privilege/cancellation proof.
  `support.rs` shares row rendering moved from `published.rs`.
- `peer_network/live_subquery.rs`: stale printed-plan assertion replaced by payload
  decoding; appended task evidence records the diagnosis. Verify report evidence
  and preservation of the original scalar-subquery assertion.
- Unchanged dependencies needing tracing: Oracle scan closure, peer dispatcher and
  authenticated plan fingerprint; capture accounting/lease owners; Parquet staged
  consumers; DataFusion projection/filter rewrite and Variant adaptation behavior.

## Specialist applicability

| Material contract/invariant | Owners, producers and consumers | Review selection and reason |
|---|---|---|
| Ordered exact expression results; Variant semantics/fallback; all original task acceptance | Live projection/filter producer, codec, follower, memory evaluator, SDK result | `behavior-rev`: full-task adversarial review, negative/regression paths and proof coverage. |
| Capture lifetime, generation ownership, shared Arrow charges, statistics after filtering/projection, cancellation | Scribe capture and memory owners, live/follower consumers | `invariant-rev`: expression execution changes where batches are filtered/projected under retained shared state; trace governing state and sibling consumers. |
| Rust contracts, rustdoc, synchronous/async boundaries, tier dependencies, test placement and assertions | Changed runtime/codec modules, cross-crate test/support consumers | `repo-rev`: scope spans Scribe, Oracle, codec and harness; mandatory changed-item rules require complete coverage. SDK/SQL/generated contracts are unchanged but verify that claim. |
| Existing-owner reuse, new requirement/adaptation structs, optimizer boundary extension and public preflight visibility | LiveUnionBoundary, ReadRequirements/AdaptedRequirements, codec/follower workflow | `maintainer-rev`: substantial execution/abstraction changes require whole-workflow comparison with existing owners, not a line-count judgment. |
| Request failure scope, mixed-build refusal, remote cancellation/recovery and healthy sibling availability | Versioned codec, peer/live/follower path in mixed and role-separated server topology | `system-rev`: version 2 and moving exact evaluation to Scribe materially change failure propagation and rolling-build behavior. |
| Tenant/table/version proof, authenticated expression/body integrity, pre-IO refusal and schema closure | Placeholder producer, dispatcher proof, follower parsing/validation, retained tail and SDK caller | `domain-rev-security`: executable expression payload crosses an authenticated server boundary; exact filters replace leader evaluation. Security/tenancy expertise is required. |

No separate persistence/migration domain reviewer: no writer, WAL format,
learning policy, SQL or persisted format diff; behavioral/state reviewers must
confirm unchanged durability and flag evidence invalidating this omission.
No separate concurrency domain reviewer: no new locking/scheduling protocol is
apparent; retained shared state and cancellation are explicitly assigned to
invariant/system reviewers, which must identify any materially new protocol.
No Python/TypeScript specialist: no SDK contract or implementation diff; repo
reviewer verifies surface/dependency closure rather than assuming path absence
alone establishes non-applicability.

## Reviewers and reports

- `behavior-rev`: `task-review-behavior.md`.
- `invariant-rev`: `task-review-invariants.md`.
- `repo-rev`: `standards-review.md`.
- `maintainer-rev`: `maintainer-review.md`.
- `system-rev`: `system-review.md`.
- `domain-rev-security`: `domain-review-security.md`.
- After discovery: fresh `ponytail-rev` writes `findings-validation.md`, validates
  applicability/coverage and each proposed finding/correction against source.
- Follow-up is conditional on a material conflict/untraced path, not a finding
  count. Final verdict and any remediation use only independently validated facts.

No reviewer may edit reviewed source/task/spec or run Cargo concurrently. Review
is static unless the orchestrator coordinates a particular focused test. Record
verification limitations explicitly. Recheck candidate and authority identity
before verdict; a changed candidate blocks this review.

## Supplemental evidence received during review

The maintainer identified the original task as the diagnosis report location and
added the verbatim independent diagnostician report and disposition under its
existing Diagnoses section. Current task SHA-256 is
`dc933d52d5111145011fd431ecc2a88c2b29c54d0006f69e20987eb8103bdf9b`.
Removing precisely that added report/disposition reproduces the original task
hash above. Approved obligations, task instructions, source diff, spec and
candidate are unchanged. This is supplemental verification evidence, not a new
review subject or authority revision. Reviewers/validator must consume it and
source-check it; no prior acceptance is assumed from the diagnosis alone.

## Discovery claim comparison and follow-up disposition

All six selected discovery reports are present. Group proposals by invariant:
INV-001 concerns newly retained query allocation accounting; SYS-001 concerns
execution snapshotting of absorbed scalar outputs; SEC-001 concerns closure
validation before tail access; RSR-001 concerns public Rust execution surface
scope; RSR-002 concerns mandatory changed-item rustdoc. The initial missing
diagnosis concern is closed by supplied evidence and independent source checks.

Behavior and maintainer reports propose no material findings. Their broader
PASS results do not resolve the targeted specialist proposals. In particular,
the maintainer's interpretation of public Rust visibility differs from RSR-001;
the validator must adjudicate that from the approved public-surface boundary
and actual callers rather than count votes. Allocation consequences observed
by multiple reviewers share INV-001's correction boundary, not separate issues.

No discovery follow-up is selected at this point: each proposed path has a
concrete traced producer/consumer and authority, and the public-visibility
interpretation can be reconciled from available source/authority. Fresh
independent validation must decide every proposal and applicability/coverage.
If validation discovers a new blocker or cannot resolve a material path, obtain
the required separate focused source validation before retaining it.

## Post-verdict maintainer clarification

The maintainer explicitly required all affected bytes/memory to be tracked and
attributed through the core Bifrost-owned budget/tracker, with one source of truth
for accounting rather than two independent trackers. The remediation and verdict
now spell out core-backed reservation handles, query ceilings/attribution as views
of the same ownership, and core snapshot/no-double-charge proof. This is human
direction reinforcing the existing resource invariant, not a new FIND ID or a
change to the immutable reviewed source, original task or approved specification.
The independent findings-validation report remains unchanged.
