# TASK-008 Deployments addressed by name, open by default, `granted` access

Spec: `changes/active/example-journey-fixes/spec.md` r6, REQ-009 (all except
aliases, which are TASK-010) and AC-006 (except the alias case).

## Outcome

```bash
wyrd gateway credential put --file openai.json      # key for the shared account
wyrd gateway credential put --file openai-pt.json   # provisioned-throughput key
wyrd gateway deployment put --file - <<<'{"name":"openai","adapter":"openai","auth":{"bearer":{"credential":"openai"}}}'
wyrd gateway deployment put --file - <<<'{"name":"openai-pt","adapter":"openai","auth":{"bearer":{"credential":"openai-pt"}},"access":"granted"}'
```

- `openai/gpt-6-luna` and `openai/any-other-model` route to deployment
  `openai` for any principal holding a built-in role.
- `openai-pt/gpt-6-luna` routes to deployment `openai-pt` only for a principal
  whose role grants `gateway:invoke` scoped to `{deployment: openai-pt}` (or
  `{deployment: openai-pt, model: ...}`); a principal holding only `all` gets
  the existing invoke permission error.

## Scope (contract first, then every consumer)

- `crates/wyrd-spec/src/gateway/deployment.rs`: `ProviderDeployment` loses
  `model` and `routing_weight`; gains optional `models: Vec<ModelId>`
  (omitted or empty = every model), optional `capabilities` (omitted = every
  operation the adapter serves), and `access: open | granted` (default
  `open`). `ModelRef.provider` now names a deployment; rename the field or
  type only if it stays consistent across the wire, otherwise document the
  meaning. Delete the reserved-provider-identity rule in `validate`.
- `crates/wyrd-spec/src/auth/permission_scope.rs`: `GatewayAccess::Provider`
  becomes `Deployment { deployment }`; `Model` names `{deployment, model}`.
  `covers` and `Permission::covers` implement the REQ-009 table: `All` covers
  open deployments only; a `granted` deployment needs a named grant. The
  covering check needs the deployment's access, so put the decision where
  the gateway has the snapshot (the invoke authorization in
  `crates/wyrd/wyrd-server/src/components/gateway/invocation.rs` `route`/
  admission), not in a context-free `covers`.
- `crates/wyrd/wyrd-gateway/src/routing.rs`: plan resolves each candidate to
  the one deployment named by its prefix; delete `weighted_order` and its
  tests; a model outside a declared `models` list is not a candidate.
- Everything else naming `routing_weight` or the old deployment `model`
  (list from `grep -rln routing_weight crates sdks docs examples`): engine,
  credential assignment, snapshot, SQL schema for deployments (edit the
  defining migration in place, no new migration: nothing has shipped), CLI,
  `wyrd-testing` harness and fixtures, Rust/TS/Python SDK types and stubs,
  generated schemas (`mise run codegen:check`), docs.
- `crates/skald/skald-workflow/src/route.rs` `gateway_model`: use the
  Prompt's `spec.deployment` when present, else the built-in provider name.
  Add optional `deployment` to the Prompt spec in `wyrd-spec`.
  `crates/wyrd/wyrd-server/src/components/gateway/workflow.rs:222` must pick
  Vertex vs Gemini from the resolved deployment's adapter, not the prefix.
- Fallback rules, pricing, and governance limits keep their `ModelRef`
  shape; they now mean `<deployment>/<model>`. Update docs and rustdoc.
- `/v1/models` lists models of deployments that declare `models`; do not
  enumerate every-model deployments.
- `architecture/wyrd-design.md` gateway sections: update to the new routing
  and access rule.

## Evidence

- Rust unit: covering table for `all`, `{deployment}`, `{deployment,model}`
  against open and granted deployments; routing plan for prefix resolution
  and `models` filtering.
- Journeys (Rust `WyrdTestServer`, plus TS and Python gateway integration
  tests that already exist, updated): every-model deployment serves an
  unnamed model; two deployments with the same adapter and different
  credentials each reached by prefix (assert `resolved_deployment`); granted
  deployment refuses an `all`-only principal and serves a granted one; an
  uncovered fallback is dropped; a Workflow/judge call with a Prompt naming
  `spec.deployment` reaches it.
- Lanes: `mise run fmt`, `mise run lints`, `mise run codegen:check`,
  `mise run test:gateway:gate` (check `mise.toml` for the exact gateway lane
  names), `test:wyrd`, the SDK gateway integration lanes in Rust, TS and
  Python, `py:test:unit`, `py:typecheck`, `check:tenant-isolation`.
