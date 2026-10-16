---
id: SPEC-verifier-building-blocks
revision: 6
status: approved
---

# Verifier building blocks

## Objective and approval

Let a developer describe the evidence they have, prepare it when necessary,
and check declared requirements through one Verifier interface. The same
primitives must cover a request assertion, a document review, an agent trace,
a multi-step review, and scheduled statistical drift without choosing a
Verifier implementation kind.

The everyday model is:

```text
inputs → optional preparation steps → requirements → verdict
```

Revision 1 was explicitly approved by the owner in this conversation
("spec is approved decompose into 5 tasks"). Revision 2 records the owner's
subsequent agreement to retain the batched submission path and make replay
conflict confirmation asynchronous after discussing the cost of putting
Postgres writes back on the request path. These decisions introduce no per-request Postgres enqueue transaction.
Revision 3 records the owner's explicit approval ("agree. lock these") of
the extraction output types and semantics in REQ-004. Earlier extraction
sketches are superseded. Revision 4 records the owner's approval of the Monty
limits and startup environment configuration, doubles warm/max workers to
2/8, and replaces the suggested one-second checkout timeout with bounded
waiting under the caller deadline. Revision 5 records the owner's explicit instruction to provide a built-in
Agent tool registry, resolve declared names automatically at runtime, and
preserve the original subject or valid delegated authority on every protected
tool call. Revision 6 records the owner's explicit requirement that every
tool invocation follows Wyrd's global-or-scoped permission rule in addition to
its underlying resource permissions. The remaining approved behavior is unchanged.

The five agreed simplifications remain: preserve missing results in aggregation;
carry original-source citations through derived output; use unique names and
explicit slot mappings; separate evidence recording from execution requests;
reuse existing queued-work behavior. The last rule preserves retry protection
at batched acceptance rather than promising synchronous conflict rejection.

The owner also requires a working TypeScript demo, equivalent examples in
Python, first-class Rust support alongside both languages, docsite and
architecture updates, and delivery in four or five cohesive tasks with
parallel work where dependencies allow. This specification creates no tasks.

## Baseline and authority

The change branch is `change/verifier-building-blocks`, based on
`change/deployment-addressing` after its merge of main, inspected at
`982a4900ad6c2f874bd0e97f54c11645d1e84be5`. This is the implementation baseline,
not a claim that all prerequisite verification has passed.

The base already introduces Agent `execution_route`, shared Prompt compilation,
and Workflow removal. This change must preserve those contracts. It adds
Verifier preparation and judgment through the existing Skald agent loop;
it must not assume the existing judge invoker already runs that loop.

[AGENTS.md](../../../AGENTS.md), [agent rules](../../../architecture/agent-rules.md),
[Wyrd design](../../../architecture/wyrd-design.md),
[Wyrd doctrine](../../../architecture/wyrd-doctrine.mdx), and
[Bifrost design](../../../architecture/bifrost-design.md) remain governing
authorities. This approved revision replaces the typed Drift/Eval/Task Verifier
split and observation-triggered Eval path described there. Approval covers
that explicit change; unrelated authority remains in force. The owning
architecture prose must be updated with the implementation before completion.

[proposal.md](proposal.md), [example.md](example.md), and the
[verification control-flow diagram](architecture/verification-flow.html)
explain the source intent, worked examples, and end-to-end runtime/storage
flow. Their Rust skeletons, dependency API sketches, historical baseline
descriptions, and task outline are not implementation requirements. This
approved specification governs disagreements with those supporting documents.

## Scope and non-goals

In scope:

- One Verifier authoring contract, registration validation, execution engine,
  result contract, and the affected persistence and scheduling integration.
- Extraction, Agent and Python code preparation; assertion, trace, Agent,
  Python code, and drift checks.
- Real-time `verify`, background `submit`, execution status and bounded waiting.
- Rust, Python, and TypeScript SDKs over the shared client, HTTP, MCP, and
  affected CLI/UI consumers, schemas, errors, and documentation.
- Matching runnable support-policy examples in all three languages, the
  working `wyrd-example-ts` application, and docsite/architecture updates.

Out of scope: nested Verifiers, loops, a new Workflow Card or application
runtime, human verdict submission, scenario simulation as a new graph action,
server-side Office/PDF rendering, new drift algorithms, external writes from
checks, regulated-data erasure guarantees, and a new durable job or result
recovery system. Existing scenario simulation is not removed merely because
it is not a new action. Evidence garbage collection is not introduced here.

## Required behavior

### REQ-001 One declarative Verifier

A subject-less `VerifierSpec` declares `inputs`, optional `steps`,
`requirements`, and `decide`, with optional description and result capture
configuration. It keeps the normal `wyrd/v1` Card envelope and exact version
identity. There are no Drift, Eval, or Task Verifier kinds or authored
`implementation` discriminator.

Inputs name evidence. Steps produce data. Each requirement has `id`,
`statement`, and one `check`, and produces one verdict. Requirements can be
attached to a step or to the whole Verifier. A Verifier may have no steps;
an empty set of requirements produces an inconclusive result, never success.

Every step ID and requirement ID is unique within the entire Verifier,
including across different steps and across the two categories. Registration
refuses duplicates. These IDs are used unchanged in references, results,
route overrides, and fitted drift-state keys.

### REQ-002 Named evidence slots

| Slot type | Evidence |
| --- | --- |
| `json` | JSON, optionally constrained by JSON Schema |
| `media` | A pinned file with accepted `formats` and optional lower `max_bytes` |
| `trace` | A typed trace reference resolved through existing tenant-scoped trace reads |
| `samples` | Named feature measurements or metric samples recorded for a scheduled window |

Slots are required by default. Unknown slot names, wrong value types, invalid
trace identities, and values outside declared schema or size constraints are
refused with structured validation errors. A missing required slot is missing
evidence: checks that need it are inconclusive and never silently excluded.
An omitted optional slot resolves to null; a check that needs a non-null value
is inconclusive, while an explicit null assertion or `when` can inspect it.
Scheduled bindings must be capable of filling every required slot.

Callers supply named values using each language's ordinary argument/map
conventions. A single-slot Verifier also accepts one positional value. The
slot type determines projection; JSON and samples need no wrapper. SDK local
validation uses hydrated contracts; server validation remains authoritative.

### REQ-003 References and graph execution

