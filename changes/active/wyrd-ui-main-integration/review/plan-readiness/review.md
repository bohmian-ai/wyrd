Review ID: wyrd-ui-main-integration-main-8651b96-spec
Target: changes/active/wyrd-ui-main-integration/review/plan-readiness/spec-revision-5-reviewed.md (original: changes/active/wyrd-ui-main-integration/spec.md, revision 5; snapshot identity: subject.json)
Gate: Spec
Verdict: Revise
Architecture axis: FAIL
Executability axis: Not applicable
Critical: 0
Major: 6
Task coverage: Not applicable

## Evidence and scope

Reviewed revision 5 against committed main at `8651b96fd0c049ab5e63b0606c98326af1fc8848`, the current canvas inventory, repository agent rules, applicable doctrine, MCP implementations, verification types/result table, UI auth consumers, manifests, and the plan-readiness review rubric. Spec line citations below refer to the preserved revision-5 snapshot, not subsequent amendments.

This is a specification gate. No replacement implementation tasks exist; the archived invalid packet is not authority. The user's current instructions govern the desktop/mobile completion gate and replace the pedantic-maintainer review requirement.

## Findings

### PRR-001 — Major — Declared read parity includes an upload write and unsupported bound-query arguments

**Location:** reviewed spec revision 5, lines 424–445.

**Issue:** The closed union is described as eight existing reads. `verification.evidence` plans uploads and requires write authorization. MCP `bifrost.query` does not accept `params`, although every loader must use parameterized SQL.

**Consequence:** The same loader cannot execute as specified across Node and MCP. Implementers must either change a public MCP contract, invent SQL interpolation, or route a write through a read executor.

**Evidence:** `crates/wyrd/wyrd-server/src/mcp/verification.rs:97–110` places evidence in scoped write descriptors; `crates/wyrd/wyrd-server/src/query/collect.rs:125–139` declares the closed query arguments, and `:197–203` explicitly constructs an empty parameter list.

**Required artifact edit:** Remove upload planning from the read union; distinguish retained declarations from retrievable evidence content. Assign an explicit owner and prerequisite for bound-parameter MCP parity using the existing Bifrost query contract. Ordinary argument-name differences can remain executor mappings.

### PRR-002 — Major — The fixed loader contract cannot represent required dependent reads

**Location:** reviewed spec revision 5, lines 419–450, 574–577, 659–662.

**Issue:** `requests(input)` produces one complete request batch, followed by a terminal `shape`. A result-driven Verifier graph needs the result before discovering the exact Verifier to fetch. Data drift explicitly requires discovering Verifier/requirement identities before querying features.

**Consequence:** Implementers must alter a published cross-host loader/executor contract or move orchestration into pages/components, violating the declared reuse boundary.

**Evidence:** The spec's Data drift decision at lines 659–662 requires this sequence. `crates/wyrd-spec/src/verification/execution.rs:188–199` makes exact Verifier identity part of the retrieved result.

**Required artifact edit:** Permit dependent read stages within the shared loader contract, retaining transport/credentials in existing executors. Specify observable cancellation and supersession behavior when inputs/range change, including refusal to overwrite newer panels with stale responses. Leave the local orchestration mechanism to implementation.

### PRR-003 — Major — Interactive components lack a cross-host action boundary

**Location:** reviewed spec revision 5, lines 405–407, 447–463, 465–470, 579–582.

**Issue:** Components are render-only and executors are read-only, but the inventory includes chart saving, requesting verification, review submission, repository management, and Ask/save operations. Catalog `emits` is named without defining what the host does with emitted actions.

**Consequence:** Each host must invent mutation dispatch, unsupported-action behavior, and credential ownership. A fixture-rendered form could incorrectly count as complete while its required interactions cannot work in all four hosts.

**Evidence:** REQ-025, REQ-029, and REQ-031 expressly require server-owned writes; REQ-030 only defines read execution.

**Required artifact edit:** Define the observable component-to-host action contract and completion expectations per host. Components may emit intent; durable mutations and permission checks remain with established server APIs and credential-bearing hosts. State what a host displays when a prerequisite action is unavailable.

### PRR-004 — Major — Fixture-only prerequisites can currently pass the library gate, and mobile acceptance is absent

**Location:** reviewed spec revision 5, lines 551–554, 599–646.

