---
id: TASK-001-R1
kind: remediation
status: ready
spec: SPEC-ambient-auth
spec_revision: 3
requirements: [REQ-002, AC-004, AC-006]
parent_task: TASK-001
remediates: [FIND-TASK-001-1, FIND-TASK-001-2]
---

# Complete the example consumer migration and reuse principal setup

Implement with `$wyrd-implement`.

## Authority and immutable review inputs

- Approved spec: `changes/active/ambient-auth/spec.md`, revision 3.
- Original task: `changes/active/ambient-auth/tasks/TASK-001-client-http-auth.md`.
- Verdict and validated diagnosis: `changes/active/ambient-auth/review/task-001-r1/verdict.md` and `findings-validation.md`.
- Wyrd base: `5bcd9e4872f1376f1722922fa63b67384243be04`.
- Reviewed Wyrd candidate: `824f0c491ea2686977c9da9fce03ac4ef65c01b1`.
- External repository: `/home/thorrester/Documents/GitHub/wyrd-example-ts`.
- External base: `bef9fc05ec1725070588bcd49efdc937b02b0131`.
- Reviewed external candidate: `b283666df93a32cb73aa8c07f881d9271797dff0`.

Read repository authorities and the original task. Preserve original acceptance; this task does not replace it. Work in both repositories as needed, record both resulting candidate identities, and preserve the immutable review reports. Correct only the two independently validated findings. Reversible implementation syntax is yours; a material behavior or boundary change requires returning to the approved specification workflow.

## Scope and owners

- External application: `src/agent.ts` owns the existing Wyrd client/OpenAI gateway; `scripts/investigate.ts` owns the existing MCP investigator.
- Wyrd TS tests: `sdks/wyrd-sdk-ts/wyrd/tests/integration/gateway-inference.test.ts` and `tests/support/server.ts`.
- Evidence: update the original task's implementation evidence with accurate external consumers, package resolution, exact focused commands and results.
- Existing SDK build/local file dependency preparation may be needed to run the example. Do not change production SDK behavior or introduce a packaging/distribution contract.

## Diagnosis and selected correction

### FIND-TASK-001-1: the committed MCP consumer was missed

AC-006 requires one application WyrdClient whose transport authenticates both OpenAI and MCP. Reviewed external `src/agent.ts:19–20` constructs that client and gives httpAuth to OpenAI, but keeps the client private. The tracked `scripts/investigate.ts:9,11,21–27` imports that gateway while still importing/calling `wyrdFetch` for `MCPServerStreamableHttp`. `package.json:12` makes the script runnable; README advertises it and tsconfig includes scripts. The reviewed SDK removes that helper, so the investigator cannot consume the reviewed package. The original task's statement that no example MCP connection exists is incorrect.

Selected correction: make the already-created application client available to this existing sibling MCP consumer and pass that client's httpAuth to its MCP transport. Remove the obsolete import/call. Keep one client construction; do not replace the stale helper with another wrapper or another client. Preserve OpenAI gateway.withOptions Run/Card headers, tracing, model settings, MCP read-only tool filter, investigation behavior, and connect/close ownership.

The local typecheck also fails TS2339 because the ignored vendor SDK lacks httpAuth. That vendor is an ignored symlink outside both immutable subjects, not stale SDK bytes committed by this change. Supply a built reviewed/current SDK through the existing `file:vendor/wyrd-ai` mechanism to prove the correction. The package must contain the executable JS, matching declarations, and normal native dependency; native declarations alone do not supply the TS getter. Record SDK candidate identity and actual package resolution. Do not commit an ignored SDK tree, hand-edit emitted files, publish a package, add a dependency, or invent a vendoring system. Correct the implementation evidence and prove the actual example against that supplied package.

### FIND-TASK-001-2: raw setup duplicates the existing typed owner

AC-004's negative behavior is implemented, but the newly added `tests/support/server.ts:113–143::revokedApiKey` duplicates principal administration: raw POST create/revoke routes, JSON serialization/casts, manual status checks and errors. Its sole caller is `gateway-inference.test.ts:225–242`, before that test installs the revoked credential in the environment.

The complete equivalent owner already exists: `Principals.connect`, `createServicePrincipal`, and `revokePrincipal` in `wyrd/src/index.ts:2243–2315`, through N-API to the shared Rust `Principals` handle. It uses the same ambient administrator, typed request/response bodies, same routes, authentication, authorization/audit, and stable error handling. `principal-roles.test.ts` provides the existing client pattern.

Selected correction: delete the one-use raw helper and its sole import; use the existing typed Principals operations in the negative journey. Retain creation of an empty-role service principal and the same service-principal revocation operation/reason. Retain the fresh client constructed after revocation, placeholder key, WyrdError cause assertion, unpinned status and unchanged-upstream assertion. Remove the support-module WyrdClient import if now unused. No replacement helper, principal API, lower-level guard, raw HTTP or new fixture is needed. This removes duplicated setup at its producer while preserving the actual outbound httpAuth test.

## Preserved behavior and non-goals

Keep all approved client transport/credential semantics, Python gateway administration, CLI output, and existing journey assertions. Do not add aliases for removed helpers, per-framework adapters, token caches, server fallback, new permissions or a different revocation lifecycle. No Rust/Python/runtime/generator/lockfile change is required by the validated findings. Do not broaden this into optional documentation/style cleanup or full integration verification.

