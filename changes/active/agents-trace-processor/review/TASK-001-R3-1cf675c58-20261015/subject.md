# Immutable TASK-001 / R3 cumulative re-review

Repository: /home/thorrester/Documents/GitHub/wyrd-worktrees/split/agents-trace-processor

- Original base: dac4c340f835676adc29f3dcc01f98e1d3d51465.
- Previous candidate: d088519ea846aba8b2a77c24b2af2491ad2b3936.
- Current candidate: 1cf675c58d01cba0e714102c75cb7d89e7a678b5.
- Full cumulative diff: `git diff dac4c340f835676adc29f3dcc01f98e1d3d51465 1cf675c58d01cba0e714102c75cb7d89e7a678b5 --`.
- Fix navigation only: `git diff d088519ea 1cf675c58 --`.
- Authorized main merge: 1b769759dfd92a82e60082fe80460166bdd860f8, importing cb29781ab6f633700f9e47befddb1f325fd8b732.
- Supplemental task-vs-imported-main contrast: `git diff cb29781ab6f633700f9e47befddb1f325fd8b732 1cf675c58 --`; never replace original base.
- Approved spec: changes/active/agents-trace-processor/spec.md r2.
- Original task: changes/active/agents-trace-processor/tasks/TASK-001-framework-agnostic-telemetry.md.
- Historical original review: ../TASK-001-r2-692d390dc-20261015/{verdict.md,findings-validation.md,TASK-001-R1-telemetry-contract-closure.md}.
- R1 review: ../TASK-001-R1-295446005-20261015/{verdict.md,findings-validation.md,TASK-001-R2-python-overlap-and-docs.md}.
- R2 review: ../TASK-001-R2-d088519ea-20261015/{verdict.md,findings-validation.md,followup-review.md,TASK-001-R3-otlp-publisher-and-proof.md}. R3 includes committed implementation evidence.

Prior stable findings 1–12 are hypotheses: 1 optional TS declaration edge; 2 complete executable docs/prerequisites; 3 live managed flush/continuation proof; 4 safe Python Responses usage; 5 effective Python timeout; 6 concurrent TS setup; 7 Python later caller obsolete flush success; 8 Python same-state setup race; 9 implicit OTLP publisher actor identity; 10 Rust Panics docs; 11 Python exact-code credential proof; 12 obsolete Rust OTLP publisher readback. Prior 1–8 closed in previous independent review; validate closure with current source. IDs9–12 are latest remediation targets. New stable IDs start13 only for genuinely distinct validated causes.

## Starting navigation map (expand independently)

- R3 Gate: crates/vala/vala-bifrost-redux/src/gate/mod.rs::otlp_batch_id, dispatch_canonical, existing otlp_batch_id_tests::derived_identity_is_stable_per_tenant_table_and_payload. Existing auth.attribution() now passed to identity and Scribe frame; inspect full producer/consumers, all trace/log/metric and HTTP/gRPC siblings, persisted replay fence identity and old/new behavior where reachable. No intended conclusion about compatibility follows from this map.
- R3 real proof: crates/wyrd/wyrd-testing/tests/bifrost/otlp/negative.rs::identical_exports_from_two_principals_each_keep_their_own_attribution. Pair/run extraction/grouping and two delegated actors across transports, expected logical row sets/replay; support.rs public query/client/service/token/delegation helpers and all seven sibling files.
- R3 Python proof: sdks/wyrd-sdk-python/tests/integration/otel/frameworks/app.py module boundary catches public WyrdError code and reraises; parent test_framework_telemetry.py ambient gateway negative exact JSON code, positive gateway/identity/payload/upstream-key flow retained.
- R3 Rust docs: shared wyrd-client/src/state.rs settlement regression Panics section; shared TelemetryFlushFailure behavior untouched but cumulative acceptance remains.
- Python full telemetry: python/wyrd/otel.py bridge/callback map, _gen_ai, timestamps, stock exporter current-token session, correlation, _ManagedTelemetry/_start_telemetry. Native src/state/mod.rs OnceLock/telemetry_setup/start/GC/flush/shutdown and shared native Bifrost drain/error settlement. test_observe_surface.py setup/overlap/retry/provider preservation/deadline/old-attempt barrier/callback/thread/owned/freshness.
- TS full telemetry: wyrd/src/index.ts bridge structural optional types, postawait setup, provider lifecycle/error settlement/Run scope; src/otel.ts stock exporters/correlation; native cards.rs projection; clean package checker/generated declarations/optional peers. telemetry.test.ts and actual framework journey.
- Real framework coverage: Python framework app + server/query test matrix for actual Pydantic AI/ADK/OpenAI and TS agents-trace-processor.test.ts with observed-service.ts; managed/owned/live continuation, errors/stream/nested/concurrent/uncorrelated, sampling/privacy/denied/cross-tenant/gateway/missing credentials.
- Server trusted publisher: public on_behalf_of exchange, AuthContext::attribution/verified chain, canonical pre-stamp logical/correlation digest, Scribe stamps/logical exclusion/WAL/memory/durable fences; native explicit batch IDs and server outbox identity are sibling owners whose semantics must be preserved. Test actors must have genuine subject/actor/scope/permission/audience intersection; no mocked false proof.
- Docs/packages/selection: Observe page complete install/build/readback/provider/failure recipes, Connect/Understand/Quickstart/lasting design; docs conventions/draft. Python floor/dev/abi3 and TS optional dependencies/checker; mise integration/Bifrost owner selection. Ambient selected credential/current token and stock http_auth/httpAuth remain shared owners.
- Imported main authorized: subject/actor schema and attribution across clients/server/Gate/Scribe/query/fixtures. Audit task-facing seams and preservation, not wholesale unrelated-main certification; do not mark authorized merge as unsolicited drift.

## Required review discipline

Read wyrd-task-review skill, AGENTS.md, agent rules, spec-driven development, maintainer style, reference router and selected complete authorities. CodeGraph first where indexed. Review entire original task against stronger authority, 21 obligation IDs plus constraints/non-goals, not latest patch alone or implementation summary. Source/verification evidence governs; no optional redesign/speculative guarantees/style-only round. Trace shared causes and sibling owners fully, fix a root cause once. No approval for new material guarantees is inferred from implementation evidence.

Separate fresh discovery roles: behavior, invariant, standards, maintainer, system; domain concurrency, security/tenancy, OTEL and durability. Own only assigned report here, preserve others. Do not read current peer discovery reports or implementation chat summary. Previous review packets are shared historical inputs, not intended current verdict. Root owns fresh tests. No source/spec/task/index/HEAD edits, staging, commits, builds/tests/installs by agents. Tiny isolated existing-dependency probes only with coordination and cleanup. No concurrent Cargo. HEAD was exact and tracked/index/worktree clean before creating this directory. Candidate change or missing mandatory source/report blocks review. Current verification.md will distinguish fresh execution from committed historical records.
