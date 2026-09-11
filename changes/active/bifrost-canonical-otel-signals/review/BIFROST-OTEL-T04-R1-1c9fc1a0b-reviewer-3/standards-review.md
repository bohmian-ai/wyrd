# BIFROST-OTEL-T04-R1 repository standards review

## Immutable subject

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd`
- Original task base: `442da074cb316be7f580694ba8274229561935a8`
- Prior candidate: `81eaa346e643ac6315e517041fc838c41057f7ac`
- Remediation base: `e2ebc45a6359dad784614ab238540ea85543298c`
- Candidate: `1c9fc1a0bd5b54733e241902b671f1f00345d892`
- Reviewed remediation range: `e2ebc45a6..1c9fc1a0b`

Applicable authority read for this audit: `AGENTS.md`,
`architecture/agent-rules.md`, `architecture/wyrd-design.md`,
`architecture/wyrd-doctrine.mdx`, `architecture/bifrost-design.md`,
`architecture/wyrd-security-posture.md`, `TESTING.md`,
`architecture/references/README.md`,
`architecture/references/doctrine/architecture-constraints.md`,
`architecture/references/languages/rust-core.md`,
`architecture/references/languages/spec-driven-development.md`,
`architecture/references/languages/implementation-execution.md`,
`architecture/references/languages/testing-workflows.md`,
`architecture/references/domain/telemetry-observations.md`,
`architecture/references/domain/arrow-analytical-interop.md`,
`architecture/references/domain/olap-serving.md`, and
`architecture/references/domain/analytical-operations-reliability.md`.

## Authority coverage

| Remediation surface | Applicable authority | Result |
|---|---|---|
| `gate/mod.rs` retry identity and dispatch | `AGENTS.md` §§3–6, 9–12, 16; `architecture/agent-rules.md`; `architecture/bifrost-design.md`; Rust, telemetry, OLAP, Arrow, and reliability references | PASS |
| `scribe/preprocess.rs` Arrow digest helpers | The same Rust, Vala, Arrow, Bifrost, and reliability authorities | PASS |
| OTLP negative journey | `AGENTS.md` §§11 and 16; `TESTING.md`; testing, telemetry, and OLAP references | PASS |
| OTLP trace test helper | `AGENTS.md` §16; `architecture/agent-rules.md` import/signature rule; Rust and testing references | FAIL — STD-R1 |
| Remediation task and verification record | `AGENTS.md` §§11 and 14; spec-driven development; implementation execution; testing workflows | FAIL — STD-R2 |
| Prior verdict edits | Spec-driven development and current user authority | PASS — supplied review context |
| UUID representation | Current user authority and withdrawn prior finding | PASS — not reopened |

## Applicable-rule results

- PASS — authenticated tenant, principal, and accepted correlation attribution enter the existing Gate-owned retry identity.
- PASS — the existing Scribe WAL and commit fence remain the sole duplicate-decision owner.
- PASS — no public contract, generated schema, migration, dependency, Cargo feature, or additional persistence mechanism was introduced.
- PASS — the Arrow walkers are synchronous, bounded by the admitted batch, and documented with their error conditions.
- PASS — `dispatch_canonical` remains an inherent method on the dependency-owning `Gate` and is async only because it awaits Scribe IO.
- PASS — the new and materially modified remediation items carry meaningful rustdoc and applicable `# Errors` or `# Panics` sections.
- PASS — Postgres/server behavior is tested in the existing ignored OTLP journey target.
- PASS — `verify:bifrost` is truthfully recorded as 15/16 with an isolated passing Forge rerun, consistent with the unrelated-failure rule.
- FAIL — the new trace-export test helper uses fully qualified signature types.
- FAIL — the named two-principal journey lacks its exact durable focused command.

## Material findings

### STD-R1 — new helper violates mandatory signature style

- Rule: `architecture/agent-rules.md` requires module-scope imports and bare type names in function signatures.
- Location: `crates/wyrd/wyrd-testing/tests/bifrost/otlp/trace_export.rs:32-55`.
- Evidence: the new `export_traces_over_grpc_as` helper and the materially modified wrapper use fully qualified `ResourceSpans` and `ExportTracePartialSuccess` types.
- Consequence: the changed Rust does not satisfy mandatory repository style.
- Testable correction: import both types in the module import block and use their bare names in both signatures.

### STD-R2 — named journey command is not recorded exactly

- Rule: `AGENTS.md` §11 requires every specifically named Rust test in a task artifact or implementation report to include and run its exact focused `mise exec -- cargo nextest ... -E 'test(=...)'` command.
- Location: `changes/active/bifrost-canonical-otel-signals/review/BIFROST-OTEL-T04-81eaa346e-reviewer-2/BIFROST-OTEL-T04-R1-close-retry-identity-review-gaps.md:146-158`.
- Evidence: the two-principal test is named, but the command record abbreviates its selector as `-E '...'`.
- Consequence: the durable evidence cannot reproduce the exact test expression that was executed.
- Testable correction: record the complete command and passing result for `negative::pg_tests::identical_exports_from_two_principals_each_keep_their_own_attribution`.

## Verification notes

- Independent Gate identity unit test: PASS, 1/1.
- Independent two-principal Postgres journey: PASS, 1/1.
- Independent mixed-replay Postgres journey: PASS, 1/1.
- `mise exec -- cargo fmt --all --check`: PASS.
- Cumulative `git diff --check`: PASS.
- Reported `mise run fmt` and `mise run lints`: PASS.
- Reported `mise run verify:bifrost`: 15/16 lanes; the unrelated Forge failure passed in isolation.
- The reviewed commit remained immutable. The shared branch advanced after
  evidence collection; that does not alter the explicitly pinned candidate.

## Result

`FAIL`
