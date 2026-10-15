# Shared starting map

Expand this map independently; it is not a conclusion or exhaustive coverage.

- Contracts: wyrd-spec card/agent.rs ExecutionRoute; gateway provider/deployment/model/auth; errors/generated schemas. Workflow removal across all consumers.
- Permissions/delegation: wyrd-runtime permission.rs PermissionSet normalization/intersection; wyrd-auth exchange_api_key and issuance; gateway caller access Open/Granted, discovery/batch.
- Prompt: skald-spec authoring.rs AuthoredPrompt compile/native/portable, prompt.rs binding/render/model/schema; skald-prompt builders; wyrd-cards prompt; SDK wrappers.
- Agent: skald-agent agent.rs from_card/to_spec/execution_route/run/call_once; registry/gateway_caller; wyrd-client gateway_caller.rs native projection and observe Run invoke/invoke_on; Vala judge server caller.
- Gateway: wyrd-gateway routing/engine/adapters request admission and attempts; wyrd-server components/gateway invocation, batches, capture, pg_invocation_tests; wyrd-sql batch listing scopes/snapshot.
- Round trips: SDK register_and_hydrate Rust/Python/TS four authored forms; materialized inline Prompt exact identities, JSON/hash and existing derived accessors.
- CLI journey: Python test_local_development.py::test_installed_cli_gateway_setup, TS local-development, examples/gateway-setup; installed devserver tenant creds four deployments direct requests then register/download/load/run all four Agents, restart same bundles and auth.
- Docs: approved developer-docsite-rebuild spec and TASK-006; published Connect gateway setup and framework request guides; stock Python/TS clients and Agents SDK/Pydantic AI tests; navigation/indexes.
- Verification: appended R1 evidence table in remediation file, original task evidence; source checks of selectors and strength required. User completion summary is not acceptance evidence. No tests rerun by orchestrator at start.

Reports must include prior findings 1–14 closure and docs criteria where in scope. FIND-001-14 is human-directed addition (retain provenance), original13 independently validated. Preserve stable IDs on unresolved same causes; new findings start15. No retrospective RED can be manufactured. Missing historic RED does not alone disprove current behavior; assess required workflow evidence honestly. No source edits.
