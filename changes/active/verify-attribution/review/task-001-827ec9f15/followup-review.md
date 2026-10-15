# TASK-001 follow-up review

## Immutable subject

- Base: `ef4f6b0251e9d11c2712765f6db90545e40d590b`
- Candidate: `827ec9f15f64cf308749e17fcd5fe914acf64723`
- Approved authority: `changes/active/verify-attribution/spec.md`, revision 3
- Task: `changes/active/verify-attribution/tasks/TASK-001-distributed-trace-continuity.md`

This pass resolves only the conflicts assigned by the orchestrator. It does
not issue an overall task verdict.

## Conflict 1: complete producer context versus operator independence

### Resolution: reject BR-002 / PD-002; the omission is intentional

Gate is required to use the producer identities and their real sampled bit to
create valid causal links and decide tenant-copy eligibility. It is not
permitted to let the remaining client-controlled fields change the operator
span.

The controlling text is more specific than the generic statement that Gate
"adds valid links":

- `spec.md` INV-004 says client-supplied trace context never changes the
  operator export's trace content or sampling.
- REQ-007 says the linked-ingest tenant copy uses the operator's ids, parents,
  and links and that the operator copy is unchanged.
- `architecture/wyrd-design.md:773-780` says no client-supplied field,
  explicitly including `tracestate`, changes what the operator receives, while
  a sampled parent or link controls the additional tenant copy.
- The approved task's final acceptance wording at lines 701-705 separates
  operator independence from the bounded, retry-stable producer-context wire
  contract. The implementation summary approved by the owner likewise calls
  out the real sampled flags, not preservation of vendor state or reserved
  flag bits.

The implementation matches that split. `producer_links` validates a one-byte
wire flag and retains only `flags & 1`; `link_remote_span` constructs a remote
link with the producer ids, the actual sampled bit, and empty `TraceState`
(`gate/mod.rs:1279-1291`, `wyrd-telemetry/src/lib.rs:282-301`). Preserving the
wire `trace_state` or non-sampling flag bits on that link would necessarily
change the operator span's link content. Producing a richer tenant-only link
would instead violate REQ-007's requirement that the tenant links are the
operator's. The four wire fields still have a purpose at the queue/wire seam:
the complete context is sealed and resent byte-identically, while Gate applies
the approved server-side projection.

No implementation correction is warranted. If full vendor state or reserved
flags are ever intended to appear in persisted links, that is a specification
change to INV-004, REQ-007, and the architecture text, not a TASK-001 bug fix.
BR-002 and PD-002 are duplicates and should both be rejected.

## Conflict 2: Scenario 7 recovery and sibling proof

### Resolution: confirm BR-003 as a material focused-test gap

The production ownership is credible: `ServerSpanSink::stage` puts the owned
permit in `ScribeWrite::ServerSpans`, and the shared outbox keeps the same item
through retry and drops it on acknowledgement, terminal consumption, or
abandonment. Generic outbox tests support that reasoning. The task, however,
requires a stronger named proof and the current test does not provide it.

`scribe_outbox::tests::sustained_scribe_outage_caps_tenant_spans`
(`scribe_outbox.rs:1396-1430`) does the following:

1. fills an unreachable outbox and proves the 2,048 cap;
2. directly stages audit and capture, but not a verifier result;
3. immediately abandons that outbox at shutdown and observes permit release;
4. creates a different healthy outbox to prove one successful span releases
   its permit.

It never recovers the failed outbox, never proves those retained items drain
and return permits after retry, and never exercises HTTP, gRPC, MCP, or
verifier-result availability during the outage. The prescribed
`pg_verification_runtime::sustained_scribe_outage_caps_tenant_spans` does not
exist, although Scenario 7 RED and its exact command require it
(`TASK-001...md:352-361`). The evidence table's broad `pg_verification_runtime`
claim therefore does not close the named obligation.

Smallest correction: keep the production code unchanged. Add the prescribed
focused integration test using the existing test server and a controllable
Scribe seam. On one outbox, hold Scribe unavailable, exceed 2,048 tenant spans,
stage one verifier result, exercise the existing HTTP/gRPC/MCP probes, release
that same Scribe, wait for the retained items to drain, and assert all permits
return and a subsequent span is accepted. Audit/capture can remain covered by
the owner unit test; they need not be reimplemented in the integration fixture.
No new production option, limiter, or recovery mechanism is justified.

BR-003 has no duplicate in the other discovery reports. The invariant and
concurrency reports establish likely implementation correctness, but they do
not substitute for the approved task's missing cross-boundary proof.

## Conflict 3: producer contexts and `ClientByteBudget`

### Resolution: confirm CONDUR-001

The bypass is reachable and violates the queue's existing hard-memory
contract:

- `QueueConfig` defines `client_byte_limit_bytes` as the only ceiling for
  every client-owned row, sealed batch, in-flight send, and retry
  (`config.rs:9-17`). The producer deliberately uses an unbounded data channel
  and relies on this budget for admission.
- `Producer::enqueue_prepared` reserves only the Arrow `charge`, then retains
  an owned `TraceContext` in an `Arc` for every admitted logical record
  (`producer.rs:541-560`). `TraceContext` owns three `String` buffers, and its
  public shape does not itself cap `trace_state` (`wyrd-spec/src/trace.rs:8-17`).
- `RecordQueue::seal` clones up to 32 distinct contexts into each
  `SealedBatch`, then resizes the only guard to the IPC frame length
  (`queue.rs:586-594,665-683`). `SealedBatch` retains those uncharged strings
  through every send and ambiguous retry (`sink.rs:95-115`).
- Normal traced JSON writes reach this path through
  `transport::trace::producer`, and the public queue/context APIs also make a
  large context directly constructible. Thus this is not dead or
  attacker-only state.

There is no policy bound on the extra retained bytes: a caller can supply an
arbitrarily large public `trace_state`, and even ordinary valid W3C vendor
state is additional memory absent from `used_bytes()`. The 32-context seal cap
bounds contexts per batch, not the number or bytes of retained batches. During
an unavailable or ambiguous transport, the configured 256 MiB ceiling can be
exceeded while metrics continue to report at most the charged Arrow/IPC
owners.

Smallest correction: keep `ClientByteBudget` as the sole admission owner and
add no configuration or second limiter. Charge each optional context's owned
bytes with the logical record before handoff, carry that existing reservation
with each distinct context selected for `SealedBatch`, and release reservations
for contexts discarded by the 32-context seal rule. The retained reservation
must live through in-flight send, ambiguous retry, acknowledgement, terminal
loss, and shutdown exactly like the frame reservation. Reuse
`ClientByteGuard`; do not add a parallel byte counter.

One focused queue test is sufficient: use a small budget and distinct
non-empty states, hold an attempt ambiguously, and assert admission refuses
before `used_bytes()` can exceed the configured limit, the context charge
survives seal/retry, and terminal settlement returns `used_bytes()` to zero.
This test may extend the existing ambiguous-retry fixture; no new integration
harness is needed.

CONDUR-001 is independent of BR-003. One concerns queued client-ingest memory;
the other concerns server tenant-span outbox recovery proof.

## Verification limits

No Cargo, mise, database, or code-generation command was run during this
source-only follow-up. The conclusions use the immutable candidate source,
the approved task/specification, architecture authority, and all discovery
reports in this review directory.
