# Immutable TASK-001 review subject

- Repository: /home/thorrester/Documents/GitHub/wyrd-worktrees/split/agents-trace-processor
- Branch: split/agents-trace-processor
- Base: dac4c340f835676adc29f3dcc01f98e1d3d51465 (origin/main merge-base)
- Candidate: 692d390dc40e43c6e1d7e25c7850d5c826d26a9e
- Complete cumulative diff: `git diff dac4c340f835676adc29f3dcc01f98e1d3d51465 692d390dc40e43c6e1d7e25c7850d5c826d26a9e --`
- Spec: changes/active/agents-trace-processor/spec.md (approved r2)
- Original task: changes/active/agents-trace-processor/tasks/TASK-001-framework-agnostic-telemetry.md
- Available verification: committed task's execution evidence, including follow-up integration/dependency records. Verify claims against source; do not accept implementation conclusions as proof. No prior independent verdict is present.
- Working tree was clean when pinned. Review artifacts alone may be added. Source/spec/task/candidate must stay immutable.

## Starting navigation map (expand independently)

| Changed surface | Owners / symbols | Consumers and evidence |
|---|---|---|
| Python OTEL bridge/correlation | sdks/wyrd-sdk-python/python/wyrd/otel.py: WyrdAgentsTraceProcessor, install_run_correlation, _start_telemetry, span_exporter | unit/state/test_observe_surface.py; integration/otel/frameworks/test_framework_telemetry.py and app.py |
| Python lifecycle | sdks/wyrd-sdk-python/src/state/mod.rs: start_telemetry/flush/shutdown | authored python/wyrd/state/__init__.pyi; runtime unit/integration tests |
| Shared failure projection | crates/shared/wyrd-client/src/state.rs: TelemetryFlushFailure; crates/wyrd-spec/src/error.rs | Python/TS state wrappers; native/src/cards.rs::telemetry_flush_error; generated errors; unchanged Rust consumers |
| TS bridge/lifecycle | sdks/wyrd-sdk-ts/wyrd/src/index.ts: WyrdAgentsTraceProcessor, WyrdState; src/otel.ts: runCorrelationProcessor/startTelemetry/spanExporter | tests/unit/telemetry.test.ts; tests/integration/agents-trace-processor.test.ts; support/observed-service.ts |
| Auth/privacy | Existing wyrd-client authentication and per-export access_token; existing client.http_auth/httpAuth; bridge metadata | Framework gateway/denied/foreign-tenant/content tests and local upstreams |
| Package/collection | Python Cargo.toml/pyproject.toml/uv.lock; TS manifests/lock; mise.toml; scripts/check_ts_package.mjs | optional imports, Python minimum, integration collection, SDK/Bifrost leaves |
| Docs/authority | architecture/wyrd-design.md; Observe, Connect, Quickstart, Understand pages | docs/CONTRIBUTING.md, docs/PRODUCT.md and recipe journeys |

## Review discipline

Follow .agents/skills/wyrd-task-review/SKILL.md. Read AGENTS.md, agent rules,
maintainer style, spec-driven development, router and complete relevant slices.
CodeGraph first. Inspect full diff and expand source/callers for your role.
No source edits, staging, commits, or build/test lanes without coordination
(shared target). Each reviewer owns only its assigned report. You are not alone
in the codebase; never revert another agent's artifacts. Check HEAD and tracked
source immutability before finalizing. Cite exact file:line and realistic
consequence. Record PASS/FAIL/BLOCKED, coverage and verification limits. No
optional redesigns; green-check/implementation summaries are claims to verify.
