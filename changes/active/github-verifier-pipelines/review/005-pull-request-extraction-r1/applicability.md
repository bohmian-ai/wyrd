# TASK-005 review r1 — scope and specialist applicability

Subject: base `b262696af` (parent of the first TASK-005 commit `38aecdb1a`),
candidate `79a7f985e`. Range: `git diff b262696af...79a7f985e`.
Spec: `changes/active/github-verifier-pipelines/spec.md` revision 8 (approved).
Task: `changes/active/github-verifier-pipelines/tasks/005-pull-request-extraction.md`.

Navigation map (starting point only):
- Authored field: `wyrd-spec/src/card/verifier/{mod,pull_request,validate}.rs`; TS `card-types.ts`.
- Registration `queue_only`: `wyrd-server/src/components/cards/service.rs`; direct refusal `components/verification/service.rs`.
- Extractor: `wyrd-server/src/components/github/extraction.rs`, `extraction/output.rs`; engine route `verification/hosted/blocks.rs`, `vala-verify/src/blocks/engine.rs`; completion flag `verification/{engines,runner}.rs`.
- Baseline and completion SQL: `wyrd-sql/src/queries/verifier_executions{.rs,/pull_requests.rs}`; migration `20261115000000_pull_request_source_extracted.sql`.
- Metadata batching: `components/github/review_cache.rs`.
- Tests: `pg_github_pull_request_extraction.rs`, `pg_verifier_executions.rs`, `pg_code_review_cache.rs`, `pg_card_declaration_route.rs`; mock `wyrd-testing/src/github.rs`.
- Docs/authority: `architecture/wyrd-design.md`, `docs/.../verifier-card.md`, `declare-a-repository.md`.

| Contract / invariant | Owners and consumers | Role | Decision |
|---|---|---|---|
| Extraction behavior, output, fallbacks, consumption, recovery, honest failure, surfaces (REQ-006/007/016, AC-004/011) | extractor, engine, Agent bindings, HTTP/MCP details | behavior-rev | selected (first review) |
| Persistent state: new column, replaced SQL function, baseline query, RLS tenancy, lease-fenced completion (INV-001, INV-007) | wyrd-sql, runner completion, migration | domain-rev (data layer) | selected: persistent-data and tenancy boundary |
| Shared lifecycle state | baseline flag written at completion | invariant-rev | omitted: one producer (runner completion) and one consumer (baseline query); covered by behavior-rev and the data domain review |
| Cross-tier repository rules | spec/server/sql/sdk-ts/docs | repo-rev | omitted: behavior-rev covers applicable AGENTS rules; dependency and tenant gates ran green |
| Ownership/abstractions | extends existing owners (github component, review_cache, queue) | maintainer-rev | omitted: no new tier or owner; review_cache batching extends task 004's owner |
| Failure propagation/recovery | recovery reuses existing REQ-017 checkpoint path | system-rev | omitted: no new recovery mechanism; extraction output is ordinary step output |
