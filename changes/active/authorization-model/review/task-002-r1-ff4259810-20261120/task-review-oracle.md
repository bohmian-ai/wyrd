# Oracle boundary review

Subject: immutable remediation `29a9d276f5e668df56f5f16e898a73f7aefc46a0..ff42598100b0edabad5cad2272a3c7735f2eef14`; cumulative base `300542877cd2a58cbe5f061d5075616e4cc4397b`. Read the actual diff, current owning source and its consumers. No implementation edits or execution reruns.

Authority: SPEC-authorization-model revision 2 REQ-006; root AGENTS; `architecture/bifrost-design.md` §§ Variant SQL, Nested-field reads, Distributed analytical execution and Public surface (managed visibility); `architecture/wyrd-security-posture.md` §§ Resource and retained-data visibility and Peer identity and distributed Oracle; `changes/spec-driven-development.md`; `agent_docs/maintainer-style.md`; TESTING. No nested AGENTS exists in the affected Vala tree. Applied `wyrd-task-review` and ponytail review discipline. The supplied approved working-tree spec, prior subject and remediation explicitly exclude benchmark qualification: preserve that exclusion; no measured-performance acceptance is claimed.

## Navigation and coverage

- `Oracle::plan_logical`, `oracle/mod.rs:3275–3296`: SQL plan → managed-row rewrite → parameter binding/optimization → payload authorization.
- `restrict_verification_rows`, `oracle/mod.rs:3933–4013`: every managed verification scan and subquery; existing exact/family parent filter and CASE masking; newly wraps each CASE in a cast to its own nullable Field.
- `verification_session`, `oracle/mod.rs:5157ff`: unshredded Variant documents on payments, other and unknown-parent rows in results/evidence.
- `verification_rows_are_filtered_by_the_readers_card_scope`, `oracle/mod.rs:5297–5425`: exact/family/table-only/tenant-wide, counts, join/subquery/CTE and failing hidden-row expression. New lines 5395–5403 execute four Variant key expressions, all returning `a1` for payments.
- Alice connected journey, `ui-pages.integration.test.ts:2771–2826`: actual retained execution_inputs through the TS SDK/CLI/MCP; line 2800 directly checks `inputs -> 'head' ->> 'sha'`. Python checks identifiers/counts rather than Variant decoding. Its member remains pending; no judged result/evidence/event is produced.
- `encode_follower_subtree`, `oracle/codec.rs:76–83`, delegates native physical encoding. `OraclePhysicalExtensionCodec` handles source placeholders, without replacing native cast encoding.
- Existing `a_masked_verification_plan_executes_identically_on_a_peer`, `oracle/codec.rs:754–836`, roundtrips a scoped physical plan, but selects only `result_id`; it does not consume or assert Variant fields.
- Exact pinned dependency: Cargo.toml:300–310 and `oracle/codec.rs:32` identify DataFusion `5f4b1713af4f0394c7c564b6bc6de42e92640186`. Inspected the available checkout under `/home/thorrester/.cargo/git/checkouts/datafusion-44e2ebb5250b7f6f/5f4b171/` for native expression/plan serialization and per-file projection analysis.

The local correction sits at the shared owner and preserves the parent predicate, unknown-parent hiding, CASE masking, table qualification and nullable result. It introduces no second authorization mechanism, SDK behavior, storage format, schema contract or migration. The four new Variant cases exercise actual optimization and collection, rather than just asserting a logical schema. Supplied O1 RED/GREEN and 221 Oracle units match the assertions present. I found no demonstrated local filtering regression.

## Proposed ORA-1 — EVIDENCE_GAP: scoped Variant compatibility across the peer codec

Requirement: REQ-006 requires managed reads to apply identically across local and peer execution; the Bifrost public surface requires the same boundary on memtables/Parquet/live/peers, while Variant SQL requires every Variant result to retain `arrow.parquet.variant` on the wire.