A single scope document contains original `inputs`, each finished step's
`status`, `output`, and requirement verdicts, and the current requirement's
`id` and `statement` during its check. References use `inputs.<slot>`,
`steps.<id>.output`, `steps.<id>.status`, and `requirement.id` /
`requirement.statement`, with dotted child fields and array indexes. A step's
requirement verdicts are readable as
`steps.<id>.requirements.<requirement-id>.outcome` or `.reason`.
There is no `$.` prefix or bare `output`. Wildcards are supported only in an
assertion's `each` selector.

References in an action or `when` establish dependencies; `depends_on` can
add ordering without data. Registration rejects cycles, unknown references,
position violations, and nonexistent fields when the output schema is known.
Unknown output shapes remain usable, with missing-value behavior at execution.

Independent steps can run concurrently under execution budgets. A step runs
its action and then its requirements before dependents start. Its requirements
see original inputs, its own output, and upstream outputs/statuses. Whole-
Verifier requirements run after all steps and can see every output/status.
Actions and `when` can read upstream requirement verdicts; checks cannot read
other requirement verdicts. Requirements never feed each other, and each
verdict reaches aggregation exactly once.

### REQ-004 Preparation actions

The closed action set is `extract`, `agent`, and `code`.

- `extract` reads declared media and returns the format-discriminated JSON
  contract below, shared by Rust, Python, TypeScript, assertions, and Agent/code
  inputs. Reuse third-party readers internally and project only these fields.
  Extraction is text and structure, not a claim of visual/layout fidelity.
- `agent` invokes an inline, file-authored, or registered Agent through the
  existing Card resolution/registration workflow. Output follows the schema
  derived from its Prompt, or is text when no structured output is declared.
- `code` runs immutable inline source or an exact Artifact reference using
  `runtime: python` (the default and only supported runtime), with a named
  entrypoint and explicitly bound inputs. It returns JSON data.

Actions read and compute. They cannot mutate external systems, register Cards,
or dispatch Operators. Local scratch writes within the sandbox are allowed.
A step supports bounded retry and timeout. Attempts, duration, failure, and
final status remain visible; retries never conceal the final failure.

#### Locked extraction output types

The TypeScript notation defines the language-neutral JSON wire shape; it
neither selects a parser dependency nor adds a TypeScript-only implementation.
Every field is required. Unavailable values use `null` only where the type
permits it; empty collections use `[]` and empty text uses `""`.

```typescript
type ExtractOutput =
  | PdfOutput
  | PptxOutput
  | DocxOutput
  | XlsxOutput
  | CsvOutput
  | JsonOutput
  | TextOutput;

type PdfOutput = {
  format: "pdf";
  pages: {
    number: number; // 1-based
    text: string;
  }[];
};

type PptxOutput = {
  format: "pptx";
  slides: {
    number: number; // 1-based, presentation order
    title: string | null;
    paragraphs: string[];
    tables: {
      rows: string[][];
    }[];
    notes: string;
  }[];
};

type DocxOutput = {
  format: "docx";
  blocks: (
    | {
        kind: "paragraph";
        text: string;
        style: string | null; // stored paragraph style ID
      }
    | {
        kind: "table";
        rows: string[][];
      }
  )[]; // document order
};

type XlsxOutput = {
  format: "xlsx";
  sheets: {
    name: string;
    cells: {
      address: string; // A1, B2, etc.
      value: string | number | boolean | null;
      formula: string | null;
      error: string | null;
    }[];
  }[]; // workbook order
};

type CsvOutput = {
  format: "csv";
  rows: string[][];
};

type JsonOutput = {
  format: "json";
  value: JsonValue;
};

type JsonValue =
  | null
  | boolean
  | number
  | string
  | JsonValue[]
  | { [key: string]: JsonValue };

type TextOutput = {
  format: "text" | "markdown" | "html";
  text: string;
};
```

Locked semantics:

- Counts come from array lengths. There are no duplicate `page_count`,
  `slide_count`, or line-count fields.
- Table cells contain extracted text. Merged cells use the top-left value
  and empty strings for covered positions.
- XLSX sheets retain workbook order; cells retain row/column order. Addresses
  are worksheet addresses, not offsets into the returned array. Absent cells
  are omitted. `value` is the stored value or cached formula result; formulas
  are never recalculated. Numeric dates remain Excel serial numbers initially.
  Error cells have `value: null` and the error in `error`.
- CSV preserves every row, including the first; it infers neither headers nor
  value types. JSON preserves the parsed JSON value. Text, Markdown, and HTML
  preserve source text; structured Markdown/HTML extraction is not promised.
- Visual Agent checks use the original media through the existing input
  binding when the selected provider/model supports its visual content.
  These outputs add no extracted-image files, media references, or helpers.
  Native Office rendering and OCR remain outside this change.

Concrete DOCX and XLSX outputs:

```json
{"format":"docx","blocks":[{"kind":"paragraph","text":"Refund policy","style":"Heading1"},{"kind":"table","rows":[["Window","Days"],["Standard","30"]]}]}
```

```json
{"format":"xlsx","sheets":[{"name":"Refunds","cells":[{"address":"A1","value":"Days","formula":null,"error":null},{"address":"B2","value":30,"formula":null,"error":null}]}]}
```

For an extract step named `document`, an assertion can compare
`steps.document.output.blocks[0].text` with `"Refund policy"`. For an extract
step named `workbook`, it can compare
`steps.workbook.output.sheets[0].cells[1].value` with `30`, or use `each` over
`steps.workbook.output.sheets[*].cells[*].error` expecting `null`.
Array indexes are zero-based; PDF page and PPTX slide numbers are one-based.
For DOCX/XLSX structured citations, the existing JSON-pointer locator addresses
this canonical extracted JSON under the original input slot: for example,
`/blocks/0/text` or `/sheets/0/cells/1/value`. Validation uses the pinned source
and its extraction version; the pointer identifies neither a raw binary-file
byte offset nor an unrelated caller-supplied JSON document. Existing page and
slide locators continue to identify the original PDF/PPTX location.

### REQ-005 Requirement checks

| Check | Required behavior |
| --- | --- |
| `assert` | Existing typed comparison operators over `path` or `each` and an expected value |
| `trace` | Existing trace assertions over declared trace evidence, including span existence, order, and attributes |
| `agent` | An Agent returning the closed Verdict schema: outcome, reason, citations |
| `code` | Sandboxed Python returning the same Verdict schema |
| `drift` | Existing PSI, SPC, or Custom decision and full statistical detail |

