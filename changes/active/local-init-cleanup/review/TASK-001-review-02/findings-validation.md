# TASK-001 R1 independent findings validation

Recommendation: **PASS**. Both prior findings are closed by the bounded
remediation. The independently validated OPEN ledger is empty. No new
material blocker or unresolved source question was found.

## Immutable subject and review boundary

- Original base: `d7de12a74c4bdbf93e727cc2383147e34298f689`.
- Prior candidate: `862a69b187c81a5395bc73b872695a7bb0150330`.
- Code candidate: `050057ff4a0556db2669385e95bec289828df49a`.
- Evidence HEAD: `5aaa38937d76fef1a690cd931e3001363be69ddc`.
- Approved spec revision 1 blob: `440649c7563dede7724b56f358b3ca1d3ecd1e80`;
  independently checked the current spec hash. Its committed approval record
  does not change the approved behavior.
- Acceptance context: original TASK-001, approved spec, review-01 verdict,
  behavior report, validation ledger and verification, and TASK-001-R1 task
  and implementation evidence. Review follows this round's `review-scope.md`.

Inspected the direct prior-candidate-to-code-candidate diff, not a remediation
triple-dot range. The prior candidate is an ancestor. The complete delta is
four files: three rustdoc additions, two capture caller substitutions, and
deletion of the duplicate capture. HEAD's additional committed changes are
review/evidence/spec approval records only. Comparing the code candidate to
the current tree under `crates`, Cargo manifests/lockfile and `mise.toml`
produced no differences. CodeGraph is absent; no nested AGENTS governs these
changed server/CLI files.

Applied root AGENTS reuse and rustdoc rules, the task-review remediation
scope/materiality/validation rules, and the Rustdoc section of
`agent_docs/rust.md`. No changed runtime contract or sensitive boundary
requires reopening the prior architecture/security acceptance evidence.

## Prior-finding closure

### FIND-TASK-001-1 — CLOSED

The complete added `CapturedLogs` struct, installation/text methods, writer
and MakeWriter implementation are deleted from
`crates/wyrd/wyrd-server/src/test_support.rs`. Candidate source search finds
no remaining `CapturedLogs` occurrence in the server. Both affected callers
use the existing public `wyrd_testing::logs::LogCapture::install()`:

- `http/router.rs::tests::readiness_503_is_not_logged_as_a_failure` imports
  the existing type and installs it before either observed request. The test
  retains its plain `#[tokio::test]` current-thread execution, both 503 status
  assertions, the readiness absence-of-ERROR/presence-of-DEBUG assertions and
  the ordinary-route ERROR control.
- `components/health/mod.rs::tests::readiness_transitions_are_logged` installs
  the same owner before any transition. It retains unchanged-unready silence,
  recovery INFO, loss WARN, lost-readiness message and `pool_acquire` reason
  assertions.

Independently read both complete test bodies, router request helper, and the
complete shared `wyrd-testing/src/logs.rs` owner. The owner installs a
thread-default DEBUG-and-above, no-ANSI subscriber, stores its DefaultGuard
in the capture, and provides the same lossy UTF-8 text access. Each caller
keeps `logs` in scope through its final assertion, so the guard covers the
entire observed behavior and restores the prior subscriber when the test
scope ends. No consumer needs the removed sink's separate guard or cloning.
The existing server dev dependency at `Cargo.toml:137` already supplies the
public owner; no dependency, feature, wrapper or harness was introduced.
The shared owner itself and unrelated private capture remain unchanged.

This is the minimum root-cause correction: one existing owner serves both
consumers, and the duplicated workflow disappears. No downstream guard or
new machinery is needed. Supplied R1 evidence records both exact logging
units passing after substitution; root owns the independent rerun.

### FIND-TASK-001-2 — CLOSED

Independently compared each new section with its complete body:

- `wyrd-cli/src/init.rs::GatewaySummary::serialize` now has `# Errors`:
  "Returns the serializer's error when it cannot write the string." The
  unchanged body directly returns `serializer.collect_str(...)` and therefore
  propagates that error as documented.
- `init.rs::tests::gateway_summary_distinguishes_empty_and_failed_reads`
  now has `# Panics` covering wrong classification/status/detail and failed
  or incorrect serialization. Its original assertions and serialization
  `expect` are unchanged.
- `health/mod.rs::tests::readiness_transitions_are_logged` now has
  `# Panics` covering wrong transition levels, missing failure checks and
  unexpected unchanged-unready logs. Its assertions are unchanged.

