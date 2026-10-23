# Independent findings validation — TASK-001

Result: **FIX_REQUIRED**. Six bounded findings remain. No new material product,
public API, security, ownership, or persistence decision is required to correct
them under approved revision 1.

## Immutable subject and inputs

- Repository: `/home/thorrester/Documents/GitHub/wyrd`.
- Base: `4d0f7005888a899910008399d1b5e15e2a3207f9`.
- Candidate: `9e56fc340cb5fd769d3b34f47433e2719339fdfb`.
- Subject: complete original-base-to-candidate three-dot range, 66 paths;
  approved `changes/active/verifier-response-data/spec.md` revision 1 and
  original `tasks/TASK-001-verdict-formats-and-custom-data.md`.
- Read both task implementation reports, standards, maintainer, system,
  security and persistent-data reports in this directory, plus `subject.md`,
  `claim-comparison.md` and `verification.md`. No intended verdict was supplied.
- Read root and applicable Python/TypeScript/docs rules, current workflow,
  maintainer/Rust/errors/agent-surface guidance, testing map, architecture
  authority map and owning Prompt, Verifier, client, Variant, permission and
  tenant-isolation sections. Current workflow edits were preserved; they are
  not candidate implementation changes.

CodeGraph preceded code navigation. Independently inspected the cumulative
diff, full correction-owner bodies and relevant callers/tests, rather than
accepting reviewer agreement as proof. Candidate source matched the working
tree at the inspected production paths; only the three recorded workflow
edits and review artifacts were dirty. HEAD remained the candidate through
the final check. Attempt 01 has no accepted FIND ledger or remediation: it was
blocked on a candidate change. The stable IDs below therefore start at 1.

## Claim dispositions

| Discovery ID | Disposition | Independently established result |
| --- | --- | --- |
| BEH-001 | CONFIRMED | Embedding custom data changes local-reference meaning. Consolidated with INV-001 as FIND-TASK-001-1. |
| INV-001 | CONFIRMED | The same resolver also removes the effective schema dialect, admitting values refused by a supported later draft. Same correction owner, FIND-TASK-001-1. |
| BEH-002 | CONFIRMED | Null/undefined throws and malformed optional issue locations bypass the catalogued TypeScript error. FIND-TASK-001-2. |
| INV-002 | CONFIRMED | Same shared TypeScript catch defect, FIND-TASK-001-2. |
| MNT-001 | CONFIRMED | Same public error-contract consequence, rather than a separate maintainability/style defect. FIND-TASK-001-2. |
| BEH-003 | CONFIRMED | Shallow handwritten-data validation both admits optional data under composition and rejects required object data behind a local reference. FIND-TASK-001-3. |
| INV-003 | CONFIRMED | Same incompatible schema interpretation, FIND-TASK-001-3. The correction reuses the existing compatibility owner, without a second walker. |
| SEC-001 | REVISED | Payload-derived dictionary keys reach server reasons and all three SDK errors. FIND-TASK-001-4 retains the privacy defect and selects conservative existing-boundary locations; generic SDKs cannot safely infer static fields from arbitrary decoder paths. |
| INV-004 | CONFIRMED | Required real code-check malformed-output/exact-boundary proof is absent. FIND-TASK-001-5 is a verification gap, not an independently demonstrated production size/depth bug. |
| REPO-001 | CONFIRMED | The new local Rust test struct and field lack expressly mandatory rustdoc. FIND-TASK-001-6; batch with the functional remediation. |

No proposed source ID is silently omitted. System and persistent-data reports
have explicitly empty proposed ledgers. Their scoped PASS results do not
establish exact-schema or diagnostic correctness and do not contradict these
claims. No additional follow-up discovery is necessary: the claims are
compatible, reachable paths are identified, and source plus the bounded
experiments below resolve them without a new authority decision.

## Validated ledger and minimal recommendations

### FIND-TASK-001-1 — Custom schema composition changes reference and dialect semantics

**Status:** CONFIRMED. **Classification:** INCORRECT.
**Discovery:** BEH-001, INV-001.
**Obligations:** REQ-001/002/007, AC-003/004: supported schema meaning must not
be silently weakened; execution validates the exact declaration.

**Location:** `crates/wyrd-spec/src/verification/execution.rs:598–620`, the
complete `Verdict::response_format` method at lines 564–627.