## Acceptance and focused proof

Use existing checks one scenario at a time; record failures/results through mise. These findings require no new harness or duplicate test. The current tracked removed-helper call and failed external typecheck supply the first finding's defect evidence. The second finding is deletion/reuse work: preserve and rerun the existing negative journey rather than manufacturing a new assertion.

| Finding | Required outcome | Closure evidence |
|---|---|---|
| FIND-TASK-001-1 | One application WyrdClient supplies both OpenAI and MCP; no removed-helper application import remains; task evidence names the actual consumer. | Inspect resulting committed application source; record supplied SDK identity/resolution; example typecheck passes; existing investigator establishes its MCP path against configured Wyrd. |
| FIND-TASK-001-2 | Raw revokedApiKey setup is deleted; existing typed Principals owner performs the same setup; negative flow/assertions remain. | Diff/source shows reuse without new helper; exact existing revoked-key test runs once and passes; owning TS format/lint/typecheck pass. |

From the Wyrd repository, prepare the candidate SDK with its existing build if needed (`mise run ts:build`), supply that built package via the existing example file dependency, then run:

```sh
mise exec -- npm --prefix /home/thorrester/Documents/GitHub/wyrd-example-ts run typecheck
mise exec -- npm --prefix /home/thorrester/Documents/GitHub/wyrd-example-ts run investigate -- 'Use wyrd.guide to explain the overview.'
```

The investigator requires the existing example's configured server/provider. Do not fabricate a successful runtime result when unavailable; report the exact proof blocker. No new external test harness or full example ask suite is required for this wiring correction.

Run the exact existing negative journey with repository-managed Postgres, then only its owning static checks:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/integration/gateway-inference.test.ts -t "^revoked api key is refused by the token exchange, before the gateway$"'
mise run ts:format:check
mise run ts:lints
mise run ts:typecheck
```

Assert one selected test actually ran. Existing TS build/testing prerequisites must be available as in the original task; do not start overlapping Cargo processes. No codegen check is needed unless the actual correction changes a generated/public surface, which these findings do not require. Do not run full TS/Python/CLI/local-development journey suites or repository aggregates for this remediation; those belong to change review.

Record each finding's correction and exact focused result in the task evidence, plus both repository candidate identities and any remaining environment limitations. Return the cumulative candidate for `$wyrd-task-review`; do not mark either finding closed from intent or a source-only summary.

## Implementation evidence

Candidates: Wyrd `split/ambient-auth` (commit recorded in the return; R1 commits `cbaad4574`, `531eb1689` and this evidence), external `wyrd-example-ts` branch `ambient-auth` at `000ed1f` (on `b283666`).

| Finding | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-TASK-001-1 | `src/agent.ts` exports its one `WyrdClient` (`wyrd`); `scripts/investigate.ts` imports it and passes `wyrd.httpAuth` and `wyrd.serverUrl` to `MCPServerStreamableHttp`; the `wyrdFetch` import and call are gone. The read-only tool filter, the `connect`/`close` calls, the `gateway` model and the agent are unchanged | `mise exec -- npm --prefix …/wyrd-example-ts run typecheck`: exit 0. `npm run investigate -- 'Use wyrd.guide to explain the overview.'`: exit 0; it connected over MCP, called `wyrd.guide`, and answered through the gateway model | PASS |
| FIND-TASK-001-2 | `revokedApiKey` and its import are deleted from `tests/support/server.ts`, along with the now-unused `WyrdClient` value import (types are now `import type`). The test calls `Principals.connect().createServicePrincipal({ name: "revoked-gateway-caller", roles: [] })` and then `revokePrincipal(id, { principal_kind: "service", reason: "journey revokes this key" })`. The fresh client after revocation, the placeholder key, the `WyrdError` cause, the unpinned status and the untouched upstream are kept | exact vitest selector under `with-test-postgres.sh`: 1 passed, 11 skipped; `ts:format:check`, `ts:lints`, `ts:typecheck` pass; `git diff --check` clean | PASS |

How the example's SDK was supplied, and its limits:

- `vendor/wyrd-ai` is the ignored, existing `file:` link; `node_modules/wyrd-ai` resolves through it. It used to point at another session's `int` build, which predates `httpAuth`. Nothing was committed or published, and no emitted file was hand-edited.
- Pointed at this candidate's own `mise run ts:build` output, the example typecheck fails with only `TS2305 'WyrdAgentsTraceProcessor'` at `src/agent.ts:9`. The example also uses the unmerged `split/agents-trace-processor` change (`2347acd97`), which is not in this change's base.
- So the link now targets a composed scratch build: a detached worktree at `scratchpad/compose` with `HEAD` plus `split/agents-trace-processor`, merged and uncommitted. Its only conflict, Python's `uv.lock`, was resolved to ours and does not affect TS. It has `pnpm install --frozen-lockfile` and `tsc -p tsconfig.build.json`, plus the `wyrd.linux-x64-gnu.node` built by this candidate's `ts:build`. That branch changes no native crate.
- The investigator ran against the example's already-running `wyrd server dev --tenant acme` (127.0.0.1:8080), using its saved administrator login through `WYRD_CONFIG_HOME`. It did not use a Service key from `wyrd auth issue-key`, and it made live OpenAI calls through the gateway deployment.

Non-goals held: no production SDK, Rust, Python, generator or lockfile change; no new helper, alias, fixture or harness.