A requirement result is `passed`, `failed`, `inconclusive`, or
`not_applicable`. Infrastructure, provider, malformed-output, and missing-
evidence failures are not judgments that the subject violated the requirement;
when isolated to a check they produce an inconclusive result with a reason.
Execution-wide cancellation, deadline, and orchestration failure remain
execution statuses/errors rather than fabricated check failures.

Check-position Agents have no tools or workspace. Registration refuses them
if their tools or derived response schema violate this rule. The statement
is passed by an explicit `requirement.statement` binding, not hidden injection.

### REQ-006 Missing evidence, skips, and aggregation

The engine preserves the cause of a skip, including through dependent steps.

| Situation | Requirement outcome |
| --- | --- |
| Step intentionally skipped by false `when`, or blocked only by an intentionally skipped dependency | `not_applicable` |
| Step failed or timed out, or a dependency was blocked by such a failure | `inconclusive` |
| Check needs output from an intentionally skipped step | `not_applicable` |
| Check needs failed/timeout-blocked output, or an otherwise missing required value | `inconclusive`; do not call the check |

When both causes affect evidence needed by one requirement, failure wins:
its result is inconclusive. Reading a step's status alone does not require
its output and can still be checked.

`decide` defaults to `all`. Its authored forms are `all` and
`{ pass_rate: { threshold: <number> } }`. The threshold is finite and greater
than zero and at most one. Exclude only `not_applicable` requirements.

- With no applicable requirements, return `inconclusive`.
- `all`: any failed requirement means failed; otherwise any inconclusive
  requirement means inconclusive; otherwise passed.
- `pass_rate`: divide passed by all applicable requirements, including
  inconclusive ones. At or above the threshold means passed. Below it,
  return inconclusive if any applicable result is inconclusive, otherwise
  failed. One pass and nine inconclusive results is 10%, not 100%.

### REQ-007 Agents reuse Prompt and execution ownership

Agent owns Prompt, tools, run configuration, and `execution_route`. Prompt
owns provider, model, content, inference settings, and response schema.
Input placeholder names and output shapes are derived through the shared
compiler; Verifiers do not reintroduce authored `variables`,
`media_variables`, or `response_type`.

The keys in an Agent call's `inputs` match its Prompt placeholders in both
directions. Text and native media use their existing placeholder forms.
Native media bindings accept only provider-supported formats and limits.

Server-hosted calls require `wyrd_gateway`, using the existing tenant gateway,
authorization, accounting, and capture path. Offline Vala execution preserves
native execution where host capabilities support it. Overrides are keyed by
the unique step/requirement ID and replace that call's route whole, without
changing the registered Card. Effective routes and exact Agent/Prompt
identities are recorded. Existing unsupported-provider refusals remain.

### REQ-008 A bounded workspace and tool harness

A step Agent can declare both injected `inputs` and a `workspace` mapping
virtual `/inputs` paths to media, JSON, or prior output references. Only
injected values reach the model directly; workspace files require tool reads.
The file set is authorized, pinned, and materialized before the call. `/inputs`
is read-only; `/work` is bounded, initially empty scratch retained across tool
calls within that invocation. All scratch is removed when the call ends.

Wyrd provides a server-owned built-in Agent tool registry. An author selects
built-ins with `AgentSpec.tool_names`; no SDK decorator, callable upload,
manual registration, or MCP connection is required. Initial built-ins are:

- `cards.list`, `cards.get`, `cards.get_by_ref`;
- `bifrost.list_tables`, `bifrost.describe_table`, `bifrost.query`;
- `fs.list`, `fs.read`, `fs.search`, `code.run`.

All deployments register these capabilities for runtime resolution; availability
is not permission to access a resource. The Invoker resolves the Agent's
explicit names automatically before its first model call, attaches only those
tools, and uses the existing Skald Agent loop for dispatch. Unknown names,
unsupported capabilities, or missing required workspace fail explicitly;
never silently omit a tool or fall back to another implementation. Check-position
Agents remain tool-less. Built-in names are valid on registered Agent Cards;
arbitrary runtime-local client callables remain non-registrable.

Reuse the existing `ToolResolver`/`ToolRegistry` and `AgentTool` machinery.
Keep the built-in catalog shared, but bind executable access to each
invocation's verified authority, cancellation/deadline, and workspace. A shared
registry or cached Agent must not retain another call's principal or scratch.
Call existing authorized Rust capability owners directly, beneath MCP/HTTP;
no loopback protocol request or duplicate Card/query implementation is needed.

Wyrd's general authorization rule is **global or exact scope permission**.
Use the existing resource/action/scope coverage and intersection semantics for
every permission check, including tools; this is not a separate tool policy.
A covering schema-level Bifrost grant, for example, continues to cover tables
in that schema under the existing rules.

Every tool invocation, including workspace and Python tools, first requires
invoke permission on the tool resource, either globally or scoped to the
requested built-in. Extend the existing resource/action/scope representation
for the tool resource; do not introduce another authorization service. The wire
permissions are:

```json
{"resource":"tool","action":"invoke","scope":"all"}
```

```json
{"resource":"tool","action":"invoke","scope":{"tool":"bifrost.query"}}
```

The typed tool scope is an exact built-in name from the REQ-008 catalog, not
an arbitrary executable, prefix pattern, or filesystem path. Global covers
all registered built-ins; the tool-name scope covers only that name. Distinct names
have disjoint scopes. Existing role/grant validation, wildcard semantics,
permission intersection, delegation attenuation, issuance/forwarding, schemas,
and stable denial errors must handle this scope through their existing owners.
No tool permission is inferred from an Agent declaration, `verifier:run`, or a
domain grant; use explicit grants or an existing covering wildcard grant.

After tool authorization, the called capability still enforces its own
resource permissions and scopes. For `bifrost.query`, both tool invocation
permission and existing Bifrost query-read permissions for every accessed
table/schema are required, including multi-table queries. A global tool grant
grants no Bifrost data access. Conversely, Bifrost read access grants no tool
execution permission. Cards tools likewise retain existing Card read checks.
Filesystem/code tools still need tool invocation permission and remain confined
to the invocation's authorized mounts and execution limits; they gain no host
filesystem or network access. Deny before running the tool when its invocation
grant is absent; domain denial cannot return partial data. Each allow/deny
permission decision follows the existing non-blocking audit rule.

