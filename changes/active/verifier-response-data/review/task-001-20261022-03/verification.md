# Verification evidence and limits

Subject: `fb1fd9234acf9ff20b1feff89bf9f3da9cba4f14...d2ffc59873b5af688adf1d1e5b2a3a388160bb83`.

Orchestrator checks:

- `mise exec -- git diff --check fb1fd9234...d2ffc5987`: PASS, no output.
- `mise exec -- cargo tree --locked --offline -i schemars@1.2.1 --depth 1`: PASS; active workspace dependents resolve schemars 1.2.1.
- Initial HEAD: candidate; only the three user workflow edits listed in subject.md were dirty before creating this review directory.

Recorded implementation proof is in the original task and R1 task Implementation evidence sections. It includes focused Rust/Python/TypeScript SDK stories, declaration/runtime/schema regressions, OpenAPI/MCP consumers, unit lanes, format/lints/typechecks, generation/dependency checks, and docs checks/build. These are recorded results; this review does not claim to have rerun them all. Independent reviewer reproduction, if any, is recorded in each report.

No broad gate, full journey suite, live provider call, crash injection, or deployment was run by the orchestrator. In particular, live OpenAI strict structured output with embedded schema resource IDs remains an implementation-evidence limit. Historical missing RED in original scenarios 1–3 remains disclosed.

Reviewers must coordinate Cargo-backed proof sequentially. Reports only may be added; source, generated artifacts and user edits remain untouched.

Final immutability check: HEAD remained `d2ffc59873b5af688adf1d1e5b2a3a388160bb83`;
tracked working-tree changes remained exactly the three excluded user workflow
files. New files are confined to this review directory. The final validation
report records independent Rust-validator, typechecker and byte-count probes;
no passing full-runtime rerun is inferred from those probes.
