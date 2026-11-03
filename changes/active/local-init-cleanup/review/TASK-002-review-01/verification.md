# TASK-002 review verification

Immutable source candidate: `729255588540db8a46dba9f5d090bbfc9e59d2bb`.
Base: `5aaa38937d76fef1a690cd931e3001363be69ddc`.

## Independent reruns

```sh
mise exec -- pnpm --dir docs exec vitest run src/lib/derived-nav.test.ts src/lib/content.test.ts src/lib/indexes.test.ts src/routes/shell-links.test.ts src/lib/readme.test.ts
```

Exit 0: five files, 62 tests passed, 4.62 s. Node emitted a nonfatal module
registration deprecation warning. No source changed.

```sh
mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=mcp::tests::setup_prompt_names_its_release_and_holds_no_secret)'
```

Exit 0: one selected test passed, 758 skipped. Run
`29b1249a-9a21-49da-9f19-e6b4581dae2c`.

```sh
mise run docs:linkcheck
mise exec -- git diff --check 5aaa38937...729255588
```

Completed without errors. These establish structural/local-link and whitespace
proof; they do not establish public workflow execution or reader comprehension.

## Supplied evidence not independently rerun

Candidate TASK-002 records docs:check (100 tests, generation/link/build/a11y),
the MCP discovery and uploaded-Artifact SDK journeys, and eight fresh-reader
exercises. Inspect the actual record and assertions before using these claims.
The candidate explicitly labels MCP reader entry as source-material simulation
and its Artifact journey as scripted-model proof, without a public workflow
execution against the documented release or a vendor-source refresh.

Review does not edit implementation, generate committed docs, run real
providers, reproduce benchmark work, or rerun broad suites without a specific
proof gap justifying that action. Spec-revision-2 approval confirmation is
pending separately; no approval is inferred from draft metadata.
