# Oracle remediation boundary

Subject: `ff42598100b0edabad5cad2272a3c7735f2eef14..3cc2052d977c94f52a11548f887ea548c2d6f297`.
Source remained immutable. Reviewed the direct remediation diff, prior Oracle
report and validated FIND-TASK-002-3, supplied R2 acceptance evidence, approved
working-tree specification revision 2 REQ-006 and original TASK-002.

Authority: root AGENTS, `agent_docs/index.md`, Rust and maintainer-style guides,
TESTING, task-review procedure; Bifrost design Variant SQL, Nested-field reads
and Distributed analytical execution; security posture Resource and retained-data
visibility and Peer identity and distributed Oracle. No nested Vala AGENTS exists.
For the production dependency correction, also inspected the fork's root
`AGENTS.md`. The maintainer explicitly excludes benchmark qualification.

## Source and proof

- Wyrd `Cargo.toml:300–310`, all DataFusion lock sources and
  `oracle/codec.rs:32` agree on
  `1a0f1af0bc6ce686e5fd25aaee000bd88f6ca17a`.
- Both `/home/thorrester/Documents/GitHub/datafusion-scalar-snapshot` (branch
  `wyrd-production`, clean worktree) and Cargo's fetched checkout
  `/home/thorrester/.cargo/git/checkouts/datafusion-44e2ebb5250b7f6f/1a0f1af`
  resolve to that exact commit. Only one commit separates it from the prior
  `5f4b1713af4f0394c7c564b6bc6de42e92640186` pin.
- That commit changes the existing `CastExpr` physical encoder/decoder,
  `PhysicalCastNode` protobuf and generated Rust/JSON projections, plus its
  in-module regression. It introduces no Wyrd wrapper or alternate policy path.
- Fork `physical-expr/src/expressions/cast.rs:419–479` serializes explicit
  metadata and `Option<bool>` nullability. `Some(false)` and `Some(true)` both
  reconstruct an explicit target field; absent nullability preserves the prior
  type-only constructor. Field naming remains derived from the source by
  `resolved_target_field`, as before. Cast option handling is unchanged.
  The protobuf's tags 3/4 and both generated projections agree.
- Fork regression `proto_round_trip_keeps_an_explicit_target_field` checks
  metadata, nullable target and return field, and confirms a plain cast remains
  type-only. Existing cast tests exercise ordinary codecs; the supplied record
  reports 29 cast and 286 proto tests passing. These are implementer results,
  not independently rerun executions.
- Wyrd `oracle/codec.rs:755–857` now plans each query under payments-family
  authority and executes locally and after actual production native encoding
  and decoding with `OraclePhysicalExtensionCodec`. Its source has payments,
  other-family and unknown-parent rows. Exact `p1`/`a1` results prove other rows
  stay hidden for row selection, Variant key read and key predicate. Whole
  Variant output has one row, the local extension tag must be
  `arrow.parquet.variant`, and the peer's entire Field and ArrayData must equal
  local. This directly detects the documented pre-fix extension loss, including
  target nullability parity. No previous null-mask assertion was removed.
- Production `encode_follower_subtree` uses those same native APIs, and
  `PhysicalPlanFollower::decode`, `follower.rs:931–1000`, uses that decoder with
  role-local authenticated providers and complete consumption. The test does
  not replace cast serialization. Its in-process source substitution is enough
  for this specific codec defect; it makes no deployment/lifecycle claim.
- `physical_plan_fingerprint` and `peer::stage_body_digest` both bind the changed
  revision through the existing shared helper. The pin consistency regression
  remains present. This retains existing cross-build refusal discipline.

Supplied R2 evidence records the exact focused nextest selector passing, 959
`vala-bifrost-redux --lib` tests passing, and Wyrd fmt/lints clean. The recorded
RED is a runtime Field mismatch with otherwise equal whole-Variant values, which
matches the old type-only codec and the owner correction. No runtime checks were
rerun in this specialist pass. Reviewed-range `git diff --check` was clean.

**Behavioral closure:** FIND-TASK-002-3 is supported. No new runtime defect,
security widening, persistence change or unnecessary abstraction was found.
Carry forward the earlier local authorization tests; this report does not reopen
shredded pushdown or claim file-pruning measurements.

## Proposed ORA-R2-1 — mandatory dependency-owner lint proof

Classification: **VIOLATION / mandatory gate evidence gap**, requiring independent
validation. No production correctness defect is asserted.

Authority: R2's Observable closure and verification requires affected owner
format/lint checks. The fork's `AGENTS.md`, Before Committing, explicitly says
contributors **MUST** run and fix `cargo clippy --all-targets --all-features --
-D warnings` before committing, and not commit code failing its required checks.
The same section requires formatting. This is an owner requirement, not an
optional whole-dependency quality improvement invented by review.

Location and trigger: Wyrd `Cargo.toml:300–310` adopts production fork commit
`1a0f1af0bc6ce686e5fd25aaee000bd88f6ca17a`, whose changes are essential to closing
the peer finding. The user explicitly reports that the fork's clippy was not run
clean. R2 supplies fork unit results and clean Wyrd lints, but no clean fork gate.
Wyrd workspace lints do not establish the dependency owner's all-target,
all-feature lint result.

Expected: the production correction has clean required owner validation.
Actual: a named mandatory owner check remains non-clean/unestablished.
Consequence: behavior is credibly repaired but mandatory qualification of the
changed production owner cannot be reported complete. This is not evidence that
clippy warnings imply incorrect Variant behavior.

Smallest closure: obtain and record a clean owner clippy run for the exact
pinned revision under the applicable owner environment. If an owner correction
is necessary, make it there, preserve codec semantics, update all Wyrd pins and
fingerprint together, then rerun the focused codec proof and affected checks.
Do not repair historical test-first ordering, invent another codec, or add product
scope. Do not mark Wyrd lints as proving fork lints. The validator must settle
gate applicability before retaining this proposal.

## Waits, scope and process

Added/moved hot-path external waits: **PostgreSQL none; Scribe none; object
storage none; Vault none**. The fork correction only copies target metadata while
encoding/decoding an existing expression. It adds no IO, lock across await, pool,
runtime, catalog access, audit blocking or durability wait.

TDD for this repair has credible supplied runtime RED-before-fix and GREEN;
not independently witnessed history. Preserve prior Alice test-first violations
separately; this repair cannot erase them.

Skipped runtime reruns, real multiserver execution, benchmarks and full fork lint
execution. Codec behavior is supported; the proposed remaining risk is mandatory
dependency-owner lint qualification, pending independent validation.