The producer is a standalone custom object schema from YAML, Python schema/model
authoring, or TypeScript `verdictFormat`. The resolver meta-validates it through
`check_output_schema`, removes `$schema`, hoists only `$defs`/`definitions`, and
embeds the remainder under envelope `properties.data`. Local pointers therefore
resolve against the envelope rather than their original data resource. The
generated envelope is draft 7; the existing schema checker supports drafts
4/6/7/2019-09/2020-12 and treats an undeclared dialect as 2020-12. Removing the
custom dialect can change constraints even without references.

The whole shared method and its siblings/callers were traced:
`card/prompt/spec.rs::compile_prompt` passes it to the existing authored compiler;
PromptSpec construction/deserialization and inline Agent prompt decoding reuse
that owner. `CodeCheck::response_schema` and `VerifierValidator::validate_check`
call the same resolver. `AgentExecutor::execute` uses the compiled response
schema; hosted `CodeExecutor::execute` obtains the code schema. Both pass the
full returned value to the only production `verdict_from` calls. `Judged`
projects an accepted value into whole and step requirements, and direct/queued
result writers publish the full outcome. No read-side guard can repair an
incorrect judgment already produced here.

A reachable authored schema declares integer `data.reason` and
`data.copy: {"$ref":"#/properties/reason"}`, with both required. In its own
resource both must be integers. In the compiled envelope that reference points
to Wyrd's string `reason`. Public Python Prompt rendering plus the installed
JSON Schema validator independently produced:

```text
authored {reason: 1, copy: 2}: valid
compiled envelope with that data: invalid
compiled envelope with {reason: 1, copy: "wrong"}: valid
```

A supported 2020-12 data schema constraining the first `scores` item with
`prefixItems: [{type: number}]` also rendered successfully, but under draft 7
with its own `$schema` absent. The authored validator refuses `scores:["wrong"]`;
the rendered schema accepts it. These experiments exercise the actual public
schema producer and standard validation semantics; they are not represented as
a fresh production Rust/server execution. The source's compiler/schema ownership
and `jsonschema::validator_for` use establish the production reachability.

**Consequence:** valid outputs become inconclusive; invalid outputs can become
real judgments with retained custom data. Existing green simple/model-definition
tests do not cover this root cause.

**Selected correction:** keep `Verdict::response_format` as the single resolver
and preserve the custom schema as its own standard JSON Schema resource within
the composed schema, including its local-reference root, definitions and
effective dialect. Use conventional resource scoping and the installed JSON
Schema implementation; do not flatten definitions into the Verdict resource or
erase dialect semantics. The installed Rust library already has draft-aware
subresource compilation (`compiler::Context::in_subresource` sets the resource's
draft), so this is not a request for a new schema engine. A standard internal
resource identity for an otherwise anonymous schema is implementation detail,
not a new authored/public option. Preserve authored Cards/hashes. Existing
provider/compiler refusal is the boundary for a genuinely unsupported provider
combination; never emit a weaker schema as fallback. Do not add per-executor,
SDK, persistence or readback validation copies.

This selects resource-preserving composition rather than arbitrary keyword
rewriting, a new dialect translator, or silently rejecting all local refs/later
drafts. Standard compound-resource validation was independently checked with
the same two schemas: resource-scoped data yields `[valid, invalid]` for the
matching/mismatching pairs. Production closure still needs Rust proof.

**Focused closure proof:** extend the existing resolver/declaration tests with
the envelope-property-colliding local pointer, definitions that reference their
data resource, and explicit/default later-draft constraints. Validate good and
bad responses using the existing shared execution validator. Exercise one code
and one Agent path through the existing focused fixtures/tests; prove accepted
data is retained and invalid data is inconclusive/absent. Keep named/plain,
handwritten, supported model definitions and native round-trip regressions.
No new harness/dependency/public API is justified.

### FIND-TASK-001-2 — TypeScript parser rejection can escape normal SDK error mapping

**Status:** CONFIRMED. **Classification:** INCORRECT.
**Discovery:** BEH-002, INV-002, MNT-001.
**Obligations:** REQ-012, AC-006: any decoder mismatch uses normal WyrdError and
`WYRD_SDK_422_VERIFICATION_DATA_INVALID`, without a partial return/rerun.

**Location:** `sdks/wyrd-sdk-ts/wyrd/src/index.ts:281–292`, in the full
`typedResult` function at lines 272–298.

