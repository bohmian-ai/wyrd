# Immutable cumulative re-review subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/sdk-client-instantiation`.
- Original base: `2f1db2194f5630df850b10987fe0f360946a525c`.
- Prior candidate: `fde7d396a3917964dc344ede2ed09046e179aa66`.
- Current candidate: `07419b4d060303959384b682ca88e8673c722e63`.
- Complete acceptance diff: `git diff 2f1db2194f5630df850b10987fe0f360946a525c 07419b4d060303959384b682ca88e8673c722e63`.
- Latest remediation diff for navigation/closure: `git diff fde7d396a3917964dc344ede2ed09046e179aa66 07419b4d060303959384b682ca88e8673c722e63`.
- Approved specification: `changes/active/sdk-client-instantiation/spec.md`, revision 11, status approved. The current user explicitly identifies revisions 10/11 as the remediation authority: tenant saved-only coverage, existing Platform environment/no-credential coverage, and injected Rust Environment through the single shared resolver. Do not resurrect superseded process-environment/runner/credential-source requirements.
- Original task: `git show 2f1db2194f5630df850b10987fe0f360946a525c:changes/active/sdk-client-instantiation/tasks/TASK-001-unify-sdk-handle-instantiation.md`, applied under current approved spec revisions. Candidate task appendices contain implementation/remediation evidence claims, not acceptance conclusions.
- Prior verdict/validated ledger: `changes/active/sdk-client-instantiation/review/task-001-fde7d396a/{verdict,findings-validation}.md`; also read its relevant discovery reports for prior source IDs and rejected-scope boundaries. Preserve FIND-TASK-001-1 through -6; new validated findings start at -7.
- No remediation task was issued by the prior SPEC_REVISION_REQUIRED verdict, and none exists. Remediation inputs are that validated ledger, original task, later spec decisions and direct user instructions, with evidence appended to TASK-001. Do not treat an artifact never issued as missing implementation behavior or recreate history.
- TASK-002 owns authored docsite/README guidance and generated docsite outputs; its pending prose is excluded from TASK-001 acceptance.
- Starting tracked tree clean; HEAD equals candidate. Only new assigned review reports may be written; source, prior reports, spec, task, manifests and generated artifacts must remain immutable. Recheck candidate/source before final report.

## Shared navigation map (starting points, not coverage limits)

| Surface | Cumulative owners | Latest closure/navigation starting points |
|---|---|---|
| Shared client construction | `crates/shared/wyrd-client/src/{client,config,environment}.rs`; Cards/Gateway/Principals/OperatorConnections/Platform handles; Bifrost async/blocking/options/query/table owners | `WyrdClient::{from_global,from_environment,connect_ambient,connected}`, removed duplicate `ClientConfig::from_global`; CLI pg config consumer; single resolver and approved proof boundary |
| Python lazy local boundary | `sdks/wyrd-sdk-python/src/{client,state/mod,bifrost/mod}.rs`; gateway/principal/operator lazy consumers; `python/wyrd/bifrost/_client.py` | `LazyConnection`, new shared Cards owner and four typed view consumers; native Bifrost `Binding`, local getters/use_table, initial connection/publication and retained native client; mutation/read/concurrent first-use boundaries |
| Python public API and runtime proof | Public auth rename, authored Cards/auth/Gateway/operators stubs, Bifrost inline annotations/export; new construction journeys/unit tests | Local-state unit/integration tests; eager AsyncBifrost ambient success; delegated query and retained audit attribution; first-use timing docs |
| Rust public projection and journeys | `sdks/wyrd-sdk-rust/src/lib.rs`, all cumulative SDK consumer migrations, integration/support | Instantiation journey's new resolver use and actual shared factories; delegated audit evidence; saved_user_auth revoked eager-refusal assertion; helper rustdoc/Result changes |
| TypeScript/N-API | Shared native client/Card/Gateway/principal/operator factories, public index.ts, generated declarations and migrated consumers | Instantiation delegated audit attribution proof; original eager Promise/listing parity remains within cumulative scope |
| Server/CLI/MCP/testing/example consumer closure | Cumulative CLI/server setup/MCP/testing/Bifrost consumer migrations and executable examples | Latest changed CLI card/workflow helpers and pg tests, MCP connectivity/investigation helpers, Rust request example, SDK helper docs/errors and stale state links |
| Contract/security references | Client model, Bifrost public surface, Rust/PyO3/Python/TS/errors/testing/maintainer references and repository authorities | Spec11 AC-002/004/007 qualification; Platform stays separate and Rust-only; no new auth source is requested |

Run `git diff --name-only BASE CANDIDATE` to expand the complete map; prior review documents are themselves now tracked in this range and are review evidence, not production implementation.

## Prior findings to reassess with current source

1. Local Python getters/table changes/Cards view selection must not initialize transport; actual first server-needed operations may initialize.
2. Ambient journey proof is governed by spec11's explicitly approved injected Environment/single-resolver approach; eager Python AsyncBifrost real-server path remains required. Check that proof uses the actual one resolver rather than a duplicate.
3. Each language must compare server-observed delegated subject attribution, retaining useful permission checks.
4. Mandatory changed-item rustdoc, failure/panic contracts and stale constructor links must close; batch any remaining material documentation findings.
5. Revoked saved login fails at eager connection, with its stable error and real revocation/expiry setup retained.
6. Spec10 resolves Platform saved-only applicability without changing production authentication sources.

## Available verification evidence and execution limits

Candidate TASK-001's Remediation evidence appendix records exact/grouped checks and limits: Rust six instantiation journeys, shared-client library, four CLI pg cases, Python local/instantiation unit+integration, TS six instantiation journeys, managed Keycloak/Dex revoked-login proof, Rust/Python/TS format/lint/type and broken-link docs. Earlier appendix records initial broad lanes. No separate raw transcript was supplied. Treat pass labels as claims; inspect actual asserted outcomes and selectors. Do not use the implementation completion narrative as proof.

Static review by default: no test/build/Cargo/generator/formatter or runtime child-process execution by reviewers. Request coordinated focused verification only if needed to resolve a material uncertainty. No baseline runs, source fixes or new navigation/checker scripts. Use CodeGraph before code lookup when .codegraph exists. Write only your assigned report; other agents share the checkout.

Read `.agents/skills/wyrd-task-review/SKILL.md` for exact role/report requirements. Discovery reviewers must not read other current-round reports or receive an intended verdict. Both implementation reviewers cover the full acceptance matrix, using latest changes to locate prior closure and expanding across the cumulative candidate. All roles distinguish real approved behavior from optional improvements or already-rejected queue/auth/harness redesign.