Location: newly introduced field-typed cast at `oracle/mod.rs:3996–3997`, consumed by the native physical plan codec at `oracle/codec.rs:76–80`. Missing assertion boundary is `oracle/codec.rs:754–836`.

Starting condition and trigger: a Card-family reader queries a managed verification Variant column, and its masked projection is dispatched to a peer; e.g. a projection returning `inputs`, or a key expression whose masked child is serialized. Expected: the peer result preserves local values, null masking, field metadata and Variant decoding.

Available proof establishes local key reads only. The implementer explicitly records peer serialization of this field-typed cast as unverified. Source inspection shows this is a material compatibility seam:

1. Pinned `datafusion/physical-expr/src/expressions/cast.rs:409–429` encodes `PhysicalCastNode` with the child and `arrow_type` only, omitting the explicit target Field.
2. Its decoder at lines 432–461 reconstructs `CastExpr::new`, not `new_with_target_field`.
3. `CastExpr::resolved_target_field`, lines 214–249, deliberately removes extension metadata for a type-only cast; the explicit-target branch preserves it.
4. Pinned `datafusion/physical-plan/src/projection.rs:634–724` serializes projection expressions/names and rebuilds the projection with `try_new`, without a separate output schema preserving that lost field metadata.

Consequently the new local guarantee that the expression itself owns the Variant Field is not preserved by the native roundtrip. The existing peer test's string projection cannot detect this. This is sufficient to require focused executable compatibility proof; this report does not assert that every `->>` query necessarily fails: decoded scalar functions retain their own return fields and their invoke implementation decodes array values without re-running argument-field validation. A whole Variant result's extension field is the direct concern.

Consequence: a supported peer query can have different Variant field semantics/consumer decoding from the local query; local happy-path tests cannot qualify the peer boundary.

Smallest testable closure: extend the existing scoped codec roundtrip test/fixture to return a masked Variant whole column and read it by key, assert output field extension metadata and values/null masking equal local, then execute through the existing production peer query boundary as narrowly required if the codec-level check exposes a difference. Correct any demonstrated loss at the native codec owner, retaining the pinned-revision/fingerprint discipline; do not add Wyrd Variant functions, a custom row-policy engine, or unrelated production changes merely for missing proof. This is a proposal requiring independent validation, not a retained finding ID.

## Shredded reads and waits

The scoped mask already existed before this range, including the CASE barrier between literal-path functions and the source. The per-file projection analyzer in pinned `datasource-parquet/src/projection_read_plan.rs` follows declared paths on direct columns and recognized field chains, and conservatively visits arbitrary expressions (including CASE children). Neither the new unit nor Alice's memtable journey proves shredded leaf selection. I did not establish a new cast-caused regression in leaf pushdown: do not create a new optimization finding merely from this unverified limit or reopen unchanged masking without a demonstrated supported regression. The limit must remain explicit.

Added or moved external waits in this Oracle diff:

| External owner | Added/moved waits | Evidence |
| --- | --- | --- |
| PostgreSQL | None | synchronous plan rewrite only; no catalog/cache acquisition edits |
| Scribe | None | no ingest, live-discovery, audit or durability changes |
| Object storage | None | no provider/read call-site changes; constructing the cast performs no IO |
| Vault | None | no credential or secret access |

There is one additional expression/Field construction per projected field, no new lock/await/runtime/client/pool. No benchmark was run under the explicitly excluded qualification; no latency, allocation or throughput improvement is claimed.

Behavioral assessment: credible local Variant repair; peer Variant compatibility remains a material proof proposal for independent validation. TDD: preserve supplied O1 failing collection before its production cast correction separately from Alice assertions written after exploratory probes. No historical RED reconstruction is possible or required.

Skipped Cargo/journey reruns, benchmark qualification and shredded-file measurements. Peer Variant metadata preservation is not established by the existing string-only roundtrip test.
