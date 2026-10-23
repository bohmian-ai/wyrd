# Security/privacy domain review — TASK-001

**Result: PASS.** No material security/privacy finding remains in the reviewed cumulative candidate.

## Subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `fb1fd9234acf9ff20b1feff89bf9f3da9cba4f14`
- Candidate: `d2ffc59873b5af688adf1d1e5b2a3a388160bb83`
- Independently inspected the security-relevant cumulative diff, using `4b723032f..d2ffc5987` only to locate remediation changes.
- Applied approved specification revision 1, original TASK-001, prior attempt-02 validated findings, and TASK-001-R1.
- Applied root and relevant SDK instructions, the committed development workflow, maintainer guidance, `architecture/wyrd-security-posture.md`, and the owning Bifrost result/access rules.
- Used CodeGraph before code navigation. No other attempt-03 discovery reports or implementation chat summary were consumed.
- HEAD remained the candidate; inspected production paths had no working-tree differences from it.

## Boundary and source coverage

| Boundary | Inspected evidence | Assessment |
|---|---|---|
| Author declarations → generated format | `skald-spec/src/authoring.rs`, `skald-spec/src/prompt.rs`, `wyrd-spec/src/card/prompt/spec.rs`, `card/verifier/mod.rs`, `card/verifier/validate.rs`, `verification/execution.rs` | Code-check format validation and Prompt compilation share the canonical Verdict resolver. Strict format decoding retains unknown-field refusal. |
| Embedded schema resources and references | `Verdict::response_format`, `check_output_schema`, schema generation, Cargo manifests/lockfile, resource regression tests | The data schema retains its definitions, reference root, authored `$id`, and authored `$schema`; an otherwise anonymous schema receives its own `$id`. No network resolver or reference-retrieval feature was introduced. |
| Untrusted check response → accepted judgment | `vala-verify/src/blocks.rs::verdict_from`, `blocks/agent.rs::execute`, hosted `sandbox.rs::execute`, `blocks/engine.rs` | Both production check kinds use the same whole-response validator and fixed data bounds before accepted data reaches the requirement. |
| Rejected response → diagnostics | `verdict_from`, `Verdict::check_data_bounds`, `Judged::inconclusive/report` | Response values and instance paths are excluded from validation refusal reasons. Bound failures disclose counts and allowed bounds only. |
| Stored result → Rust/Python/TS typed result | Rust `RequirementResult::decode_data`; Python `verify.rs::decode_data`; TS `index.ts::typedResult`; typed verify/wait/status siblings | Converters discard decoder messages and locations, expose the requirement and safe empty path, and fail the complete conversion. Ordinary execution, transport, and polling owners remain shared. |
| Accepted data → persistence/readback | `verification/results.rs::ResultPayloadBuilder::summary`, `ResultsTable`, verification service result reads, authenticated scheduled queries | Data remains inside the existing outcome Variant. No metadata column, audit field, new storage path, or permission bypass was added. |
| Tenant and permission enforcement | `VerificationControl::get_execution/read_result`, `ScheduledQueryCaller::authenticated`, HTTP tenant test and MCP scoped/foreign-reader tests | Execution lookup uses caller-bound tenant SQL. Retained result reads use the verified caller’s ordinary Bifrost query path. |
| Logs, audit and telemetry | Changed execution owners, engine instrumentation/error propagation, result projection, outbox diff | Changed spans skip payload arguments; the candidate adds no custom-data logging or audit projection. |
| Dependency/authentication migration | `Cargo.toml`, `Cargo.lock`, changed secret/OIDC schema implementations, Python lock/auth diff | Schema migration retains inspected secret-field constraints and redaction metadata. The HTTP auth alias continues using the existing token/header replacement flow. No new dependency source or credential default was introduced. |

## Prior-finding closure

**FIND-TASK-001-4 / prior SEC-001 — CLOSED in source.**

The previous leak arose because validation locations could contain dictionary keys returned by the check.

- `crates/vala/vala-verify/src/blocks.rs:198–213` now formats only `error.schema_path()`. It no longer copies `instance_path()`, the validation error’s display text, or the rejected value.
- `crates/wyrd-spec/src/verification/execution.rs:516–529` discards Serde’s error and returns only the requirement identity and `path: ""`.
- `sdks/wyrd-sdk-python/src/verify.rs::decode_data` discards Python validation exceptions and does not inspect their messages or locations.
- `sdks/wyrd-sdk-ts/wyrd/src/index.ts:275–295` catches arbitrary parser throws without accessing them, returning the same safe-root error.
- The shared server negative path produces an inconclusive requirement with no data. The existing result writer serializes that sanitized outcome; downstream HTTP/MCP/Bifrost readers do not recreate a payload-derived diagnostic.

Inspected regression coverage includes `verdict_refusal_omits_returned_keys`, Rust nested-map decoding assertions, TS null/undefined/Error/malformed/keyed parser rejection cases, and the three SDK mismatch journeys. These are source-inspected tests, not tests independently rerun during this review.

**FIND-TASK-001-1 — Security-relevant resource/dialect aspect CLOSED in source.**

`Verdict::response_format` at `verification/execution.rs:569–626` no longer removes the authored dialect or hoists definitions into the envelope. It embeds the data schema as its own resource and preserves explicit identities. The added shared-validator tests cover an envelope-property-colliding local reference, `prefixItems`, draft-07 reference siblings, and definition-name collisions.

The workspace still declares `jsonschema` with `default-features = false`; inspected consumers inherit that declaration without enabling retrieval features. Thus custom schema references do not introduce an HTTP/file-fetch capability. Invalid validator construction produces a fixed terminal error rather than exposing resolver diagnostics.

**FIND-TASK-001-2 — Privacy/error-boundary aspect CLOSED in source.**

The TS catch no longer assumes any shape for the thrown value. This also removes the previous secondary TypeError paths and prevents copying parser-owned diagnostics.

**FIND-TASK-001-3 — Declaration-boundary aspect CLOSED in source.**

The shallow handwritten-data check was removed. `validate_agent` now uses the existing bounded `fits` owner against canonical plain and required-object Verdict shapes, retaining reference/composition handling and traversal limits.

FIND-TASK-001-5’s complete cross-language proof acceptance and FIND-TASK-001-6’s rustdoc acceptance belong to their owning review scopes; this report makes no independent whole-task closure claim for them.

## Security Audit

### Critical

None identified.

### High

None identified.

### Medium

None identified within the approved task and reachable cumulative changes.

### Low / Defense In Depth

No additional hardening is required by this scoped review.

### Positive Controls

- One server validation owner serves Agent and hosted-code checks.
- Rejected data is excluded from requirement results and diagnostics.
- SDK decoder exceptions cannot supply public error content or dynamic field locations.
- Data limits remain fixed server controls, with existing provider/sandbox bounds retained.
- Accepted data inherits existing outcome storage, tenant isolation, authorization, and retention.
- Verification data does not enter new audit fields, metadata projections, or logging statements.
- Schema resources use standard reference scoping without enabling external retrieval.

## Verification limits

This was a static, review-only audit. I inspected recorded verification evidence and relevant assertions but ran no Cargo, server, SDK journey, live-provider, exploit, or dependency-advisory checks. Existing source tests support the assessed paths; their execution results remain the recorded implementation evidence.

A live provider’s acceptance of embedded `$id` was not independently exercised. Provider refusal is an interoperability limitation, not evidence of a security bypass.

**Proposed finding ledger: empty. Overall domain result: PASS.**

Skipped independent runtime and advisory checks; no claim of an exhaustive repository security audit is made. Source files were not edited; the orchestrator must persist this report at the assigned path.