Every protected tool call uses the verification's accepted original subject
(`sub`) and applicable scopes, or valid delegated permissions originating from
that subject through the existing delegation mechanism. Delegation cannot
widen the accepted authority. Preserve tenant, subject, actor, and delegation
history through background handoff/retry/restart. SYSTEM may be the recorded
executor but grants no extra permission. Scheduled execution retains the
existing accepted binding-owner authority rule. Tool/model arguments cannot
supply or override authentication context. Keep credentials out of prompts,
workspace files, and Monty workers. Enforce permission and audit at the existing
capability operation on every protected call; denied calls cannot disclose
resources or bypass checks by naming a built-in directly.

There is no fetch tool or arbitrary network access. Filesystem tools require
a workspace; a workspace requires at least one filesystem/code tool. `fs.read`
returns bounded text with page/slide/line locators and supports bounded reads.
Search returns matching locations. Tool names, arguments, output digests, and
returned byte counts are recorded.

`code` and `code.run` use sandboxed Python worker processes, not host Python.
They may access only explicitly mounted scratch, never credentials, host
files, arbitrary host callbacks, subprocesses, or the network. No third-party
package execution is promised. The Monty-based runtime proposed in the
supporting document must demonstrate these properties before shipping; its
unverified dependency APIs are not specified here. Workers have a separate
unprivileged identity and empty credential environment. Interpreter state
cannot cross invocations. A worker crash/hang must not take down the server.

### REQ-009 Citations stay attached to original evidence

A citation contains an original input `slot` and a typed locator: page,
slide, line range, JSON pointer, or page region. It never identifies a
transient scratch path as the original source. Workspace paths are mapped
back to original input slots.

A preparation step may include citations in its declared ordinary output.
Authors pass those findings and citations to a downstream check through
explicit input bindings. There is no new evidence wrapper and no requirement
that the downstream Agent read the original file again. Derived data without
supporting source citations cannot manufacture a cited pass.

The server validates that cited locations exist in pinned original evidence
reachable through the call's declared bindings. Citations supplied with
upstream output can be reused by a tool-less judge. A passed Agent check with
no valid evidence citation becomes inconclusive. Locator validity establishes
provenance, not the truth of the model's conclusion.

### REQ-010 Files and media formats

`File` is a lazy local path/bytes handle with optional format; constructing
it performs no IO or Card registration. During `verify`/`submit`, the shared
client validates slot compatibility, stream-hashes files, obtains upload plans,
and sends missing bytes directly to tenant object storage. Files are pinned
by SHA-256; deduplication stays within tenant-authorized storage. An uploaded
customer file does not become an Artifact Card. `ArtifactRef` selects an
existing registered Artifact, with its normal authorization and identity.

Reuse existing single-PUT upload plans and the storage client. Evidence uses
the tenant-qualified `evidence/sha256/<digest>` namespace and creates no
multipart/upload metadata row. File contents never pass through Postgres or
the verification request body. The server checks length, digest, and sniffed
format when reading; mismatches cannot produce a pass.

One shared `MediaFormat` vocabulary in `skald-spec` accepts short tokens and
MIME aliases and serializes canonical short tokens. Preserve distinct legacy
and OOXML formats: `ppt` is not `pptx`, `doc` is not `docx`, and `xls` is not
`xlsx`. The vocabulary covers existing native image/document/text formats,
OOXML, JSON/CSV/HTML/Markdown, and zip/tar.gz archives. Registration rejects
known incompatible extraction/native-media bindings. Archives are usable as
bounded workspace directories, never native model attachments. A caller
supplies a PDF/image rendition when a visual check needs one.

### REQ-011 One real-time and one background entry point

Rust, Python, and TypeScript expose `run.observe.verify` and
`run.observe.submit`, with idiomatic language naming and typed results.

| Evidence mode | `verify` | `submit` |
| --- | --- | --- |
| Run (`json`, `media`, `trace`) | Execute now; return `VerificationResult` | Queue one execution; return `VerificationExecution` |
| Window (`samples`, drift) | Refuse with remediation to use `submit` | Record samples through existing Bifrost ingestion; return `Recorded` |

Routing follows the closed check/evidence kinds. Registration refuses mixing
Run and Window checks or mixing samples with other slot types. A Window
Verifier requires a schedule binding; submitting samples creates no execution
handle because the schedule later judges a window. Rust's return type is
`Submission::{Recorded, Queued(VerificationExecution)}`; Python and TypeScript
project the same distinction without pretending recorded samples are a job.

`verify` has a 60-second execution deadline. Both Run entry points use the
same engine and verdict semantics. No wait flag, separate Eval submit API,
or immediate drift-scoring escape hatch is added.

### REQ-012 Execution identity and public control

`run_id` is the existing client-created application invocation ID. One Run
can have many verification executions. `execution_id` is the server-issued
identity of one exact Verifier execution; scheduled work need not have a
`run_id`. The public type is `ExecutionId`, consolidating the base's separate
verification run/direct-execution identities without aliases.

| HTTP route | Contract |
| --- | --- |
| `POST /v1/verification/evidence` | Declare files; receive missing-file single-PUT plans |
| `POST /v1/verification/execute` | Submit exact verifier/subject, optional Run, named input declarations and route overrides; return 200 result |
| `POST /v1/verification/enqueue` | Same Run evidence declaration; return 202 with execution ID |
| `GET /v1/verification/executions/{execution_id}` | Authorized queued execution status and result when available |

File declarations carry digest, length, format, and optional filename;
Artifact declarations carry the exact Card reference and optional file path;
JSON and trace declarations are bounded typed/JSON request data. Requests
never carry trusted tenant or principal fields.

`VerificationExecution` retains one shared client and exposes ID, `status`,
and `wait(timeout)` with capped polling backoff. It can reconnect from an ID
in another process through the existing `connect` conventions, with ambient
or explicitly supplied client; Python retains its lazy form. No new required-
client constructors or language-specific transport are introduced.