The public parser contract is any `parse(value: unknown): T`; it does not
require an Error object or Zod exception. All three typed public operations
(`Observe.verifyTyped`, `Execution.waitTyped`, `Execution.statusTyped`) delegate
ordinary work, then call this one local converter. For present data, `throw
null`/`undefined` reaches a cast whose runtime access still reads
`error.issues`. Optional chaining protects `issues`, not `error`. A non-array
`issues[0].path` similarly fails during `.map`. Independent Node execution of
the actual expressions produced TypeError for all three cases, before the
required `new WyrdError` can run. Current tests only supply normal Zod issues.

**Consequence:** callers receive an uncatalogued TypeError from every typed
sibling; error-code handling promised by the public API fails. Durable values
remain unchanged, which does not excuse the local error contract.

**Selected correction:** keep the catch at this generic parser trust boundary
and always project parser rejection to the existing WyrdError. Use the safe
fixed data-root path selected in FIND-TASK-001-4 when an independently safe
location is unavailable. Removing reliance on parser-owned optional diagnostics
also removes these null/malformed-object failure paths; do not add separate
per-method guards, parser registration, retries or a schema-library dependency.
Normalize neither thrown payload nor thrown message into public text/details.
This consumer guard belongs here because the SDK accepts arbitrary caller
decoder behavior and owns the public error conversion.

**Focused closure proof:** reuse one existing typed SDK test/setup with parser
rejections of null, undefined, ordinary Error and malformed optional diagnostics.
Assert normal WyrdError, exact stable code, requirement identity and safe root
path; ordinary reread and invocation counts stay unchanged. One common-boundary
case table suffices; no three duplicate harnesses or broad journey suite.

### FIND-TASK-001-3 — Handwritten data checks disagree with existing schema compatibility

**Status:** CONFIRMED. **Classification:** INCORRECT.
**Discovery:** BEH-003, INV-003.
**Obligations:** REQ-001/002 and AC-003: compatible handwritten schemas remain
supported; an extended handwritten contract explicitly requires object data;
incompatible declarations persist no Card.

**Location:** `crates/wyrd-spec/src/card/verifier/validate.rs:583–585` and
`:789–797`, `declares_data_object` and its sole production caller.

Read the entire `validate_agent`, helper and existing
`schema_path::{fits,Walk::resolve,Walk::fits,Walk::fits_object}` bodies. The
bounded owner follows local `$ref`, anyOf/oneOf branches and allOf proofs.
The supplementary helper independently reads only literal root
`properties.data`, root `required`, and literal `data.type`. Consequently:

- An otherwise compatible closed handwritten Verdict, with optional object
  data, can be placed under root allOf with root `type:object`; preserve any
  root definitions at the real root. Existing `fits` proves the branch against
  canonical Verdict (whose data is optional), but the new helper sees no
  top-level data property and returns true. It can persist and accept absent
  data, contrary to the declared-extension rule. The same root-local-ref wrapper
  bypass is reachable with correctly placed definitions.
- A direct closed handwritten Verdict with required `data` referencing a
  root-local object definition is compatible through `fits`, but
  `data.get("type")` is absent, so the new helper incorrectly refuses it.

This is a producer-side declaration defect, not merely a missing runtime guard.
The shared validator feeds actual registration; HTTP declaration journeys already
demonstrate validation-before-save, but their cases use direct literal schemas.
`fits` has no other independent production consumer; the same module's
`admits` serves output-reference checks and must remain unchanged.

**Selected correction:** delete the shallow parallel declaration interpretation.
Use the existing bounded `schema_path::fits` compatibility mechanism against
the two approved closed target shapes: plain Verdict without data, or Verdict
with required non-null object data. A handwritten schema must fit one complete
shape. Derive these shapes from the existing canonical Verdict owner rather
than copy a second Verdict definition. This directly covers existing supported
refs/composition, rejects optional custom data and reuses the existing budget
and failure behavior. Do not add an unbounded walker or broaden the schema
analyzer into a new general-purpose validator; no change to code actions,
reference admission or runtime/citation ownership is needed.

**Focused closure proof:** extend existing contract cases with allOf and local-ref
wrapped optional-data and wrong-type declarations (refused), corresponding
required-object declarations including referenced data (accepted), and plain
handwritten regression cases. Add the refused composed declaration to the
existing focused HTTP declaration test and prove its identity was not saved.

### FIND-TASK-001-4 — Payload-derived keys enter server and SDK diagnostics

**Status:** REVISED. **Classification:** INCORRECT.
**Discovery:** SEC-001.
**Obligations:** REQ-007/011/012 and AC-004/005/006: no custom payload in
validation diagnostics; only safe local field locations and requirement identity.

