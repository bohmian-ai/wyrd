# Shared starting map (expand independently)

- Deployment contracts/CLI/credentials/tenant snapshots: wyrd-spec gateway, CLI gateway/serverdev, gateway administration; named <deployment>/<model>, credential-first single/list onboarding.
- Security: wyrd-runtime PermissionSet subsumption/intersection; wyrd-auth issuance/exchange; gateway Open/Granted authority, discovery/batch visibility and subject/actor. Approved r6 exact shared authority, reject unrepresentable exchange no token; no blanket empty-intersection denial.
- Prompt: skald-spec authoring.rs + prompt.rs native/portable compile, check_output_schema (R2 offline jsonschema0.46.4), literal typedbody/canonical model check, expectedmedia/bind/render/schema; skald-prompt builders/Python request setters; PromptSpec registration/hashes.
- Agent: skald-agent agent.rs from_card/from_yaml_path/to_spec/to_card/runconfig/runtime conversion; execution_route, loop_runtime run_prompt vs run_with, gateway registry, callbacks/tools/delegate; sharedclient observe Run/invoke and PublicWyrdGatewayCaller Vertex guard; separate server/judge caller.
- Public/native ingress and initial adapter matrix; engine admitted provider facts/capture; server GatewayInvocation actual-snapshot admission; GatewayBatches pre-LIMIT SQL eligibility and claim/replay/uncertain-dispatch state.
- SDK projections: NAPI cards hydrated_prompt/hydrated_agent and TS PromptCard variables/mediaVariables/responseSchema/Name; Rust/Python/TS four-form registerGET/hydrate JSON/hash/derived introspection.
- Installed CLI Python test_local_development driver; R2 new wyrd-client tests/integration/local_development.rs Rust Agent child, normal downloaded ref registry, actual Agent::from_card + PublicCaller/run_prompt before/after restart; saved auth/header/native trap/denial/run-correlated retained rows.
- Docs: approved developer-docsite-rebuild r2 + Connect TASK006; two Connect gateway setup/framework pages; capture-policy prerequisite, stock client/Agents SDK/PydanticAI calls and retained-row tests; indexes and viewport evidence.
- Dependency update: workspace jsonschema, skald-spec owner, workspace-hack/lock, server OpenAPI and wyrd-mcp principals schema test consumers. Verify no networking/PyO3/async leaked to pure contracts.

Read original full spec/task/prior findings and both remediations/evidence. Prior IDs1–15 retain stable source causes, owner14 keeps human-directed provenance. New confirmed findings start16. All prior closure needs source evidence. Review source/callers and meaningful tests, not completion prose/green summaries.

Owner-raised question (hypothesis, not finding/conclusion): loaded Agent to_card/to_yaml reportedly inserts absent tool_concurrency_cap Some(8), changing spec/hash. Determine actual source, complete sibling config conversions/mutations, current original approved save/load/identity obligations and whether correction belongs here versus independent debt. Do not decide scope just because R2 did not enumerate it; do not manufacture an unrelated improvement.

Evidence includes reported traced stale Python extension diagnosis rebuilt with py:setup, and Chrome rendered inspection via narrow iframe. Independently assess whether evidence/proof meets required behavior and whether changed assertions were narrowed for valid scope or hid required behavior. No retrospective RED manufacturing.