Status distinguishes queued, running, completed, errored, cancelled, and
timed-out work. A not-yet-created or unknown tenant-scoped row returns not
found, never accepted merely because an ID has a recent UUID timestamp.
`wait` tolerates delayed visibility within its caller-supplied timeout.
Completed without a visible result remains pollable; `wait` returns only a
real result, a terminal execution error, or a timeout. It never invents a
result from completion status. Direct `verify` returns its result immediately
and creates no queued-status handle.

Existing binding inspection/readiness remains available. HTTP/MCP and
existing CLI/UI verification consumers adopt execution terminology and the
new result contract; old `/runs` and kind-specific execution contracts are
removed without compatibility aliases. MCP projects execute, enqueue,
evidence planning, and execution status under the same domain authorization.
The active `sdk-verification-control` proposal's conflicting `/runs` handle
is absorbed here, not implemented as a second public control workflow.

### REQ-013 Reuse the queued-work lifecycle

No Postgres write is added to either Run request path. `verify` runs inline
and stages inputs/outcome on the process Scribe outbox. `submit` stages inputs;
only after Scribe acknowledges them does the existing batched request-outbox
pattern create a queued row. The existing runner claims it and uses the same
engine. Retarget the current observation acknowledgment/request machinery;
do not add a second scheduler, job service, WAL, or durability system.

Recording evidence is not an instruction to execute. Input records carry a
server-owned `queue_execution` marker: false for `verify`, true for Run
`submit`. Only the latter activates queued work. Retrying or replaying input
persistence cannot execute `verify` again or dispatch its Operators.

Retain `Idempotency-Key`, request hashing, and tenant/subject/actor scoping,
but decide replay/conflict when the existing batched queue writer accepts the
request. SDK transport retries reuse the same key. With a key, identical
requests receive the same execution ID even before queue visibility; a
request with different content has a distinct submission identity so its
refusal cannot overwrite the original execution. Without a key, separate
requests remain separate submissions as today.

202 confirms staging, not durable queue acceptance or replay/conflict
confirmation. The batched writer accepts one request per scoped key and
records a conflicting submission as terminal `errored` with the existing
idempotency-conflict code. It never executes that submission or dispatches
Operators. `status` exposes that refusal and `wait` raises the same structured
error. The original execution/result is unchanged. Matching retries do not
create another logical execution or dispatch. Concurrent conflicting requests
are ordered by the queue writer's acceptance transaction, not HTTP arrival.

Use the existing queue rows, key/hash validation, transactional conflict check,
and outbox owners for this behavior. No separate reservation service, receipt
resource, or per-request Postgres write is added. A staged submission lost
before queue acceptance still has no durable outcome and can time out.

Accepted caller authority and delegation, without credentials, travel through
the acknowledgment handoff and queued row and survive claim/retry/restart.
Manual submission never switches to binding-owner or SYSTEM authority.
Scheduled acceptance keeps its existing binding-owner rule. Preserve leases,
fencing, cancellation, bounded shutdown, and reaction deduplication.

202 means accepted into the in-memory path. Abrupt death or an expired drain
can lose staged work before acknowledgment. Settlement does not wait for the
result's Scribe acknowledgment; a completed execution can lack a retained
result. Count observable losses, preserve the settled state, and surface a
bounded wait timeout. Do not claim exactly-once model execution across crashes
or silently retry lost work as a new submission.

### REQ-014 Scheduled drift and reactions

A binding keeps `verifier`, optional `runs_on`, and `on_failure`, and adds an
explicit `inputs` map for scheduled evidence. Each required slot names its
existing feature/metric source selector; names are not inferred. For example,
`current: { features: [income, dti] }` and
`latency: { metric: latency_p95 }` select separate slots within the same
subject's schedule window. Sample slots declare their measurement names
consistently, including metric names. No new source registry is added.

`schedule` is the only automatic Trigger activation. Preserve fixed cron
windows `[previous boundary, due)`, timezone, and missed-occurrence behavior.
Caller-supplied Run inputs use named arguments rather than schedule mappings.

PSI, SPC, and Custom retain existing statistical algorithms, threshold and
minimum-sample rules, baseline readiness, and insufficient-data behavior.
Full per-feature statistics, thresholds, counts, flags, and window identity
remain in the requirement detail. Fitted state remains owned by `vala-drift`,
keyed by tenant, exact Verifier, and globally unique requirement ID. There is
no mutable implicit baseline or immediate drift route.

Only failed background verdicts produce the binding's independent
`on_failure` Operator dispatches. Inconclusive, execution errors, and direct
`verify` do not dispatch. A replayed queue request cannot create duplicate
logical dispatches. Existing Operator delivery retry semantics remain.

### REQ-015 One attributable result contract

A result carries execution ID, optional application Run ID, and an outcome
containing the overall verdict, requirement breakdown, step results, and
original evidence identities/digests. It also retains exact subject and
Verifier identity and existing result/principal attribution in stored rows.

Each requirement includes ID, containing step if any, statement, outcome,
reason, permitted actual value, citations, typed check-specific detail where
applicable, and the exact check configuration/Agent/Prompt/code provenance.
Each step includes ID, status, permitted output, attempts, duration, failure
reason, and tool provenance. Capture redaction preserves the existing rule
for actual values; it is not advertised as erasing uploaded files, all step
outputs, or stored inputs.

Execution status is separate from verdict. A failed assertion is a normal
result, not a transport error. Deterministic checks stay deterministic;
provider output is attributable stochastic evidence, not a promise of
identical regeneration. Record code hash/runtime, extraction version,
Agent/Prompt versions, effective routes, drift baselines/windows, and input
digests needed to explain the judgment.

Retain `vala.verification.results` as the canonical analytical result read
surface, extended for the generic outcome, and use
`vala.verification.execution_inputs` for pinned input declarations and queued
activation context. Queued control rows use `wyrd.verifier_executions`.
Keep existing useful analytical detail projections where needed; they do not
create another kind-specific SDK result contract. Server-owned tables remain
protected against generic public writes. Use existing Scribe writes and
Bifrost reads, not a parallel result store.

### REQ-016 Matched runnable examples and demo

Use the existing `examples/support-desk` story and shared versioned Cards to
show the same workflow in Rust, Python, and TypeScript. Extend the existing
language entry points rather than introducing a new application framework.
The Python and TypeScript examples must differ only where language idioms
require it; the Rust example demonstrates the same capability.

