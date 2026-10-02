# Workflow loading journey fixtures

Shared by the Rust, Python, and TypeScript Workflow loading journeys. Every
Card lives in the `workflow-loading` space and is derived from
`examples/workflows/code-review`.

These Workflows use the `native` route. Their Prompts keep the example's Chat
messages but send them to the built-in `mock` provider (`provider: {custom:
mock}`), which answers with the rendered user message. Each user message
names its Prompt, such as `registered security review of {{code}}`, so a run's
outputs show which Prompt body ran and what was bound into it. Running them
needs no network or credentials.

| Directory | What it is | What the journeys prove with it |
|---|---|---|
| `team/` | `security-reviewer` and `correctness-reviewer` Agents with their Prompts (`registered ... review`), registered first | The registered Cards that other fixtures reference |
| `team-v2/` | `security-reviewer@2.0.0`, whose Prompt answers `v2 security review` | A newer version never floats into a pinned Workflow |
| `mixed/` | Workflow `code-review`: security and correctness are registry refs, the final reviewer is a local file | Authored refs resolve through the registry; this file is applied, reloaded, and run |
| `shadowed/workflow.yaml` | Workflow `shadowed-review`: a local `security-reviewer@1.0.0` (`local security review`) plus a step referencing the registered `security-reviewer@1.0.0` | A local sibling never satisfies a registry ref with the same identity; each runs its own Prompt |
| `shadowed/local-workflow.yaml` | Workflow `local-review`: the same directory's three local Agents only | A wholly local Workflow loads and runs with no server |
| `retired/` | Workflow `retired-review`: the local security Agent references Prompt `retired-prompt@1.0.0` (`retired/retired-prompt.yaml`), which the journey registers and then deletes | A deleted registry Card is refused |