**Locations:** shared server `crates/vala/vala-verify/src/blocks.rs:201–205`;
Rust local `crates/wyrd-spec/src/verification/execution.rs:515–522`;
Python `sdks/wyrd-sdk-python/src/verify.rs:340–365`;
TypeScript `sdks/wyrd-sdk-ts/wyrd/src/index.ts:283–292`.

A supported data schema `{type:object,additionalProperties:{type:integer}}`
accepts arbitrary dictionary keys. A check returning
`{"customer-secret@example.test":"wrong-type"}` fails at an instance path
containing that returned key. `verdict_from` inserts the raw instance path in
its terminal reason. Both full executor bodies call that helper;
`Engine::check` turns its terminal error into `Judged::inconclusive`,
`Judged::report` keeps the reason, and `ResultPayloadBuilder::summary`
serializes it in the outcome. Direct return and queued/published readers expose
the reason through existing HTTP/MCP/SDK/Bifrost paths despite `data: None`.
Independent public schema rendering/validation located the failure at
`$.data['customer-secret@example.test']`. The full transfer and writer bodies
were inspected: no later scrubber removes that reason.

Typed decoders independently repeat the same assumption: Rust copies
`serde_path_to_error` path text; Python copies arbitrary Pydantic-style location
parts; TS copies parser-provided issue path parts. An accepted dictionary with
string values decoded to integer values emits dynamic map keys in those paths.
The generic model/parse/Serde contracts do not provide a portable way to prove
that a string location is a static model field rather than an input-derived key.
Current tests place secret markers in values and expect a known `category`
location; they do not establish safety for arbitrary dictionary keys.

**Consequence:** rejected custom-data content survives in retained requirement
reasons; locally rejected accepted data enters SDK error message/details. This
does not establish a new tenant bypass, but directly violates the explicit
diagnostic privacy contract.

**Selected correction:** at the existing shared server rejection owner, remove
instance-derived location text and use only the schema-owned location or a
fixed root location. No guards belong in HTTP, MCP, storage or the individual
executors. At each existing generic SDK conversion boundary, use the fixed
safe data-root location (the existing empty relative `path`, consistently
documented as the root) whenever static provenance cannot be established.
For these arbitrary decoder interfaces, choose that conservative root fallback
instead of attempting to interpret/copy model-supplied strings. Retain the
requirement ID, exact error code, whole-conversion failure and unchanged raw
readback. This requires no schema-bearing decoder API, model-library dependency,
registry, generic path parser or recursive redaction machinery. Remove now-unused
unsafe diagnostic extraction rather than retain speculative introspection.

Root is a safe field location within the approved existing string path contract;
the spec does not require a particular number of path segments or the current
literal `category` value. Update those expectations for this diagnosed privacy
correction; do not weaken the privacy assertion or alter the stored judgment.
No new public/security decision is necessary.

**Focused closure proof:** put a unique secret marker in a dynamic map key,
including a nested dictionary case, with a wrong-type value. At the shared
server seam and a real code/Agent execution assert the reason/serialized result
omit the marker and rejected data. In each language's existing focused typed
mismatch proof, decode accepted dictionaries to incompatible value types and
assert normal error code/requirement, safe root path and marker-free message
and details. Existing ordinary reread/non-rerun assertions remain. Combine TS
normalization proof with FIND-TASK-001-2 at the same converter.

### FIND-TASK-001-5 — Required real code-output acceptance proof is incomplete

**Status:** CONFIRMED. **Classification:** MISSING (verification).
**Discovery:** INV-004.
**Obligations:** original task scenario 2 and its SDK proof contract; AC-004/005
explicitly require negative and inclusive-boundary execution proof for both
check kinds. Root journey rules do not let a pure test replace it.

**Locations:** `fixtures/cards/verifier_building_blocks/scored-review.yaml:96–111`;
Rust `verifier_building_blocks.rs:1425–1462`; corresponding Python
`test_verifier_building_blocks.py:628` and TS
`verifier-building-blocks.test.ts:684` invalid-output cases.

