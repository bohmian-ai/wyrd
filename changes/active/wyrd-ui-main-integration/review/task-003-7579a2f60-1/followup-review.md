# Focused claim reconciliation

Result: **RESOLVED**. The discovery PASS statements concern different invariants from the two proposed defects. Source establishes both narrower gaps without contradicting the passing authorization, typed-reference, terminal-validation or actual-result-budget paths. This report resolves the stated uncertainty; final finding adjudication belongs to the independent validator.

## Subject and scope

Repository `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`; cumulative base `c62ebce8c4de0d43a2b9d50d360de628c49fd533`; candidate and checked HEAD `7579a2f603c023daae0d15b9d5cc3013eec6f00d`. Read the original TASK-003, specification revision 13, root and TypeScript SDK AGENTS, task-review instructions, maintainer and spec-driven workflow guidance, applicable Card-reference authority, and the six discovery reports. Inspected the cumulative changed owners and their current callers for only the two assigned uncertainties. No CodeGraph index exists. No unrelated PR #205 behavior or TASK-027 implementation is proposed as a finding.

## Typed reference versus copied reference

`standards-review.md` passes strict flat CardRef fields and the exact get/list sequence. `domain-review-security.md` passes tenant-scoped reads, kind/name/version preservation in requests, and optional UID mismatch enforcement by the existing server. Those conclusions are supported for those boundaries. They do not establish canonical clipboard text. The domain report's phrase “copied/chip identity uses the flat form” is too broad if interpreted as canonical text compliance.

The relevant source chain is:

- `packages/ui/src/lib/components/CardHeader.svelte:29–31` produces a reference with `kind`, `name`, and the selected exact `version` from a successful Card read.
- `packages/ui/src/lib/format.ts:51–53` accepts only name/version and produces `name@version`.
- `CardHeader.svelte:103` passes that reduced text directly to the clipboard helper; the helper at lines 44–52 writes the supplied text without a separate kind.
- The only sibling formatter consumer is `components/CardRefChip.svelte:9–11`, which displays kind in a separate span but uses the reduced text for its title/reference span. The other header use is its metadata line at line 84. Neither supplies kind to the clipboard string.

`architecture/wyrd-design.md:2159–2163` explicitly defines canonical text as `Kind/name@version[#uid]`, and `crates/wyrd-spec/src/reference.rs:556–571` implements that exact Display convention. TASK-003 requires preserved copied identity; specification INV-001/current contracts and flat-identity revision 13 do not authorize dropping kind. Consequently Model/ranker@1.4.2 and Data/ranker@1.4.2 collapse to the same clipboard text even though their actual read requests remain distinct and authorized. Existing component assertions at `ui-library.test.ts:270–300` assert the shortened text and therefore do not disprove this path.

B-COPY and INV-003-FLAT-REF describe one shared formatting cause. The existing `fmtCardRef` owner can project the canonical flat reference using the generated CardRef fields and retain the optional UID suffix when supplied. No new parser, lookup, route, identity owner or compatibility rule is needed. The required correction is canonical copied text; it need not change version selection, read authorization or the separate Copy uid action. A compact formatter/component assertion distinguishing same-name/version different-kind references is sufficient focused proof. There is no architecture or public-contract decision to invent: the canonical spelling is already authoritative.

## Actual-result budgets versus admissible requested ceilings

The standards report passes the presence of row/byte budgets and shared positional encoding. The domain report passes authenticated query admission, validated terminal completion and bounded projection. Those statements hold when “bounded” means bounded by the supplied values. They do not prove that callers may request only the existing maximum values. A shared builder can enforce a supplied budget correctly while accepting an impermissibly large budget.

The relevant producer-to-consumer paths are:

| Path | Source evidence |
|---|---|
| Public Node request | `packages/ui/src/lib/contract.ts::WireRequest` exposes `max_rows` and `max_bytes`; `node.ts:29–37` starts the existing SDK query and forwards both options to collect without range admission. |
| SDK/native producer | `sdks/wyrd-sdk-ts/wyrd/src/index.ts:544–552` passes both numeric options to native collect. `native/src/lib.rs:1377–1392` takes the owned stream and constructs `JsonResultBuilder` directly from those values or defaults. Positive values 10001 and 16777217 are valid u32 and need no questionable native conversion behavior to reach it. |
| Shared budget owner | `crates/shared/wyrd-queue/src/json_result.rs:69–79` stores both supplied values unchanged. `accept_batch` compares rows to that stored row value; `charge_bytes` compares bytes to that stored byte value. Constants at lines 25–36 separately declare the largest admissible ceilings as 10000 rows and 16 MiB. |
| MCP sibling | Server `query/collect.rs::QueryArguments::validate`, lines 171–182, rejects zero or above-maximum options. `src/mcp/bifrost.rs:236–243` invokes it before `BoundedQuery` and maps refusal to existing MCP invalid params. Server `ResultCollector::collect` uses the same shared builder after this admission. |

Thus a tiny authorized `SELECT 1` with max_rows 10001 or max_bytes 16777217 can succeed via Node while MCP refuses the option. A sufficiently large result may also be retained beyond the required collected-result maximum. General server streaming admission cannot close this gap: the Node server request carries SQL/params/deadline, while these smaller JSON collection options remain local to the collector. No claim is made that ordinary Chart inputs can set these options; the exported executor and SDK collection path are explicitly required reachable consumers.

REQ-030 says to keep current row/JSON-byte ceilings on both paths; TASK-003 Scenario 2 requires honest limits on the equivalent six-read surface. B-LIMIT, INV-003-BOUNDS, MNT-003-11 and SYS-003-1 describe the same missing admission at this producer. Existing integration assertions at `ui-library.integration.test.ts:180` exercise smaller valid requested budgets, testing actual-result overflow rather than admissible option maxima. They do not cover the distinction.

The existing shared bounded-result owner and constants are the appropriate reuse boundary for the common range rule. Native collection must refuse inadmissible options without retaining a partial result and must release its owned stream; server MCP should retain its current advertised schema and invalid-params refusal. Do not add per-component or per-host guards, clamp options into success, or alter the general streaming iterator. Existing `WyrdError::Validation` (`wyrd-spec/src/error.rs:305–320`) supplies a catalogued request-validation error, and `NativeQueryStep::failure` (`native/src/lib.rs:122–140`) plus SDK `projectedError` already carries stable Rust error metadata to Node. MCP has its existing `InvalidQueryArguments` reason/details projection. Reusing these mechanisms does not require a new error code or changing MCP protocol behavior; identical JSON-RPC and SDK wire error envelopes are not an additional requirement. Valid defaults, accepted boundary values, overflow errors, cancellation, terminal settlement and unrelated query admission remain intact.

Focused proof can use the existing Node/MCP read test with a tiny query and zero/just-above-maximum options, plus accepted maxima/defaults, preserving its current valid-budget overflow cases. No synthetic load, new harness, broad journey aggregate or public policy decision is necessary.

## Disposition and limits

No additional proposed findings were discovered or requested. The apparent contradictions are resolved by separating transport identity from canonical text, and supplied-budget enforcement from option-range admission. Both proposed correction boundaries already exist and can implement the approved behavior without a spec revision. They remain proposals for independent validation rather than a task verdict from this follow-up.

Only this report was written. No source/test edits, tests/builds, live probes, commits or pushes were performed. Candidate-green verification remains user-supplied; this follow-up adds source evidence only.

Skipped: fresh runtime reproduction and unrelated discovery. Risk: existing green tests do not exercise the two identified input/text distinctions.
