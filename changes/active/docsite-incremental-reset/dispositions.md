# Page dispositions

One row per baseline page (`docs/src/content/docs/`). TASK-002 owns the
onboarding rows; TASK-003 completes the rest.

| Page | Disposition | Reason | Evidence | Production state |
| --- | --- | --- | --- | --- |
| `get-started/introduction.md` | Rewrite | Taught the full Declare/Observe/Verify loop, Triggers, Operators, and the gateway before the reader had run anything | Reviewed against `architecture/wyrd-design.md` and `wyrd-doctrine.mdx`: verification, assurance, exact identities, runtime boundary | Published |
| `get-started/setup.md` | Rewrite | Mixed a 0.1.1 release with 0.2.0 docs, and carried gateway, MCP, and three-language detail the first path does not need | TASK-001 released 0.2.0 walkthroughs (Linux arm64 pass; macOS arm64 pass with the since-removed memory supply) for start, saved connection, stop, restart, persistence, and reset; pinned to 0.3.0, whose released walkthrough is pending | Published on this branch; merge waits for the 0.3.0 release and its walkthrough |
| `get-started/quickstart.md` | Replace | Required an external application checkout, provider keys, a deploy script, and default-branch source | `examples/quickstart` embedded verbatim; `test_docsite_quickstart.py` journeys (pass/fail, rerun, refusal, spec drift, missing file) on a real server | Published on this branch; merge waits for the 0.3.0 release and its walkthrough |
