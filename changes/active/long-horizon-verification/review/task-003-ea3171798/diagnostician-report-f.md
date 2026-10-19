# Retrospective diagnostician report for the R1 HTTP test failures (R2 / FIND-TASK-003-6)

**Provenance.** A fresh read-only agent produced this report on 2026-10-18, after both failures had been fixed. It received only three things: the failing command, the recovered output in [r2-http-failure-evidence.md](r2-http-failure-evidence.md), and the first passing commit `ba12fe8f1`. It was not given the implementer's hypothesis, and it ran no tests. The text below is verbatim.

**Limits.** The report is retrospective and checks present source only. It does not prove what the implementer knew before either fix. No full historical trace survives; only the excerpts in the evidence file do.

---

# Report F

## Failure 1: `queued()` read 0, expected 2 (old line 518)

**(a) Cause.** The assertion ran before anything could be queued. `VerificationControl::enqueue` (`crates/wyrd/wyrd-server/src/components/verification/service.rs:674-688`) reserves a submission slot and a Scribe slot, then only stages `ScribeWrite::ExecutionInputs` with the `Submission` attached. Its rustdoc at lines 598-603 says the submission is queued only after Scribe acknowledges the inputs. The hand-off is in `scribe_outbox.rs:948-957` (module doc at 25-28): only after a frame is acknowledged and not lost is the submission staged on `SubmissionOutbox`. `submissions.rs:1-9` then writes it to Postgres asynchronously. Scribe was still stalled, so 0 queued rows was correct production behaviour.

**(b) Fix site.** The test. Production behaved as designed. ba12fe8f1 moved the count below `stall.release()`, `wait_completed()`, `flush_bifrost()` and `visible_execution` polling for both admitted executions. That is the right place.

## Failure 2: `Audit("retained audit projection is neither text nor Variant")` (old line 475)

**(a) Cause.** `CAST(execution_id AS VARCHAR)` plans as `Utf8View`. In the pinned DataFusion fork, `sql_parser.map_string_types_to_utf8view` defaults to `true` (`~/.cargo/git/checkouts/datafusion-44e2ebb5250b7f6f/b8ec4b1/datafusion/common/src/config.rs:296`). Nothing in `crates/` overrides it, and `OracleSessionShape::apply` (`vala-bifrost-redux/src/resources.rs:4108-4125`) does not touch it. `audit_cell_text` (`wyrd-testing/src/server.rs:143-160`) only downcasts to `StringArray` (Utf8) at line 146, so a `StringViewArray` falls through to the error at 149-152. The column itself is declared `utf8("execution_id", false)` (`execution_inputs.rs`, `arrow_fields`), so dropping the cast avoids the problem.

**(b) Fix site.** The root owner is the shared decoder `audit_cell_text`. Accepting `Utf8View` there would fix it for every caller. ba12fe8f1 instead fixed the caller's SQL and made `retained_records` `pub`. That works, but it is a symptom fix: the trap is still there, and the error text still says "audit" even though `retained_records` is now a general reader.

## Siblings

- **Same as Failure 1, in the commit itself.** ba12fe8f1's final `queued() == 3` came after `input_rows`. `flush_bifrost` (`server.rs:1231-1254`) settles only the Scribe outbox, not `SubmissionOutbox`, so the recovered execution may not have been in Postgres yet. That is a race. Commit ce13c1cce later fixed it with a sentinel enqueue plus `visible_execution`.
- **Positive counts that are already gated correctly:** `enqueued_executions_replay_conflict_and_read_back` (line 429, gated by visibility of later arrivals), and lines 719 and 1011 (each after `visible_execution`).
- **Zero counts with no settle first:** lines 636, 1049, 1166 and 1270. Because of the async hand-off, these could pass even if a submission were wrongly staged.
- **Readers of the same decoder:** `retained_audit_records` (`server.rs:1956`) and `retained_span_records` (1983) share it. Their rustdoc tells callers to "cast each scalar column to text"; a caller who uses `CAST(... AS VARCHAR)` would hit Failure 2. No current caller does: they use plain text columns or `encode(..., 'hex')`, which return Utf8.

## Preserved assertions

The correction keeps all of these:
- 429 status, `WYRD_VERIFICATION_429_STAGING_FULL`, and `retry_after_seconds = 1`, asserted while the stall is held.
- "the refusal queued nothing" (2), now after the drain.
- The keyed retry is accepted.
- `input_rows == (3, 1)`.
- The replay returns the same execution.
- The final execution count, which was 3 in ba12fe8f1 and is 4 with the sentinel at HEAD.

The stall-based setup that fills the share deterministically is also kept.

## Limits

- Only the excerpts in `r2-http-failure-evidence.md` survive. No full trace remains, so I cannot confirm the actual Arrow schema at failure time, or which writes the two "outbox is full; the staged item was lost" lines dropped (probably the requests' audit or span items).
- The Utf8View cause is inferred from present source and the pinned config default; it was not reproduced, and per your instruction I ran no tests.
- I also did not confirm that the plain `execution_id` projection decodes as `Utf8` on both the Scribe-held and published read paths. The passing runs imply it does.
- A present-source check shows what the code does now. It does not prove what the implementer knew or reasoned before either fix.