The shared `support/policy-check` Verifier takes an answer (`json`) and policy
PDF (`media`). An Agent step reads the policy through `fs.read` and returns
findings with original-policy citations. A deterministic assertion rejects
an answer containing "guaranteed refund". A tool-less Agent requirement
judges the answer/findings against its explicit statement and cites the same
policy. All languages demonstrate registration/hydration, `verify`,
`submit`, execution-ID reuse in another process, bounded waiting, and reading
the requirement breakdown, including a failing answer.

The application in `wyrd-example-ts` must run this same scenario against the
integrated server using a real built SDK, not a temporary-checkout symlink.
Its request flow returns the immediate verdict; its background-review flow
returns an execution ID and exposes the resulting breakdown. Include shared
sample evidence, setup/run instructions, and checks sufficient to reproduce
the demonstration. A screenshot or mocked UI alone is not completion.

### REQ-017 Docsite, architecture, and consumer completion

The docsite explains the four primitives, reference syntax, graph order,
skip/inconclusive rules, citations, file inputs, Agent/code limits, scheduled
drift, and `verify` versus `submit`. Runnable usage is shown in Rust, Python,
and TypeScript with the same scenario and result meanings. Keep HTTP/MCP
and machine-readable contracts discoverable beside those guides. Publish the
complete REQ-004 extraction types, one concrete JSON example per supported
format, reference/assertion examples, ordering/null/table/cell semantics,
citation locator meanings, and native-media versus extraction limits in the
docsite reference. Publish all Deployment limits environment variables, defaults,
units, startup validation, restart requirements, pool scope, capacity behavior,
and memory/security boundaries. Document built-in tool names, automatic runtime
resolution, registration, position restrictions, and original-subject/delegated
authorization. Do not leave DOCX/XLSX shapes to implementation discovery.

Update the existing how-verification-works, real-time verification, agent
behavior, drift, and run/inspect-evidence guides instead of leaving competing
old instructions. Examples must use the integrated SDK construction and
Agent/Prompt contracts. Update generated references from their sources,
authored Python typing, runtime OpenAPI, and the MCP catalog as applicable.
Existing CLI/UI consumers must render/use the new contract without depending
on removed kinds or observation activation.

Keep the [verification control-flow diagram](architecture/verification-flow.html)
aligned with the approved specification, including skip/aggregation rules,
original-source citations, submit-only queue activation, replay, and delayed
result visibility. It is part of the architecture documentation, not an
independent source of conflicting runtime rules.

Update Wyrd design, doctrine, relevant references, and repository standards
that still prescribe the removed split. Update Bifrost architecture for
reserved execution-input/result tables and acknowledgment integration.
Document durable ownership, timing, authority, loss, citation, and reaction
rules; do not turn architecture into a private-method description. Keep
Card kinds at the base's 14; this change adds no new Card kind and does not
repeat Workflow removal as new work.

## Invariants and required boundaries

- **INV-001 Server ownership.** Pure contracts/validation/errors belong to
  `wyrd-spec`, which stays IO-, async-, and PyO3-free. Shared media vocabulary
  belongs to `skald-spec`. Vala owns reusable orchestration and statistical
  engines; Skald owns reusable Agent runtime/provider behavior; the server
  owns hosted admission, authorization, evidence resolution, sandbox policy,
  scheduling integration, and reactions. Extend complete existing workflows;
  leaf-call reuse does not justify a duplicate orchestrator.
- **INV-002 First-class clients.** `wyrd-client` is the one shared client
  implementation. Rust, Python, and TypeScript project the same durable wire
  contracts, errors, and workflows. Preserve ambient/explicit client
  construction; language wrappers do not own server behavior.
- **INV-003 Tenant and permission boundaries.** Derive tenant and principals
  from verified authentication. Authorize exact Verifier invocation using
  existing `verifier:run` rules and authorize all Cards, traces, files,
  baselines, model calls, and result/status reads in that tenant. A digest,
  execution ID, binding, or SYSTEM actor grants no authority. Preserve
  subject/actor attribution, delegation history, and signed Card scope where
  applicable. MCP write scopes do not replace domain checks.
- **INV-004 Audit and durability.** Permissions block; audit does not.
  Decisions stage allowed/denied events on the one Scribe outbox. Queue and
  result mechanics produce operational lineage, not invented permission
  audits. No new audit/result writer or durability claim.
- **INV-005 No application runtime.** Verifier work is bounded read/compute;
  external writes remain Operator-owned. No loops, arbitrary tool plugins,
  network/file escape, host Python, credential access, or application hosting
  inside the new graph.
- **INV-006 Honest evidence.** Missing evidence, invalid cited passes, failed
  dependencies, and insufficient drift data cannot silently become success
  or intentional non-applicability. Persist enough exact identity to attribute
  the judgment; do not promise deterministic LLM regeneration.
- **INV-007 Resource isolation.** Bound queues, concurrency, input/output,
  extraction, scratch, worker memory/CPU, model calls/tokens, and execution
  deadlines. Cancellation stops descendant work and releases resources.
  Evidence paths and archives cannot escape their authorized scratch roots;
  reject unsafe links and decompression bombs. No interpreter or workspace
  state leaks between tenants or calls.
- **INV-008 Complete replacement.** Remove authored implementation kinds,
  obsolete direct input unions, old verification-run terminology/contracts,
  `observe.eval`, `observe.drift`, `observations_ready`, writer-Card Eval
  fan-out, and their replaced observation table path when consumers move.
  No aliases or second workflow. Existing unrelated observations and scenario
  evaluation are preserved. No deletion or reinterpretation of historical
  evidence merely to simplify the cutover; historical data is not relabelled
  as new generic results.

### Deployment limits

All limits below are server-startup environment settings. These are approved
target contracts, not a claim that configuration is already implemented.

