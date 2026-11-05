# TASK-003 cumulative re-review — FIX_REQUIRED

Verdict: **FIX_REQUIRED**. Active findings: **FIND-003-16**, **FIND-003-17**. Route [TASK-003-R8-chart-and-executor-recovery.md](TASK-003-R8-chart-and-executor-recovery.md) directly to `$wyrd-implement`.

Repository `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`, branch `wyrd-ui-main-integration`.
Cumulative base: `c62ebce8c4de0d43a2b9d50d360de628c49fd533` (parent of `ff4145ac6`).
Immutable candidate: `f8c6f083103d8f55007510a4fbcc75e36b4d9c55`.
Authority: [approved specification revision 13](../../spec.md), [original TASK-003](../../tasks/TASK-003-shared-ui-foundation.md), applicable repository/architecture owners. [subject.md](subject.md) identifies history, common navigation, supplied verification and write restrictions.

## Cumulative scope and independence

This acceptance audit covers the complete task base-to-candidate range, original task, seven prior packets and R1–R7 remediations. The latest R7 commits `bca10dcfa` and `f8c6f0831` locate changes, not the whole review subject. Unrelated PR #205/main (flat Card identity; pr205-base `fb1fd9234`) and separately accepted TASK-027 layout at `39c151769` are excluded as findings. Shared foundation loading/action seams remain covered. The original task's revision-10 wording is reconciled under approved revision 13, including deferred Ask/A2UI scope.

Six fresh independent discovery reviewers read the immutable cumulative subject. A separate focused reviewer resolved newly exposed reachable paths, then a fresh structured Ponytail reviewer independently validated every proposed finding against source. The orchestrator reconciles only that validated ledger and does not introduce its own findings.

| Role | Report | Result |
|---|---|---|
| behavior-rev | [task-review-behavior.md](task-review-behavior.md) | FAIL; BEH-1 proposed |
| invariant-rev | [task-review-invariants.md](task-review-invariants.md) | PASS; no proposed finding |
| repo-rev | [standards-review.md](standards-review.md) | PASS; authority/rule coverage complete |
| maintainer-rev | [maintainer-review.md](maintainer-review.md) | PASS; no material maintainer finding |
| system-rev | [system-review.md](system-review.md) | FAIL; SYS-1 proposed |
| security/tenancy domain-rev | [domain-review-security.md](domain-review-security.md) | PASS; no material domain finding |
| fresh followup-rev | [followup-review.md](followup-review.md) | RESOLVED; both proposed failure paths traced and scope narrowed |
| fresh ponytail-rev | [findings-validation.md](findings-validation.md) | FIX_REQUIRED; FIND-003-16 CONFIRMED, FIND-003-17 REVISED |

[claims-comparison.md](claims-comparison.md) records why follow-up was needed: two newly exposed reachable failure paths were absent from other reviewers' PASS coverage. Follow-up established the Chart path in actual view hosts and limited the executor consequence to retained public instances. Validation independently reproduced both local failures from actual source and confirmed their required callers. No unresolved material conflict or missing required report remains.

## Reconciled acceptance matrix

Detailed source/proof matrices are in the two task reports. Their broad PASS claims are narrowed by the independently validated exceptions below.

| Original obligation | Reconciled implementation and verification evidence | Result |
|---|---|---|
| Approved package exports, browser/native separation, generated catalog, view validation and mountView | Package/generator/entries, schema-dispatch source, retained import/build proof and supplied check/catalog green | PASS |
| Relative ranges produce existing valid absolute bounds or canonical field error before reads on every host | R7 `bounds` reuses AbsoluteRangeSchema at admission/resolution; fixed-clock year-zero/overflow, root replacement, panel isolation and endpoint assertions | PASS; -13 closed |
| Authored SQL remains exact with only true range placeholders bound | R7 shared lexical recognition skips inert literal/identifier/comment text; loader cases retain exact SQL, params and genuine missing-range refusal | PASS; -15 closed |
| Explicit range precedence, single inherited resolution, positional duplicate columns and exact Open in Query | Existing shared loader/context, first-position column owner and executed metadata; retained focused assertions | PASS; -1/-2 closed |
| Complete Chart types/states, signed plots, twenty distinct series and collision-safe typed/aggregate captions | Chart/plot/shaping source and signed/20-series/caption tests remain correct; valid null-only results reveal a separate interaction failure | PASS for rendering/shaping; brush exception below |
| Shared Chart interactions emit safe valid ranges for supported data | Populated null-only results leave no drawable x domain; active brush release converts undefined bounds to Date and throws before intent validation | FAIL; FIND-003-16 |
| Exact flat Card identity, paging/version failure, copied references and host-controlled completion | Existing registry reads, shared formatter, Header/Chip/action owners and retained tests | PASS; -5/-12 closed |
| Six authorized SDK/MCP reads, positional null-preserving results, terminal-safe shared bounded collection | Existing facades/tools, shared JsonResultBuilder/admission, original protocol/settlement owners; supplied real-server integration | PASS within recorded limits |
| Reusable Node executor preserves later caller-initiated reads after recoverable initialization failure | Both lazy getters permanently retain a rejected connect promise; same-instance later calls cannot reach recovered SDK owners | FAIL; FIND-003-17 |
| Lossless original Number ceilings and release of refused query streams | Native original f64 parse and shared admission after ownership transfer; retained refusal/closed-stream/fresh-query proof | PASS; -11 closed |
| Failed prerequisite halts dependents; cancellation and supersession fence stale results | Loader, renderer, Node acquisition/drain and native close ownership; existing focused negative assertions | PASS; -6 closed |
| Canonical problems, standard public import and generated parity/rustdoc | Generated problem projection, endpoint envelopes, JSON import attribute; all changed Rust/test/helper docs and declarations inspected | PASS; -7/-8/-14 closed |
| Rust credential/session/query authority, tenant isolation and host packaging | Request-scoped selected session, existing Rust transport/query-only construction, existing build owner; supplied/historical evidence | PASS within recorded limits |
| Primitives, gallery, custom app and built AppBridge artifact; separate human checkpoint | Cumulative source and recorded human/artifact acceptance; supplied unit 40/40 and real-server integration 2/2 | PASS within recorded limits, except shared failures above |
| INV-001..007 and explicit non-goals | Existing durable/security owners retained; no Ask service, saved-view store, generic write dispatcher, per-component route, A2UI adapter, production MCP registration or publishing | PASS except bounded validated behavior gaps |

