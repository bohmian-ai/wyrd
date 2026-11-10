# PR review worked examples

## Current approved report flow

[Standard report](standard-report/README.md) is the approved end-to-end design
example: inferred TOML declaration, extraction supplied to both Prompt inputs
and workspace, one standard report step output, authored verdict and GitHub
consumption without a selector. It requires future engine/schema support and
has not been run against the server.

## Orchestration design examples

These examples explore three review strategies. Their TOML roots use the
agreed inferred Repository Card authoring shape.
The YAML pipelines are preserved as design references; they have not been
validated as runnable or reconciled with the final extraction/wait contracts.
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
connect/authorize GitHub and the repository. Run `wyrd declare wyrd.toml` to resolve and register the inferred Repository
Card, local Verifier and dependencies and derive its bindings. PR admission
uses the selected registered Repository version, not a live fetch of PR-head
configuration. Exact registered Verifier references can replace local paths.

The three orchestration examples follow this pattern:

```text
extract pull_request -> configured reviewers -> consolidate -> final requirement
```

In these orchestration examples, Agents read attached files through declared tools;
they do not inject review text into Prompt placeholders. The current standard
report example above demonstrates both consumption paths. The only structured return fields used for
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
parallelism. Existing process/execution bounds govern runtime work. These examples do not
select acceptance-time limits reporting or new capacity settings. Agents retain their existing bounded internal loops.

See the [Repository Card contract](../repository-card-contract.md),
[extraction contract](../approved-code-review-support.md#approved-pull_request-extraction-contract),
[report contract](../approved-report-delivery.md) and [design notes](../notes.md).
These examples illustrate configurations of one Verifier engine; their YAML has
not been reconciled with every current contract or validated as runnable.