| Environment variable | Default | Meaning |
| --- | ---: | --- |
| `WYRD_VERIFICATION_MONTY_MIN_WORKERS` | 2 | Prewarmed worker processes. |
| `WYRD_VERIFICATION_MONTY_MAX_WORKERS` | 8 | Maximum live worker processes. |
| `WYRD_VERIFICATION_MONTY_MAX_WAITERS` | 16 | Maximum in-process operations waiting for a worker. |
| `WYRD_VERIFICATION_MONTY_MEMORY_MIB` | 256 | Monty allocation budget per Python operation. |
| `WYRD_VERIFICATION_MONTY_PROCESS_MEMORY_MIB` | 384 | Hard memory ceiling for one worker process, including overhead. |
| `WYRD_VERIFICATION_MONTY_POOL_MEMORY_MIB` | 2048 | Hard combined memory ceiling for all worker processes. |
| `WYRD_VERIFICATION_CODE_TIMEOUT_MS` | 10000 | Complete Python operation, including checkout wait, compilation, execution, and parent-serviced filesystem calls. |
| `WYRD_VERIFICATION_AGENT_CODE_BUDGET_MS` | 60000 | Cumulative Python-operation wall time within one Agent invocation. |
| `WYRD_VERIFICATION_FILE_MAX_MIB` | 20 | Maximum bytes in one input file. |
| `WYRD_VERIFICATION_FILES_MAX_MIB` | 100 | Maximum combined input-file bytes per execution. |
| `WYRD_VERIFICATION_ARCHIVE_MAX_MIB` | 200 | Maximum decompressed bytes per archive. |
| `WYRD_VERIFICATION_ARCHIVE_MAX_ENTRIES` | 10000 | Maximum entries per archive. |
| `WYRD_VERIFICATION_ARCHIVE_MAX_RATIO` | 100 | Maximum decompressed/compressed size ratio. |
| `WYRD_VERIFICATION_EXTRACT_MAX_PAGES` | 500 | Maximum PDF pages or PPTX slides per extraction. |
| `WYRD_VERIFICATION_EXTRACT_TEXT_MAX_MIB` | 10 | Maximum extracted text bytes. |
| `WYRD_VERIFICATION_SCRATCH_MAX_MIB` | 250 | Maximum scratch bytes per invocation. |
| `WYRD_VERIFICATION_WORK_WRITE_MAX_MIB` | 50 | Cumulative writable /work budget per invocation. |
| `WYRD_VERIFICATION_TOOL_RESULT_MAX_KIB` | 64 | Maximum tool-result bytes, bounded while collecting output. |
| `WYRD_VERIFICATION_MAX_WORKSPACES` | 16 | Maximum concurrent scratch workspaces per server process. |
| `WYRD_VERIFICATION_TIMEOUT_MS` | 60000 | Overall immediate verify deadline. |

Read these variables once during server startup. Unset variables use the
defaults; changing a value requires a server restart. Values are positive
integers; `_MIB` and `_KIB` use binary units and `_MS` uses milliseconds.
Refuse startup for invalid/overflowing values or inconsistent settings:
minimum workers above maximum, allocation budget at or above the process
ceiling, pool ceiling below the process ceiling, file limit above combined
file limit, or writable-work budget above scratch limit. Report the offending
variable clearly; never silently disable a limit. Slot limits may lower, never
raise, the configured deployment limits. Agent/step/request deadlines still
apply; the earliest deadline wins.

One Monty pool belongs to each verification-runtime server process, shared
across tenants without sharing interpreter state. Only `code` actions/checks
and explicitly enabled Agent-action `code.run` calls acquire workers. Agent
checks, model waits, and non-Python tools do not reserve a worker. Each Python
operation uses a fresh interpreter session; workspace files may persist only
within the owning invocation. Discard a session after any resource-limit
breach; kill/replace crashed, hung, or compromised workers.

Worker checkout waits within the operation's remaining deadline, also bounded
by the enclosing caller deadline. There is no independent one-second checkout
timeout. Bound waiting operations with `MONTY_MAX_WAITERS`; when full, refuse
additional in-process admission through the existing capacity/error path.
Background executions remain in the existing queue until runtime capacity is
available rather than occupying an unbounded in-memory waiting list. Preserve
leases/fencing and cancellation while coordinating capacity; add no second
queue or scheduler. Python waiting/execution budgets do not reset across tool
calls or retries within the enclosing invocation.

The default 2 GiB worker-group ceiling remains in force with up to eight
workers. It does not promise eight workers can each consume their full
individual allocation budget simultaneously. Enforce aggregate and process
memory independently of Monty's allocator accounting; count idle workers too.
Server, extraction, model-response, and parent-side filesystem/output buffers
are outside the worker-group ceiling and need separate deployment headroom.
Worker termination under memory pressure must fail the affected operation
safely, never take down the parent server or fabricate a passing verdict.

Security remains mandatory: a separate unprivileged worker identity, empty
credential environment, OS-enforced network denial, per-invocation read-only
`/inputs` and bounded writable `/work`, parent-serviced capability-scoped file
access, no arbitrary host callbacks/subprocesses/host Python packages, and no
cross-invocation interpreter state. Environment settings cannot disable these
protections. Task 3 must prove the selected runtime and host enforcement can
provide them; unsupported isolation is not silently accepted.

## Acceptance criteria and evidence

These are completion obligations, not claims that tests have run. Planning
maps them to the smallest complete repository lanes and exact focused commands.
Per the owner's execution instruction, modules receive focused isolated proof
while developed in parallel. Full integration and user-journey suites run once
on the candidate with all five tasks merged, with failure-driven reruns only.
The journey tests are still authored with the owning behavior; this changes
when the full suites run, not the required coverage.

