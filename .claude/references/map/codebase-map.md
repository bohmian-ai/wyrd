# Codebase Map

Use source repos to distinguish real reuse opportunities from speculative
design. Search narrowly with `rg` before recommending a rewrite.

## Repos

| Repo | Inspect for |
|---|---|
| `/Users/stevenforrester/Documents/GitHub/opsml` | Registry, card lifecycle, local save/load, storage, auth, CLI, UI, MCP, SQL, and PyO3 patterns. |
| `/Users/stevenforrester/Documents/GitHub/scouter` | Drift, eval, observations, ingest, alerts, workers, SQL control plane, DataFusion/Delta, and OLAP patterns. |
| `/Users/stevenforrester/Documents/GitHub/potatohead` | Providers, prompts, agents, workflows, memory, callbacks, mock LLM testing, and runtime patterns. |
| `/Users/stevenforrester/Documents/GitHub/wyrd` | Current implementation repo. Read its `AGENTS.md` before specifying code behavior. |

## Existing Agent Guidance

Read these before detailed source work:

- `/Users/stevenforrester/Documents/GitHub/opsml/AGENTS.md`
- `/Users/stevenforrester/Documents/GitHub/scouter/AGENTS.md`
- `/Users/stevenforrester/Documents/GitHub/potatohead/AGENTS.md`
- `/Users/stevenforrester/Documents/GitHub/wyrd/AGENTS.md`

## Common Evidence Searches

- OpsML cards and registry: `rg "struct .*Card|CardRegistry|Registry" crates py-opsml`
- OpsML local materialization and install/load patterns: `rg "save\\(|load\\(|install|lock" crates py-opsml`
- OpsML MCP: `rg "mcp|list_docs|read_doc|list_cards" crates`
- Scouter drift/eval: `rg "Psi|Spc|Drift|Eval|LLMJudge|AssertionTask" crates py-scouter`
- Scouter OTel/DataFusion/Delta: `rg "DataFusion|Delta|TraceSpan|span|trace" crates`
- Potatohead providers: `rg "GenAiClient|Provider|OpenAI|Anthropic|Gemini|Vertex" crates py-potato`
- Potatohead agents/workflows: `rg "Agent|Workflow|Task|Memory|Callback" crates py-potato`

Source evidence can justify reuse or adaptation. It must not override current
Wyrd doctrine, locked architecture, or Wyrd-native public vocabulary.
