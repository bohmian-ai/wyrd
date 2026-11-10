# Final values and publications

Revision 18 preserves the approved authoring contract;
[execution state and durability](execution-state-and-durability.md) owns durability. Existing `output` is an action's final text or
structured JSON response. Proposed `publications` contains content deliberately
retained during the action: files, directories or JSON. Writing scratch does
not retain it. Neither name implies a report or finding schema.

## Publish contract

Only Agents have the proposed `fs.write` and `publish` tools. Standalone code
actions return JSON through `output` and cannot invoke these tools. The engine
persists extraction evidence automatically; its `publications.evidence` reference
does not mean the extractor called `publish`.

The proposed hosted Agent tool is `publish`. Supply a logical name and exactly one
source, including JSON null as a valid explicitly supplied value:

```json
{"name":"report","path":"/work/agent-chosen-directory"}
```

```json
{"name":"assessment","value":{"author_defined_field":true}}
```

`name` is one nonempty ASCII segment matching `[A-Za-z0-9_-]{1,64}`; it is not a
filename. A path identifies an existing invocation-local `/work` file or
directory. A value is arbitrary bounded JSON; a final structured response needs
neither this tool nor a file. Exactly one source is required; both/absent fails.
Tenant/execution/step/item/iteration/attempt come from the host, never arguments.

A successful tool result has `{name, kind}`, where kind is `file`, `directory`
or `json`. This is a receipt, not a storage descriptor to pass between steps.
Storage identities, integrity and provenance are read through typed execution
inspection/manifest APIs. Authors use step references and logical names; they
do not construct a `publication_id` payload or a cloud object path.

