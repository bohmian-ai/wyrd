# Three review configurations: diagram text alternative

The [SVG](implementation-diagram.svg) and [source](diagram-spec.json) show
revision 18 approved behavior (implementation proposed). The three middle branches are **alternative
Verifier configurations**, not three mandatory branches in one execution.
Each has a [self-contained example](proposed-examples/README.md). Exact contracts
are in [orchestration](example-orchestration.md), [extraction](pull-request-extraction.md)
and [publication](output-publication.md).

## Shared admission, evidence and delivery

A team publishes subject-less exact-version Verifiers. Under the approved
revision-24 amendment, the deployment operator configures the App once. A team
installs that existing App on GitHub, selects repositories and completes verified
connection to its Wyrd tenant with publishing/execution principals. The original
SVG illustrates the verification flow; onboarding authority is the current
[specification](spec.md) and [public contract](public-contracts.md). A Service
principal is an authentication identity, not a required Service Card.
Installation access alone confers no Wyrd authority.

A signed webhook or authorized manual request resolves the trusted target-tree
root wyrd.toml, declares selected local Cards through existing owners or resolves
registered Verifiers, then commits exact binding/configuration/PR identity and
accepted authority/budgets. PR-head configuration cannot replace the trusted
target configuration.

The pull_request extractor automatically executes the public library pipeline,
extended for pinned complete PR acquisition. It retains metadata, merge-base-to-
head diff and changed source plus unmodified supplemental context in one durable
evidence directory. Every reviewer mounts that same directory and reads it with
filesystem tools; review text is not injected through Prompt inputs.

## Alternative 1: repeated sweeps

Three independent calls to one reviewer have private scratch, invocation/attempt
identity and retained reports. Server limits bound simultaneous execution. After
all settle, the consolidator reads their collection manifest and admitted content.
Successful output order follows frozen invocation indices; failed partial work
remains visibly failed rather than fabricated successful empty output.

## Alternative 2: conditional specialists

A general reviewer and planner can run concurrently. The planner returns two
booleans. Security and concurrency reviews are distinct Agents with their own
Prompts/tools, each selected by ordinary when. False skips before binding that
branch's workspace; malformed/missing decisions record error. Consolidation
mounts all settled collections: skipped specialist, failed reviewer and successful
empty work have distinct states. Skips do not skip the consolidator.

## Alternative 3: speculative third pass

Two bootstrap Agents review independently with different sampling temperatures.
The first successful settlement makes the extra pass eligible; capacity and
ordinary required bindings still apply. Comparison waits for both settled
bootstrap collections. While comparison is pending, speculative true allows the
extra pass to execute, but its output/content stays withheld.

Agreement makes the keep condition false: skip if not started, otherwise cancel
unfinished work and exclude the contribution. Disagreement makes it true: retain
work and wait for action settlement. A failed/incomplete comparison admits
nothing. An action that finished early can still be discarded. Authorized execution-reader access
can reveal pending/discarded retained content; ordinary consumers cannot read
those bytes. The extra step and comparison must settle before consolidation.
This does not include event-triggered successors or live validation.

## Consolidation and judgment

Every alternative uses workspace-based consolidation to inspect status manifests,
discover actual report paths, verify original source and consolidate duplicates.
The consolidator publishes logical name report in its chosen file/directory/JSON
format. The final Agent requirement strictly mounts that named report plus PR
evidence. A missing required name is still missing evidence. A substantiated
introduced defect fails; insufficient evidence is inconclusive; a complete
supported clean review may pass. No Wyrd finding schema or quorum is implied.

Persist the terminal result/report manifest before exact-head GitHub delivery.
Queued/running Check Run updates and terminal conclusion derive from committed
state. Reconcile ambiguous writes and retry delivery independently; never rerun
review or regress a terminal check because delivery retried.

## Shared durability, inspection and maintenance

The engine owns one in-memory state; existing Bifrost shared-writer/WAL owners
persist complete logical checkpoints, budgets, settlements and control decisions.
PostgreSQL owns queued admission/leases/retry, not a second execution-state copy. Existing File storage retains immutable
snapshots/manifests. Recover unfinished work only and fence late/stale commits.
Ambiguous uncommitted provider calls may repeat; accepted calls/receipts and
admission decisions cannot duplicate or reverse after restart.

Bifrost owns recoverable state and explicit execution_events through existing
Scribe/Oracle/Forge; completed execution_outputs keeps its meaning. API/SDK/CLI/MCP/UI
expose active and terminal status, reasons, trees, safe previews, downloads and
provenance. Live metadata is distinct from analytical/File-inventory visibility;
show bounded wait/lag instead of false absence. Failed/cancelled work retains
accepted evidence. Excluded speculative bytes require verifier:read and applicable repository read.

Default-branch pushes refresh library snapshots; installation changes reconcile
access and stop revoked work. Offboarding quiesces builds before removal. Library
contexts keep their library-defined lifetime. No separate workflow service, client scheduler or developer cron.

## Rendering

The self-contained non-Mermaid SVG has an accessible title/description and text
labels for every state. It needs no external images/scripts/fonts. Node text is
checked against the diagram source and rendered output is visually inspected.
Runtime proof remains the spec's acceptance scenarios, not diagram parsing.

The detailed durability contract is [the approved supplement](execution-state-and-durability.md). Task 001 supplies authoritative exact loading and writer exclusion through WAL replay; the diagram does not claim those extensions already exist.