Deleting the redundant writer removes its cited Errors/Panics omissions.
The router regression retains its existing Panics documentation. These are
accurate sections for the exact mandatory gate, with no exemptions, unrelated
documentation expansion or behavior changes. No prose-pinning test is needed.

## Affected-path acceptance matrix

Prior acceptance evidence is explicitly carried forward from review-01
`findings-validation.md` (Reconciled obligation coverage),
`task-review-behavior.md` and `verification.md`. No unaffected row is reopened:
the remediation changes neither its implementation/dependencies nor its
governing authority. Logging tests receive fresh source validation because
their capture owner changes.

| Obligation | Source and proof | Result |
| --- | --- | --- |
| REQ-008 / AC-006 current docs origin and machine-index paths | Carry forward guide Start source inspection and active-source origin search; guide and onboarding owners unchanged. | PASS |
| REQ-009 / AC-007 expected startup 503 without ERROR spam | Production classifier/handler unchanged. Re-read complete router regression and shared capture lifetime; all original 503/DEBUG/ERROR-control assertions retained. R1 records focused unit PASS; review-01 independent run also passed original assertions. | PASS |
| REQ-009 / INV-004 genuine faults, readiness loss and CLI errors | Production transition/probe/CLI error paths unchanged. Complete transition regression retains silence/INFO/WARN/reason checks under equivalent shared capture lifetime. R1 records PASS; carry forward prior producer/consumer tracing and dependency diagnostics evidence. | PASS |
| REQ-010 / AC-008 restart names stored tenant deployments | Carry forward authenticated deployment read tracing and managed-secret restart journey evidence, including retained inference/credentials and secret-hiding assertions. No path or journey changed. | PASS |
| REQ-010 successful empty and failed read remain distinct | Read/status/detail implementations and entire summary unit body unchanged; re-read unit assertions. Carry forward prior independent unit PASS; R1 also records summary unit PASS. | PASS |
| REQ-010 explicit file application and first-run selection | Carry forward same atomic apply/readback and provider-offer source tracing plus supplied Python guided/file journey results. No affected implementation or fixture changed. | PASS |
| REQ-011 labelled endpoints, storage/Bifrost, settings/credential paths and gateway | Carry forward checklist inspection and supplied CLI/Python journey assertions. Output production bodies unchanged. | PASS |
| REQ-011 single readiness JSON, existing fields/types and MCP/log/shutdown outcomes | Serializer body unchanged; its new Errors section accurately describes propagation. Carry forward shared text/JSON source and first-run JSON journey evidence. | PASS |
| INV-005 tenant authority, retained configuration and secret protection | Carry forward authenticated redacted read owner and persistence/leak assertions; no read/write authority, settings, credentials or provider call changes. | PASS |
| Task non-goals and adjacent behavior | Delta contains only specified deletion/reuse/rustdoc; no schema, durable state, health framework, dependency, provider validation, timeouts, skips, assertion weakening or TASK-002 implementation. | PASS |
| Mandatory existing-owner reuse | Duplicate deleted; both callers use unchanged existing LogCapture with correct subscriber lifetime. | PASS — FIND-TASK-001-1 CLOSED |
| Mandatory changed-item rustdoc | Accurate Errors and Panics sections cover the three surviving cited bodies; redundant writer omissions disappear with deletion. | PASS — FIND-TASK-001-2 CLOSED |

## Independently validated final ledger

**OPEN: empty.** Prior stable IDs `FIND-TASK-001-1` and
`FIND-TASK-001-2` are CLOSED as proved above; neither is renumbered or
retained as an OPEN finding. No new finding is proposed. No separate focused
validator is required because no validator-originated blocker arose.

## Evidence, verification limits and recommendation

Review-01 records independent nextest run
`6ceabc3a-b77d-49b0-9c3d-652331f87d71`, all three focused units passing,
and passing whitespace checks. Original TASK-001 supplies four CLI Postgres
journeys, two Python journeys, format and lints as implementation evidence;
their unchanged behavior and prior source validation carry forward. R1
records the two logging units, summary unit, format, lints and diff check as
passing. These supplied execution claims are distinguished from this fresh
validator's source inspection; I ran no Cargo/build/test command. Root owns
this round's independent three-unit rerun and records it separately.

Recommend **PASS**: all applicable acceptance rows have source-backed or
valid carried-forward evidence, both explicit mandatory gate failures are
closed, and the OPEN ledger is empty. No specification revision, further
remediation, expanded discovery or optional improvement is justified.

Skipped builds, tests, provider calls, benchmarks and broad suites in this
validator role. Journey results remain supplied/prior evidence rather than a
fresh independent rerun; no new runtime risk was established by this delta.