The host scopes each producer by execution_id, step_id, nullable zero-based
item_index and iteration_index, plus a one-based attempt. This is its durable
logical invocation identity and retry attribution, not an authored identifier
or required string encoding. Every attempt has separate scratch/conversation;
visible /inputs paths may be identical while /work files remain isolated.
[The sweep example](example-orchestration.md#invocation-identity-and-workspace-layout)
shows the concrete three-call layout and consolidation manifest attribution.

Names are immutable within one invocation attempt. Uniqueness is scoped to
(execution_id, step_id, item_index, iteration_index, attempt, name): a different
call in that attempt reusing a committed name fails, even if its bytes match.
A fresh Agent retry may publish the same logical name in its new attempt; both
attempts remain attributed separately, and a strict named consumer selects only
the successful admitted attempt. Retrying the same tool call with identical
arguments and its original attempt identity returns its original receipt; conflicting same-call arguments fail.
A name reservation/upload interruption recovers or is released without exposing
partial content. Fanout items and repeat iterations have separate name scopes;
retries of a failed uncommitted Agent invocation are separate recorded attempts,
whose publications remain inspectible but are not merged into a later attempt.
Collections expose accepted publications from failed attempts with their attempt
status, never merged into a later attempt or counted as another reviewer.
A named member requires the successful admitted attempt; failed partial work is
readable through the settled collection and authorized inspection.

The receipt means immutable content, its exact verified completed File association
and acceptance in the engine's run state have received the required Bifrost WAL
acknowledgement. Successors use that accepted reference directly. Recovery keeps
the same association and receipt identity without mutable scratch or inventory
rediscovery. This does not wait for object publication or Iceberg promotion.
Cancellation, expiry and stale ownership prevent later acceptance; earlier accepted
receipts remain retained under existing policy.

A publication error is visible to the Agent so it may correct a path or retry;
a report is required only by a named consumer binding or an authored check,
not by inferring intent from a Prompt. A missing named publication blocks its
consumer through existing missing-evidence behavior; a valid producer final
value can still exist. A recovered tool error alone is not a new requirement.
This is not exactly-once model execution.

## Workspace snapshot rules

Proposed `fs.write` takes `path` and UTF-8 `content`, creates safe parent
directories under `/work`, and returns written path/byte count. It refuses
`/inputs`, unsafe paths and over-limit writes. It may create or overwrite regular
files in the invocation’s private `/work`; overwrites use the same writer lock
and byte limits. Publishing exports a snapshot: later workspace writes do not
change that retained snapshot.
An Agent may use its existing bounded `code.run` tool to produce binary files
in its private workspace, then call `publish` to retain their bytes. This does
not give standalone code actions a write/publish interface or require `fs.write`
to accept binary input.
Output-only Agents can use scratch without declaring an artificial input mount.

Snapshot an existing file or whole directory consistently while excluding
writers, then upload frozen content. A directory retains nested files and empty
directories relative to the selected root. A file retains its basename metadata.
Later scratch changes cannot modify an accepted snapshot. Never publish an
unreadable/truncated best-effort walk or partly uploaded tree.

Preserve safe UTF-8 names including spaces and Unicode. Refuse absolute manifest
paths, traversal, backslashes, control characters, symlinks, special files,
file/directory collisions and names colliding under NFC plus case folding.
Do not silently rename/normalize names. Reject non-UTF-8 paths. Existing Artifact
path grammar is not changed or reused unchanged for these more general trees.

Enforce exposed effective per-file/tree/JSON size, file count, directory depth,
publication count, cumulative retained bytes, staging and deadline limits.
Verifier limits may only lower server ceilings. Limits/path errors fail the
whole publication, with no visible partial manifest. Collect orphaned bytes
only after the recovery grace period; accepted evidence roots remain protected.

## References and mounts

| Reference | Meaning |
| --- | --- |
| `steps.review.output` | Existing final action value after successful settlement. |
| `steps.review.publications.report` | Publication logically named report from a plain step, or its final successful repeat iteration. |
| `steps.review.publications` | Settled collection: producer statuses and admitted committed publications, including partial work from failed attempts/items/iterations. |

Named member selection on a repeat selects the final successful iteration,
matching its `output`; it never silently falls back to an earlier report.
Named member selection is refused on foreach (including foreach plus repeat)
because several items can use that name. Mount its whole collection instead. For a
plain action, statically known extractor members validate at declaration;
Agent-created missing names fail at binding before the consumer model runs.
A final output is never an access grant to publications; typed references are
resolved by the host under accepted authority.

For a named member mount, a file occupies the requested mount path, a directory
occupies that directory, and JSON is serialized as a UTF-8 JSON file at that
path. For example the extractor's fixed `evidence` publication is one directory.
Agent-authored publication names are logical labels, independent of actual
workspace filenames. Mounting the whole collection creates:

```text
manifest.json
content/
  <host-assigned invocation segment>/
    <publication name>/...
```

A collection manifest has these fields:

| Field | Contract |
| --- | --- |
| `status` | Existing terminal StepStatus: succeeded, failed or skipped. This is not an invented successful value for a failed action. |
| `reason` | Nullable sanitized explanation/category for failure, intentional skip or dependency/condition failure. No credentials/source payload. |
| `speculation` | none, retained, discarded or blocked. Pending is visible in live execution-read metadata, never a settled ordinary mount. |
| `invocations` | Array of producer attempt records: item_index, iteration_index, attempt, status and reason. Indices are null when the construct is absent; attempt is one-based. No calls means an empty array. |
| `entries` | Array of admitted publications with name, kind, relative mounted path and producer attribution: item_index, iteration_index and attempt. |

Each entry's `path` names a file/JSON file or preserved directory root. Host path
segments are opaque; use the manifest or fs.list rather than constructing them.
Sort invocation/entry groups by item index, iteration index and attempt; entries
within an attempt use acceptance sequence. Content stays under `content/`, so a
published file named manifest.json cannot collide with the collection manifest.

Collection mounts wait for all selected producer invocations, step requirements
and any speculative decision to settle, regardless of success. A skipped step
mounts an empty collection with skipped status/reason; a failed step includes
its accepted partial publications and failed status. Intentional skips and
failures remain distinct. A discarded/blocked speculative step exposes its
status/disposition but has no ordinary content entries; its authorized inspection view can
retain excluded bytes. Empty successful array fanout is succeeded with empty
invocations/entries, not a passed review by itself.

The collection itself is not a successful action output. Requirements decide
whether its evidence is adequate; no built-in reviewer quorum is inferred.
Output and named-publication references still require successful admitted
production and actual existence. Thus consolidators can inspect optional or
failed branches, while a final named report remains mandatory when referenced.
Freeze each collection at the consumer's start: later scratch/inspection changes
cannot alter it. There is no live subscription or publication-triggered launch.

For speculative execution, publish receipts mean durable retention, not ordinary
consumer admission. Persist disposition separately from immutable content. A
pending action result stays withheld; true condition admits it, false excludes
it and fences further commits, and invalid/unavailable decision blocks it. An
already completed action can still be discarded. Ordinary consumers wait for
this resolution, while authorized inspection can show pending/discarded content.
Recovery cannot turn an old receipt into admitted evidence or reverse a decision.

Original source paths map to validated source slots/locators. Derived files
retain producer/source lineage; a generated opinion is not an original source.
Cross-step citations must identify reachable original evidence, not scratch.

## Storage, history and inspection

The [approved execution-state supplement](execution-state-and-durability.md)
is the detailed source for state, acknowledgements, recovery and writer ownership.
Publication mechanics and naming above are unchanged.

| Existing owner | Responsibility |
| --- | --- |
| Shared File/storage | Complete immutable bytes, exact descriptors, integrity, authorized reads and existing retention. No intermediate Artifact Card or duplicate uploader. |
| Verification engine | Own the in-memory run state, accepted publications/replay identity and cumulative accounting. Resolve successors directly from accepted/recovered state. |
| Existing Bifrost shared writer and Scribe/Oracle/Forge | Persist accepted state/history and selected results; provide authoritative recovery and stale-writer protection through the approved owner extensions. |
| PostgreSQL queue | Admission, claims, lease renewal, cancellation/retry coordination and independent delivery work; no duplicate publication acceptance payload. |

Preserve existing execution_inputs/results/execution_outputs meanings and extend
canonical verification data for versioned complete logical state. Ordered event
metadata and bounded authored JSON remain queryable; event time alone does not
select accepted state. Large bytes remain in File storage. History contains no
credentials or duplicated source trees.

Exact completed descriptors returned by shared File completion remain privately
associated with accepted publications in run state. Hosted mounts and inspection
reuse existing storage read/planning/integrity at those references. There is no
PostgreSQL-to-Bifrost descriptor transfer, separate projection-intent backlog,
inventory rediscovery wait, archive handoff or publication-specific collector.
Existing upload/session lifecycle and evidence retention remain unchanged.

Both direct and queued engines use this state lifecycle. Direct remains
queue-free and disconnect does not schedule background model work. Standalone
code has no publish tool; extraction persists evidence through the engine. The
same identities, exact manifests and sanitized errors survive completion/retry.

Extend existing execution APIs across HTTP, Rust/Python/TypeScript SDKs, CLI and
MCP for paginated invocation progress, publication/manifest listing and authorized
content read/download. Stable pagination cuts do not silently mix changing
pages; an explicit subsequent poll obtains new entries. Historical searches and
authored JSON-field queries use ordinary Bifrost interfaces/table permissions.
Dedicated reads use verifier:read scoped to the exact Verifier/all, plus
repository read when applicable. Discarded content needs no separate role;
workspace admission still excludes it. Dedicated reads use fixed scoped server
queries; arbitrary Bifrost queries retain caller table grants. Exact behavior is
fixed in [public contracts](public-contracts.md).

The verification UI consumes the same public APIs: show concurrent items,
sequential iterations/attempts, partial named publications and speculative disposition, tree browsing,
provenance, failure/cancellation/exhaustion reasons, safe Markdown/text/JSON
previews and downloads, plus projection/content availability state. Refresh or failure does
not lose accepted reports. No UI-owned scheduler or private storage path.

## Required implementation ownership and reuse

Extend pure Step/reference/tool/progress contracts in `wyrd-spec`; keep IO in
the existing host/storage owners. Reuse hosted workspace confinement/locking,
File completion/inventory and existing queue authority, while adding strict
snapshots, idempotent named publication and typed mounts. Extend the existing
planner/walker/ledger for ready-step scheduling, fixed/array fanout, sequential repeat and speculative
conditional admission;
do not create another Agent loop or workflow service. Current best-effort
walkers, fresh upload sessions and in-memory ledgers do not alone satisfy this
contract. Project shared client contracts thinly into all first-class surfaces.

Amend owning architecture sections for references/citations, scratch-only
Agents, writes/publication permissions, history/capture destinations and the approved shared execution-state durability before implementation. Reconcile existing doctrine rather than
silently treating this draft as an implemented contract. Publication is bounded
execution-owned persistence, not permission to mutate GitHub. Regenerate public
schemas/SDK projections from their owners; no hand-edited generated artifacts.