Read the complete checked-in fixture and all three new invalid-output story
tests, the custom-data Agent integration test, and pure bounds test. Code's
only invalid-output producers are BAD-DATA (wrong score type) and HUGE-DATA
(70,000 characters). Its new stories select only those cases. Nothing executes
missing/null/undeclared data, extra envelope fields, exactly 65,536 versus
65,537 compact bytes, or 16 versus 17 containers through the real hosted code
check. Pure bounds tests cover UTF-8, required escapes and wide arrays; Agent
integration exercises many invalid and boundary returns. Neither proves that
the hosted code return path and normal result readback preserve the promised
boundary cases. This is a concrete absent acceptance proof, not speculation
that the sandbox truncates returned Verdicts: its print-output budget is not
the returned-value authority.

**Selected correction:** extend the existing checked-in code fixture and
focused SDK case tables to exercise the missing malformed output and exact/
one-over size/depth cases, including the specified UTF-8/escape/wide-array
variants. Reuse the existing local provider, deployment and result assertions.
Complete the corresponding Agent cases where the task's existing table still
lacks them; do not create a new harness or production option. Exact-boundary
cases must assert accepted data survives ordinary result/readback; malformed
and over-limit cases must be inconclusive with absent data and scrubbed reasons,
with valid siblings preserved. One shared fixture change feeds all SDK stories.
No production change is justified by this finding unless this focused proof
exposes a real failure, in which case diagnose at its existing owner.

**Focused closure proof:** run only the existing changed invalid-output and
boundary/readback test selectors in the affected SDK stories, plus the owning
pure/Agent case selectors if extended. This is not a request for full SDK
journey suites, broad aggregates, load, crash injection or a replacement worker.

### FIND-TASK-001-6 — New local Rust test model lacks mandatory rustdoc

**Status:** CONFIRMED. **Classification:** VIOLATION.
**Discovery:** REPO-001.
**Obligation:** root AGENTS requires rustdoc for every changed/new Rust item
and explicitly blocks merge without it; `agent_docs/rust.md:235–239` includes
private/test helper types and fields regardless of visibility.

**Location:** `crates/wyrd-spec/src/verification/execution.rs:840–841`, the
function-local `Judge` type and `score` field in
`typed_data_decodes_whole_results`.

The candidate diff introduces both without rustdoc; the containing test's
documentation does not document either nested item. The nearby SDK `Scored`
fixture documents the corresponding type/fields. No runtime defect is claimed;
this is the explicit rustdoc exception to the normal nonblocking style threshold.

**Selected correction/proof:** document this expected numeric custom-data test
shape and its score field concisely, preserving the test and its assertions.
Static inspection and the normal format check suffice. Batch this with the
same bounded functional remediation; no new behavior test or style-only round.

## Acceptance reconciliation

PASS below is source/evidence-supported, not a claim that I reran journeys.