## Active validated findings ledger

| Stable ID | Status / classification | One-line summary |
|---|---|---|
| **FIND-003-16** | CONFIRMED / INCORRECT | Brushing a successful null-only timeseries throws before host validation because its drawable x domain is absent. |
| **FIND-003-17** | REVISED / INCORRECT | A transient first-connect failure permanently poisons a retained public Node executor by caching rejection. |

[findings-validation.md](findings-validation.md) is the canonical detailed ledger: discovery IDs, exact changed locations, violated obligations, producer/sibling/caller traces, executable source evidence, observable consequences, selected corrections and closure proof. FIND-003-16 belongs at Chart's geometry-to-range production; supported null query values remain intact. FIND-003-17 belongs at both initialization cache producers, retaining successful/in-flight sharing while permitting later explicit reads to connect again. The request-local shipped BFF/example are not permanently disabled across requests. Neither correction needs a new product, public API, architecture, security or persistent-state decision.

One R8 task packages these two independent bounded corrections without inventing shared machinery. It preserves adjacent lifecycle, authorization, null semantics, normal brushing and cancellation behavior; focused mounted UI/Node mock tests suffice without a new dependency or live outage harness.

## Prior-finding closure

**FIND-003-1..15 are CLOSED.** The independent validation table provides current-source evidence for each ID: range precedence, positional bindings, signed geometry, twenty-series styles, host action state, cancellation, canonical problems, native imports, caption/typed identity, original-number budget admission, exact Card formatting, relative-range absolute postcondition, test Panics rustdoc and SQL placeholder recognition.

R7 closes -13 and -15 at their original shared planning owners. The new Chart failure concerns geometry produced from valid query nulls, not relative-range admission. The new Node failure concerns rejected handle initialization, not cancelled active streams. Neither renumbers a prior issue or rediscovers one by sibling caller.

## Verification and limits

[verification.md](verification.md) distinguishes supplied candidate results from new review evidence. Supplied green: UI check, unit tests 40/40, catalog check, fmt:check and real-server integration file 2/2. R7 changes no Rust. Independent artifact-free probes reproduce the null-only Date conversion and rejected-promise retention through actual candidate source; caller/source traces establish required production reachability. Healthy-path green results miss these two negative transitions.

No builds/test suites/codegen were rerun under the packet-only write restriction. Historical broader Rust/SDK, packaging and human gallery results retain their recorded scope. Fresh browser execution, live outage injection, Linux/images and cross-platform/human qualification are not claimed. Source/test files remain immutable; no commit or push is performed.

## Packet completion

A separate bounded reader reviewed only the proposed R8 task and approved spec revision 13 for the required document comprehension pass. It correctly explained both problems, correction boundaries, preserved invariants, costs/limits, focused proof and maintainer decision; it found no material ambiguity or contradiction. This is document comprehension evidence, not implementation verification or another approval gate.

Final read-only Git checks confirmed HEAD remains `f8c6f083103d8f55007510a4fbcc75e36b4d9c55`, tracked and staged diffs are empty, and this requested packet is the only untracked path. All thirteen required packet artifacts are present, nonempty, and passed packet whitespace/local-link checks. Cumulative Git diff whitespace validation passed. Only the requested packet directory was written; no source/test changes, staging, commits or pushes occurred.
