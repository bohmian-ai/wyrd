---
id: TASK-001
kind: remediation
status: ready
parent_change: skald-workflow-runtime
base: ac19e6bbe9c975a1e2f64b22cc80a0b66093098e
---

# Close skald-workflow-runtime review follow-ups

Implementation skill: `$wyrd-implement`.

The `skald-workflow-runtime` change is complete
(`changes/completed/2026/skald-workflow-runtime.md`). Its final reviews left
twelve non-critical findings. The human directed that every one is addressed.
The full finding text, evidence, and correction boundaries are in Git history
at `395a3ad91` (read with `git show 395a3ad91:<path>`):

- `changes/active/skald-workflow-runtime/review/TASK-004-r6/findings-validation.md`
- `changes/active/skald-workflow-runtime/review/TASK-005-r1/findings-validation.md`
- `changes/active/skald-workflow-runtime/review/change-r1/review.md` (at `079794988`)

## Human decision

`wyrd workflow` commands take no `--server` option. The Wyrd endpoint is
inferred from the environment/configuration only, exactly as the existing
ambient client resolution does. The global CLI `--server` flag used by other
commands is unchanged.

## Required corrections

| # | Finding | Correction |
|---|---|---|
| 1 | `FIND-TASK-004-25` cache key ignores the Prompt destination | If `CacheKey::from_request` has no caller outside its own tests, delete it and its tests. Otherwise derive the key from the effective `Prompt::provider()`. Do not keep a request-only variant. |
| 2 | `FIND-TASK-004-24` unsupported-media error names Google for Vertex | Use `Prompt::provider()` at the existing media-error owner. |
| 3 | `FIND-TASK-004-17` task authority names Revision 13 | Closed by completion: the active task files were removed. Record that in the completed record; no other change. |
| 4 | `FIND-TASK-004-26` two rustdoc blocks conflate schema default and destination | Correct the two rustdoc blocks. |
| 5 | `FIND-TASK-004-27` four Prompt construction items lack `# Errors` | Add substantive rustdoc with `# Errors`. |
| 6 | `FIND-TASK-005-1` `--file` refuses `--server` | Apply the human decision: delete `--server` from every `wyrd workflow` subcommand and its parser refusal; the endpoint comes from the ambient client. Update the CLI tests and docs that used it. |
| 7 | `FIND-TASK-005-2` input file read before mode refusal | Validate mode compatibility before reading `--input-file`. |
| 8 | `FIND-TASK-005-3` function-local `PermissionsExt` imports | Move them to the module import blocks. |
| 9 | `FIND-TASK-005-4` Ctrl-C listener failure treated as interruption | Handle the `ctrl_c()` result in the existing select arm; project failure through the existing typed error. |
| 10 | `CHANGE-R1-DRIFT-001` external-gateway success-response credential scan | Delete the substring scan in `skald-providers/src/clients/external.rs` and its tests. Keep the standard secret handling and error/log redaction. Add nothing in its place. |
| 11 | `CHANGE-R1-INCORRECT-001` TypeScript example uses `new Cards()` | Change the example to `Cards.connect()`. |
| 12 | `CHANGE-R1-MISSING-001` OpenAPI test does not assert Workflow routes | Extend `crates/wyrd/wyrd-server/tests/pg_openapi_contract.rs` to assert the `/v1/workflow-runs*` paths in the served document. |

Finally, update the "Approved revisions and deviations" section of
`changes/completed/2026/skald-workflow-runtime.md` so it no longer lists the
closed deviations, and records the `wyrd workflow` no-`--server` decision.

## Verification

Run the narrowest lanes that cover the write set, plus `mise run fmt`,
`mise run lints`, `git diff --check`, and every focused test you change or add
by its exact command. Expected owners: `test:skald`, the three compiled CLI
workflow journeys, `test:principals:integration` for OpenAPI, `docs:check` if
docs change, and the TypeScript lane if the example is type-checked.

## Evidence

Append the acceptance table below.

| # | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
