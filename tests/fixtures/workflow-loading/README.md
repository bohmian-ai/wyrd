# Workflow loading journey fixtures

Shared by the Rust, Python, and TypeScript Workflow loading journeys. Every
Card lives in the `workflow-loading` space and is derived from
`examples/workflows/code-review`.

| Directory | What it is | What the journeys prove with it |
|---|---|---|
| `team/` | `security-reviewer` and `correctness-reviewer` Agents with their Prompts, registered first | The registered Cards that other fixtures reference |
| `team-v2/` | `security-reviewer@2.0.0`, whose Prompt says "You are a v2 auditor." | A newer version never floats into a pinned Workflow |
| `mixed/` | Workflow `code-review`: security and correctness are registry refs, the final reviewer is a local file | Authored refs resolve through the registry; this file is applied and reloaded |
| `shadowed/` | Workflow `shadowed-review`: a local `security-reviewer@1.0.0` (Prompt says "You are a local auditor.") plus a step referencing the registered `security-reviewer@1.0.0` | A local sibling never satisfies a registry ref with the same identity |
| `retired/` | Workflow `retired-review`: the local security Agent references Prompt `retired-prompt@1.0.0`, which the journey registers and then deletes | A deleted registry Card is refused |