| ID | Required evidence |
| --- | --- |
| AC-001 | Contract/schema and registration evidence for REQ-001–003: no-step Verifier, multiple named slots, globally unique IDs, references, known/unknown output shapes, cycles, invalid positions, and run/window incompatibility. |
| AC-002 | Engine evidence for REQ-003–006: independent work can overlap, dependencies wait for step checks, conditional skips differ from failures through multiple dependencies, all-skipped is inconclusive, and pass-rate denominator keeps missing evidence. |
| AC-003 | Integrated Agent/extraction/code evidence for REQ-004–010: reused Prompt/route behavior, structured output, input and workspace channels, tools, citation propagation into a tool-less judge, refused invalid citations, timeout/retry, and bounded extraction matching every locked REQ-004 output, including merged tables, sparse/error/formula/date cells, empty data, and canonical source locators. |
| AC-004 | Sandbox and isolation evidence for INV-005/007: path/link/archive escape refusal, write restrictions, bounded output/resources, no credentials/network, worker crash containment, and no retained state across calls or tenants. Do not ship code execution on unproven isolation. |
| AC-005 | Real SDK → server → SDK journeys in Rust, Python, and TypeScript for REQ-011–013/015: inline JSON and file verification, background submission, status/wait, reconnect from an ID, persisted attributable breakdown, failed verdict as data, and delayed result visibility. |
| AC-006 | Cross-boundary negative/edge evidence: permission and cross-tenant refusal, wrong digest/format/schema, unavailable Agent/model, absent required evidence, stable-key replay/conflict, no activation from recorded `verify`, no duplicate logical dispatch, bounded admission/backpressure, cancellation, and outbox-loss/timeout semantics. Controlled loss injections may be supporting unit/integration tests where end-to-end forcing adds no boundary proof. |
| AC-007 | Scheduled drift integration/journey evidence for REQ-014: correct slot/source selection and cron window, all existing methods, baseline/readiness and insufficient data, full statistical detail, failed-only Operators, and preserved accepted authority on queued retry/restart. |
| AC-008 | Runnable matched Rust/Python/TypeScript support-policy examples and working `wyrd-example-ts` against the built SDK/server, covering both immediate and background flows and failing answers. Provider mocks support reproducible automated journeys; live-provider demo setup is explicit and not required by unit tests. |
| AC-009 | Consumer and documentation evidence for REQ-017/INV-008: served OpenAPI and runtime MCP schemas/behavior; typed exports, authored Python stubs, generated TypeScript declarations/errors; affected CLI/UI; no obsolete active contract/examples; aligned design, doctrine, references, and Bifrost descriptions. Docsite checks/build and example checks pass. |
| AC-010 | Format/lint, dependency and tenant-boundary checks, contract generation, relevant Rust/Python/TypeScript suites and journeys pass under repository verification policy. Evidence names the commands actually run; aggregate claims reflect `mise.toml`. No new permanent check is required merely to police this change's prose or task count. |

## Expensive-to-reverse decisions

- One authored Verifier vocabulary; globally unique block IDs; one result
  breakdown and citation model across languages and timing modes.
- Agent and Prompt reuse with position-specific capabilities, native routes
  limited by host support, and server calls through the existing gateway.
- Original-slot citations can travel through ordinary data outputs; downstream
  judges stay tool-less, with no new evidence subsystem.
- Distinct application Run and Verifier execution identities; named slots;
  explicit scheduled mapping; only submit records activate queued execution.
- Existing Scribe/outbox and runner durability, authority, replay, and reaction
  behavior are extended rather than replaced. Accepted does not mean durable,
  and completed does not mean the result has reached analytical storage.
- All three SDKs, matched examples, working TypeScript application, docsite,
  architecture, and agent-facing contracts are part of delivery.

Private helper names, module organization, executor trait signatures, caches,
worker-pool API calls, test fixture layout, and internal Arrow column layout
remain implementation choices within these contracts. A task's need to change
one is not a reason to add a new public primitive.

## Open material decisions

None. Revision 6 is approved by the owner's explicit decisions recorded
above. Sandbox feasibility and preservation of the asynchronous replay
contract through acknowledgment must be demonstrated in implementation;
if either requires weakening these requirements or changing a durable boundary,
return for a specification revision rather than accepting a hidden shortcut.

Scenario actions, nesting, grouping, human verdicts, regulated retention, and
visual rendering remain explicit non-goals, not decisions implementers must
solve for this delivery.

## Revision history and sources

| Revision | Status | Change |
| --- | --- | --- |
| 1 | approved | Owner explicitly approved before planning. Formalizes the revised proposal and five agreed simplifications on the deployment-addressing base; includes all three SDKs and matched examples, working TypeScript demo, docsite, and architecture completion. |
| 2 | approved | Owner agreed to retain batching and keep Postgres writes off the request path; replay/conflict acceptance is asynchronous and reported through existing execution status/wait. Records five-task parallel development and full integration/journey suites once all tasks are merged. |
| 3 | approved | Owner explicitly approved the concrete extraction output types and semantics ("agree. lock these"). Locks all nine extractable formats, removes redundant counts and image helpers, and requires docsite examples and matching task proof. |
| 4 | approved | Owner approved Monty startup configuration: 2 warm / 8 maximum workers, 256 MiB operation / 384 MiB process / 2 GiB worker-group memory, 10-second operations, bounded deadline-based checkout, and docsite reference. Preserves existing workspace/file/output/request limits and security requirements. |
| 5 | approved | Owner requires built-in Agent tools resolved automatically by the hosted Invoker, reusing existing tool dispatch and authorized Rust capabilities. Locks the read/compute catalog and preservation of original subject/delegation through every protected tool call. |

| 6 | approved | Owner requires two independent authorization gates, both following Wyrd's global or exact scope permission rule: tool invocation and underlying domain/resource access. Extends the existing permission representation and preserves subject/delegation and audit on both decisions. |

Material sources beyond the governing authorities:

- [Verification control-flow diagram](architecture/verification-flow.html):
  authoring, registration, entry routing, evidence upload, inline and queued
  execution, scheduling, engine/harness, identities, and storage boundaries.
- [Reference router](../../../architecture/references/README.md),
  [spec workflow](../../../architecture/references/languages/spec-driven-development.md),
  [evaluation](../../../architecture/references/domain/evaluation.md),
  [drift](../../../architecture/references/domain/drift-monitoring.md),
  [ownership patterns](../../../architecture/references/architecture/patterns.md),
  and [testing](../../../architecture/references/languages/testing-workflows.md).
- [Deployment addressing r5](../deployment-addressing/spec.md): prerequisite
  Agent/Prompt/route contracts and Workflow removal.
- [SDK construction completion](../../completed/2026/sdk-client-instantiation.md)
  and [Agent/Prompt completion](../../completed/2026/agent-prompt.md).
- [Existing SDK verification-control spec](../sdk-verification-control/spec.md):
  overlapping control projection, superseded where this approved revision
  changes it; do not run its conflicting implementation independently.
- Existing owners: [Vala executor](../../../crates/vala/vala-eval/src/executor.rs),
  [judge invoker](../../../crates/vala/vala-eval/src/orchestrator/judge.rs),
  [server verification](../../../crates/wyrd/wyrd-server/src/verification),
  [verification edge](../../../crates/wyrd/wyrd-server/src/components/verification),
  and [shared observation client](../../../crates/shared/wyrd-client/src/observe).
- [Canonical support-desk example](../../../examples/support-desk/README.md)
  and [verification docsite](../../../docs/src/content/docs/understand/how-verification-works.md).
