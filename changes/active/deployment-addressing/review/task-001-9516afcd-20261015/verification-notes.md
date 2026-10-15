# Review verification limits

- `git diff --check <base> <candidate>` passed for the full pinned range.
- Source/evidence audit is independently delegated; no Rust/Python/TS behavioral tests rerun by orchestrator. Reported implementation passes remain reported evidence, not new execution.
- Attempted visual docs inspection: started existing `mise exec -- pnpm --dir docs dev --host 0.0.0.0`; computer-use browser entry returned `Browser is not available: iab`, and inventory returned no browsers/apps. No rendered visual inspection or code-copy interaction could be performed in this environment. Server stopped. Automated docs/accessibility evidence remains available in the candidate packet. This does not itself establish a layout defect.
- Initial unqualified `python` failed because the environment has a stale mise shim; system python3 used only for review artifact creation/archive, not runtime tests.