**Issue:** Prerequisite components skip live proof, yet the build order treats completion of every inventory component as the gate before MCP App and pages. The spec has light/dark gallery acceptance but no desktop/mobile requirement or recorded per-component visual acceptance.

**Consequence:** Tasks can satisfy the written spec with fixture-only components or desktop-only layouts and still claim the full library is ready to ship, contrary to the user's accepted requirement.

**Evidence:** T5's exception at line 613 and the “full component library” step at lines 633–641; the user's subsequent explicit completion instruction supplies the governing acceptance requirement.

**Required artifact edit:** Separate fixture-ready progress from production-complete acceptance. Record individual component desktop/mobile visual acceptance, functional interactions, accessibility, and applicable live authorization proof. State that prerequisite components hold the full-library shipping gate until their contracts and live behavior exist; placeholders cannot satisfy it.

### PRR-005 — Major — Grouped verification still specifies obsolete session semantics after main integration

**Location:** reviewed spec revision 5, lines 291–325.

**Issue:** REQ-027 describes a new `group_by` trigger running once per session and aggregation of every result sharing `group_id`. Current main already implements `group_end`, owner/source-scoped snapshots, later executions after new activity, and potentially overlapping snapshots. The spec still treats this as unspecified future work.

**Consequence:** UI grouping can merge unrelated owners sharing a caller group key, show all historical rows as members of a particular recorded snapshot, or select an arbitrary group result as the session verdict.

**Evidence:** `architecture/wyrd-design.md:1274–1365` specifies owner/source identity, `group_end`, repeated snapshots and settlement semantics. `crates/wyrd-spec/src/verification/execution.rs:173–182` exposes group owner and pinned snapshot; `crates/vala/vala-bifrost-redux/src/tables/verification/results.rs:55–71` includes `group_id`, `trigger_identity`, `execution_status`, and `verdict`.

**Required artifact edit:** Replace stale `group_by`/once-only assumptions with current recorded group semantics. Define group row identity and which actual recorded result is displayed; members of an opened group result must follow its pinned snapshot. Reuse existing execution readback and do not introduce a new session lifecycle.

### PRR-006 — Major — Arbitrary chart/query inputs conflict with the constant-SQL loader contract

**Location:** reviewed spec revision 5, lines 443–445, 480–484, 558–561, 578.

**Issue:** Every loader SQL is required to be a constant, but view documents, Chart definitions, RecordTable and QueryEditor accept SQL supplied at runtime. The spec does not distinguish loader-owned templates from caller-authored read-only queries.

**Consequence:** Implementers must reject documented core inputs or change the public input/loader contract while building the foundation. The initial `Chart` proof cannot establish the contract as written.

**Evidence:** REQ-025 expressly permits chart `sql`/`sql_file`; the view-document example supplies runtime SQL; the existing query owner accepts SQL and applies its authoritative read-only floor (`crates/wyrd/wyrd-server/src/query/collect.rs:151–158`).

**Required artifact edit:** Distinguish constant parameterized built-in queries from explicitly authored chart/Query SQL. Keep validation and authorization in the existing server query owner. Define how runtime SQL receives the approved range/bucket binds and how incompatible result schemas become panel errors.

## Axis assessment

Architectural feasibility fails on the current specified contracts, but the overall direction remains viable. These corrections preserve the requested package, existing SDK/server owners, render-only components, credential isolation, and component-first sequence.

Executability is not applicable at a spec gate. No task approval is implied.

## Task coverage

Not applicable. No replacement tasks exist.

Before deriving tasks, update the stale pedantic-maintainer references at reviewed spec lines 765 and 783 to the user-requested plan-readiness-reviewer. This is a mechanical authority reconciliation, not an additional blocking finding.

Deletion's auth import closure is real: `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/problem.ts:1`, `auth/session.ts:3`, and `auth/browser-sessions.ts:5` import deleted `views.ts`; routing tests import removed mock owners. The explicit compile/test/no-deleted-import acceptance makes this ordinary task consumer closure rather than a separate unresolved architecture decision.

## Limitations

Static review only; no implementation, tests, builds, or runtime state changed. The user's dirty trace-summary removal was preserved. This file is the sole review artifact written by the reviewer, after the root agent preserved the reviewed subject.

The principal risk remains claiming a complete shared library before dependent platform contracts and actual cross-host behavior are proven.