| Obligation | Implementation and available proof | Result |
| --- | --- | --- |
| REQ-001 / AC-001 plain scalar/expanded, generated closed schema, standalone/inline/referenced, portable/native; authored hashing and native unrelated options | Existing authored compiler/resolver/Prompt state, named-format contract tests and plain SDK fixture cases | PASS |
| REQ-001/002 exact custom schema and declaration refusal before save | Shared resolver and HTTP refusal test exist, but resource/dialect and composed handwritten holes remain | FAIL: FIND-1, FIND-3 |
| REQ-002 required non-null object, no undeclared data/unknown envelope fields; code default plain and actions arbitrary | Closed canonical schema, strict CodeCheck distinction, both executor callers; ordinary cases covered | PASS direct forms, FAIL handwritten cases: FIND-3 |
| REQ-002 supported Python model/Rust schema/TS schema-model authoring without duplicate schema | Python coercion reuse/public authoring test, existing TS jsonSchemaText/verdictFormat and compile-time fixture | PASS implementation; TS runtime authoring proof remains limited |
| REQ-007 / AC-004 full exact validation, inconclusive failures, data omission, preserved citation policy | Shared verdict_from, both executors and engine terminal path, Agent negative cases/SDK BAD/HUGE | FAIL exact semantics/privacy/proof: FIND-1, FIND-4, FIND-5 |
| REQ-007 / AC-005 inclusive compact UTF-8/escaping/wide-array size and container depth; existing bounds retained | Correct shared bound calculation, pure boundary table and Agent seam cases; no production truncation demonstrated | FAIL required real code proof/privacy: FIND-4, FIND-5 |
| Preserve aggregation/status/dispatch, actual/drift detail and citations | Data transfer separate from actual/detail; existing citation filtering and aggregate owner unchanged | PASS |
| REQ-011 / AC-002 direct, queued wait/status, whole/step, HTTP/MCP and published Bifrost data | Existing Judged -> outcome Variant -> Scribe -> tenant-scoped read; Rust/Python/TS/HTTP/MCP readback evidence | PASS for accepted data, subject to exact-schema finding |
| REQ-011 / AC-007 tenant and existing permission isolation, retention/durability, no new column/store/log/audit projection | Existing authenticated query caller/TenantConn, result serializers and table/query permission; HTTP 404/scoped refusal proof | PASS authorization/storage; FAIL diagnostic projection: FIND-4 |
| REQ-012 / AC-006 actual generic types, all typed operations including Rust deadline, absent/failed/mixed data, no partial return/rerun | Rust consuming conversion, Python model_validate, TS parse; ordinary operation reuse; SDK typed/readback/typecheck assertions | PASS normal paths; FAIL generic rejection normalization/privacy: FIND-2, FIND-4 |
| Python positional-only type/verifier and evidence named data_type, group/input compatibility | Existing PyObserve signature and journey/typecheck forwarding | PASS |
| Standard fields/identities/provenance/verdict and durable results preserved by typed reads | Full Rust envelope transfer, Python retained Rust result, TS copied envelope and no writers; mismatch raw rereads/call counts | PASS |
| AC-008 public exports/stubs/generated contracts/runtime errors/authority/examples consistent | Shared wire/error owners, authored generic stubs, generated TS contracts; recorded codegen/docs/type checks; both policy-judge examples updated | PASS runtime surfaces; missing three-SDK prose is a nonblocking bounded docs correction below |
| Repository rustdoc gate | New local model/field undoc'd | FAIL: FIND-6 |
| No new dependencies/features, migrations, stores, lifecycle, transport, decoder registry or options; no inferred action judgment | Cumulative manifests/owners and mechanical consumers inspected | PASS |
| User-directed ty/auth changes | Dev-only lock upgrade, annotation-only generator updates, shared existing HttpAuth sync/async paths; new `.get` static assertion and frozen verification evidence | PASS source/regression evidence; old `.get` limitation is closed |
| Historical RED for scenarios 1–3 | Task openly records tests added after production | Missing historical process evidence; not reconstructable or an independently repairable source finding |

`FIND-1` through `FIND-6` above abbreviate the full stable IDs in this ledger.

## Authority, documentation and verification limits

The spec's reference to inherited elevated payload-read protection does **not**
authorize a new result permission. Its scope is inheritance of the existing
outcome's controls. Narrow Bifrost authority at
`architecture/bifrost-design.md:1481–1484` explicitly makes sensitive metadata
descriptive and adds payload permission only for `vala.gateway.calls` request/
response columns. Source `oracle::authorize_payload_columns` and existing
execution read/query paths agree. No source-backed new access exists. Retain
query-read plus tenant scoping; align the spec's inherited-protection wording
with that owner as a clarification. A new result permission would be a material
security decision outside this remediation, and is not prescribed.

The new custom-data recipe shows Python only; a search of public docs found
no Rust/TS typed-operation examples. The spec asks for all three SDKs. This is
a bounded documentation completeness correction to batch with the remediation,
not a separate blocking source finding or new API decision: explain code and
Agent declaration placement, per-requirement result nesting and the existing
typed reads in the current language-tab/example conventions, then regenerate
the existing mirror. No extra runtime feature or new docs framework is needed.
The standards report's generated-surface coverage is not proof of that missing
prose. Do not convert this wording/coverage note into a style-only review round.

Independent checks in this validation were read-only Git/CodeGraph/source
inspection, `git diff --check` (PASS), offline/no-sync Python public Prompt
rendering plus installed JSON Schema checks, and small Node expression checks.
The first Python reproduction initially selected the wrong rendered dictionary
level (`response_format` instead of `body.response_format`); correcting only
the inspection produced the results reported above. No source, lockfile,
environment or dependency was changed. The optional reference-preserving
resource experiment demonstrates a conventional correction exists; it does not
replace production Rust closure proof.

The orchestrator's frozen Python typecheck and discovery's frozen auth-unit
checks support the updated candidate. Original recorded journeys/gates and the
three unchanged-Rust focused checks from attempt 01 remain available evidence,
not fresh reruns here. I did not run Cargo, broad SDK/database journeys, crash
tests, documentation builds, generation or aggregates. No historical RED can be
manufactured after implementation; retain that transparent limitation. No
accepted prior finding is claimed closed. Required remaining risks are the
six concrete findings above; the user-directed ty `.get` issue is no longer one.
