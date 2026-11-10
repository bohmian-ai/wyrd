# Three ways to build a PR review Verifier

These are revision 16 authoring targets, not runnable on the current engine.
Each folder is self-contained: copy its contents to a repository root to place
`wyrd.toml` at that root. Its local paths then resolve without files outside the
repository. The alternatives have distinct exact Card names, so trying one does
not overwrite another's immutable definitions. When editing a registered Card,
advance its version normally.

| Example | Start here when |
| --- | --- |
| [Repeated sweeps](repeated-sweeps/verifier.yaml) | The same reviewer should make three independent passes, followed by consolidation. |
| [Conditional specialists](conditional-specialists/verifier.yaml) | Different Agents should run only for selected concerns. |
| [Speculative review](speculative-review/verifier.yaml) | An additional independent pass should start early and count only if needed. |

Each `wyrd.toml` selects its local Verifier; it contains no reviewer policy,
credentials, input mappings or scheduling limits. An administrator must first
connect/authorize GitHub and the repository. Enforced execution reads trusted
target configuration. Shared registered Verifiers
can replace these local selections using the exact CardRef contract.

Every example follows this pattern:

```text
extract pull_request -> configured reviewers -> consolidate -> final requirement
```

Every Agent reads attached files through its declared tools. No example injects
review text into Prompt placeholders. The only structured return fields used for
orchestration are the conditional planner's booleans and comparison's agree
value. Those belong to the example Prompts, not a platform finding schema.

The reviewer Prompts instruct Agents to publish their accounts with logical name
review, using their own files/directories or JSON. Consolidation checks status
manifests and actual retained material, then publishes name report. The final
requirement strictly mounts that named report. Wyrd does not infer required publication
merely from Prompt prose, or treat a successful model response as
proof that its instructions were obeyed.

No example specifies concurrency. That word means the number of calls running
at once: ready calls use available server/execution capacity, not unlimited
parallelism. Effective limits, deadlines and stored evidence references are shown
at execution acceptance. Agents retain their existing bounded internal loops.

[Orchestration and backend support](../example-orchestration.md),
[complete PR evidence](../pull-request-extraction.md) and
[publication](../output-publication.md) define the proposed contracts.
These examples illustrate different configurations of one Verifier engine;
none introduces a code-review check type or application review worker.
