# Immutable review subject

Repository: /home/thorrester/Documents/GitHub/wyrd-worktrees/gateway-api-key-auth
Branch: change/gateway-api-key-auth
Base: e0a926031155f263a8721c47d4662dfed8c125ea
Candidate: 5478b2605acae3cecd2522941c30ee7f1674f0be
Original task: changes/active/gateway-api-key-auth/tasks/TASK-001-gateway-api-key-auth.md
Approved specification: changes/active/gateway-api-key-auth/spec.md
Review directory: changes/active/gateway-api-key-auth/review/task-001-5478b2605-r1
Diff command: git diff e0a926031..5478b2605

## Starting navigation map

This is a starting map, not coverage or a conclusion. Independently expand and verify it.

- GatewayIngress::authenticate and GatewayCredential::classify in components/gateway/ingress.rs consume extract_gateway_access_token, select the grant or existing token verification, and feed gateway authorization/invocation through the shared ingress router.
- GatewayApiKeys in components/gateway/api_keys.rs owns token reuse; AppState::gateway_api_keys wires its process lifetime. Compare the existing verification/cache.rs::VerifierCache and client auth cache before judging new machinery.
- components/auth/routes.rs::TokenGrants::api_key is shared with /auth/token; trace ExchangeApiKey, tenant connections, issuer, verifier, and audit outbox through their existing owners. components/auth/token_extract.rs owns extraction and token verification. Check sibling MCP, management, OTLP, and Bifrost consumers.
- Gateway mod.rs exports the cache; http/openapi.rs changes served credential descriptions. No manifest changes appear in the diff.
- Rust unit proof in ingress.rs and api_keys.rs; real-server journeys in wyrd-testing/tests/gateway/native.rs, with Journey::deploy_as and put_as in harness.rs. Trace dispatch, retained audit, ledger, capture, and tenant fixtures.
- Python tests: gateway/test_gateway_openai_client.py and test_gateway_native_clients.py. TypeScript: tests/integration/gateway-inference.test.ts. Stock-client invocation and fixture setup have distinct responsibilities.
- Documentation: architecture/wyrd-design.md delegation paragraph and docs/src/content/docs/connect/call-wyrd-from-a-stock-client.md fallback section.
- Task file includes appended verification evidence and task changes in the cumulative diff. Treat execution reports and diagnoses as claims, inspect actual tests and source. Do not use the implementer summary supplied in conversation.

## Authorities and review constraints

Read .agents/skills/wyrd-task-review/SKILL.md and the requirements for your assigned role; AGENTS.md; architecture/agent-rules.md; architecture/references/README.md; architecture/references/languages/spec-driven-development.md; architecture/references/languages/maintainer-style.md; applicable design, doctrine, security and testing authorities. Use CodeGraph first because .codegraph exists. CLI: codegraph explore with concrete symbols or paths.

Review the entire cumulative diff. Production, task, and specification source remain immutable. Write only your assigned report. You are not alone in this workspace; do not revert or change other agents' files. No commits, links, or AI trailers in artifacts. Do not run broad verification or simultaneous Cargo lanes; use supplied evidence with explicit limits unless a focused check is necessary. Missing rustdoc remains a blocking rule. Optional style, speculative hardening and unrelated debt are excluded.

Sensitive domain selections: security/RBAC (credential trust, grant, permission and secret handling); tenancy (key-derived identity and tenant-isolated consumers); concurrency (shared bounded process cache, fixed deadlines and overlapping exchanges). There is no new persistent storage or durability owner; system review still traces grant commit and existing audit staging.
