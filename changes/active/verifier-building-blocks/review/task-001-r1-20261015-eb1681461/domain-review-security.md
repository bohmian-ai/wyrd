# Security/RBAC and contract trust-boundary review

Result: **FAIL** — one bounded contract refusal remains (SEC-R1-001, related to prior FIND-TASK-001-3). No Critical or High vulnerability found. Prior FIND-TASK-001-1, -2 and -5 are source-confirmed closed.

## Subject and authority

- Original base: `08b38f74255d344e1c5ec9a6d278935a26277622`.
- Immutable cumulative candidate: `eb1681461897ee1eb9fe1fa4e00cb0b435658559`; HEAD matched it and the reviewed production paths had no worktree diff.
- Reviewed the full original-base-to-candidate diff, using the latest remediation only as a locator. No other current discovery report was consumed.
- Authority: supplied AGENTS.md; `architecture/agent-rules.md`; `architecture/wyrd-design.md` identity/authorization/client-server boundaries; `architecture/wyrd-doctrine.mdx`; `architecture/wyrd-security-posture.md` authorization, accepted authority, audit and trust boundaries; `architecture/v1/00-foundations/permission-model.md`; reference router and its agent-harness, errors, spec-driven-development and maintainer guidance.
- Acceptance: current review's `spec-snapshot.md` (r6), original `task-snapshot.md`, previous `verdict.md`, `findings-validation.md`, and `TASK-001-R1-contract-gaps.md`. The original task and current subject explicitly defer runtime tools, engine execution, live registration/cutover, and full LLM-judge removal. No new auth service or runtime-tool implementation is demanded here.

## Boundary and source coverage

CodeGraph was used before source discovery (`PermissionScope`, `BuiltinTool`, permission coverage/intersection, gateway `invocable`, accepted authority and effective permission resolution), then expanded through direct callers and sibling consumers.

| Boundary / obligation | Source trace | Assessment |
| --- | --- | --- |
| Closed ten-name catalog | `card/agent.rs:61-139` BuiltinTool serde names, ALL, from_name; `card/blocks/validate.rs:535-561`; `verification/execution.rs:320`; `auth/permission_scope.rs:54` | One typed vocabulary feeds authored step validation, tool scope and tool provenance. All ten REQ-008 names exist; unknown names have no variant. |
| Role/wire trust boundary | `wyrd-runtime/src/permission.rs:20-67,342-372`; `wyrd-auth/src/issuance.rs:773-808` | Permission deserialization always validates; stored role JSON decodes as Vec<Permission>, corrupt role rejects issuance. Exact tool scopes require Resource::Tool and Action::Invoke. All-scope retains the existing general resource/action behavior. |
| Exact/global/wildcard and domain separation | `permission_scope.rs:153-163`; runtime `permission.rs:304-320,789-809,915-1033`; `permission_check.rs:65-76` | All covers each exact tool; exact covers only itself; tool/domain scopes do not cross. All three permission axes must cover. Existing wildcard behavior remains valid. Generic meet narrows All/exact and drops disjoint tools without a new checker. Gateway-specific conditional intersection behavior remains unchanged. |
| Delegation and accepted authority | `wyrd-auth/src/issuance.rs:218-236,773-808`; `wyrd-auth-issue/src/lib.rs:99-108,279-311`; `wyrd-auth-verify/src/lib.rs:571-584`; runtime `request_context.rs:73-110,356-383` | Existing PermissionSet remains the signed claim and accepted-principal representation; no projection turns Tool into All. Issuance uses existing exact intersection. AcceptedAuthority stores the already attenuated effective principal, removes credential ID and preserves delegation history. |
| No implicit grants | `wyrd-runtime/src/builtin_roles.rs:24-108`; runtime `permission.rs:1451-1463` | Non-admin built-in roles contain no Tool grant. Admin's existing wildcard covers it intentionally. Card/Bifrost read and Verifier run do not imply tool invoke, and tool invoke does not imply data access. |
| Sibling live gateway consumer | `wyrd-server/src/components/gateway/batches.rs:169-195` and list caller; `gateway/invocation.rs:706-763` | Batch pruning first filters resource/action and ignores Tool scope; actual invocation still checks gateway/model permission and explicit object grants for granted deployments, then stages audit. The exhaustive-match adaptation grants no gateway authority. |
| Authored schema and reference work | `card/blocks/schema_path.rs:14-242`; `validate.rs:569,692,738,775`; reference parser and known-output producers | One Walk budget covers branch recursion and local refs. Exhaustion admits unknown reference selectors but refuses unresolved Verdict fit; no external ref fetch or IO. Both JSON-slot and resolved step-output selectors use the same bounded walk. One compatible-schema refusal remains below. |
| Check position and workspace | `validate.rs:500-584,840-846`; `blocks/mod.rs` AgentCall/CodeSource; `vala-eval/src/blocks.rs:1-150` | Check Agents cannot declare tools or workspace. Step filesystem tools need workspace; virtual paths reject empty/dot/parent segments. Contracts do not open these paths, fetch Artifact bytes or run code; those receiving runtime boundaries remain later work. Native media and exact Prompt placeholders retain independent validation. |
| Evidence and caller authority | `verification/execution.rs:32-108,310-389`; `vala-eval/src/blocks.rs:43-142` | Requests have no caller-selected tenant/principal. Digests use Sha256Hex; provenance shares ToolCallRecord and carries bounded returned-output digest/count rather than complete raw output. CallInput retains original source-slot attribution; executor authority is constructed per execution. Typed declarations alone are not claimed to verify file bytes, citation existence or downstream permissions. |
| Dependency / deployment exposure | cumulative diff and manifests | No changed dependency manifest/lockfile, new route, transport, secret, environment default, executable dispatch or deployment configuration. No new SQL, shell, URL or deserialization execution sink found in this task. |

## Security Audit

### Critical

- None found.

### High

- None found. No currently exposed remote exploit is asserted for the staged contracts.

### Medium

- **SEC-R1-001 — INCORRECT, related prior FIND-TASK-001-3:** [`crates/wyrd-spec/src/card/blocks/schema_path.rs:128`](../../../../../crates/wyrd-spec/src/card/blocks/schema_path.rs) rejects a compatible check-Agent response schema because authored `allOf` processing discards sibling constraints.
  - **Violated obligation:** REQ-007's Prompt-derived Verdict compatibility, original Scenario 3, and prior finding 3's explicit boundary that compatible additional constraints do not themselves constitute incompatibility. The remediation must preserve accepted authoring semantics without imposing a new schema profile.
  - **Reachable producer and consumer:** author a normal closed Verdict response, replacing only reason's `{"type":"string","maxLength":500}` with the equivalent `{"type":"string","allOf":[{"maxLength":500}]}`. Inline and resolver-supplied check Agents both reach `validate_agent` at `validate.rs:567-580` in either check position. Step action Agents deliberately skip the Verdict guard.
  - **Source proof:** typed reason is `{"type":"string"}`. At schema_path.rs:128-131 the authored `allOf` causes an immediate return comparing only `{"maxLength":500}` to that typed schema. `types(authored)` is then empty and :149-153 returns false. The sibling `type:string`, which proves compatibility, is never considered. `validate_agent` returns `check_agent_schema`. JSON Schema siblings and allOf constraints apply together; the two authoring forms constrain exactly the same strings.
  - **Observable consequence:** a valid tool-less check Agent cannot be registered/validated merely because its otherwise-compatible reason constraint uses a conventional allOf spelling. This is a public contract acceptance defect, not privilege escalation or malformed-output acceptance. The existing positive variant only tests direct `maxLength`, so its green result does not close this case.
  - **Smallest testable correction:** retain the existing bounded structural walker and typed Verdict owner; account for constraints on the authored schema object together with combinators so an already demonstrated compatible sibling shape is not lost when a restrictive allOf is present. Do not add a generic inclusion prover, schema registry, new public profile, runtime implementation, or relax known incompatible type/enum/citation/extra-field refusals. Apply the shared correction once so both inline/resolved and both check positions receive it.
  - **Focused proof:** through public VerifierSpec validation, accept the equivalent reason schema above alongside the direct maxLength form; include a wrong sibling reason type to ensure incompatibility remains rejected. Existing canonical/local-ref positives, incompatible shapes and cyclic termination tests remain required. No provider, server or journey harness is needed. This exact new form was established by source trace; no new test/source file was written during this review.

### Low / Defense In Depth

- No additional recommendation. Runtime tool permission checks, authorized mounts, byte verification, citation validation and audit are explicitly later-task work; their absence in a pure staged contract is not a new security finding.

### Positive Controls

- Strict PermissionWire decoding and domain-tagged typed exact-name scope prevent wildcard strings, executable paths and arbitrary names from becoming tool authority.
- PermissionSet coverage/intersection is reused end to end; no parallel policy store, default tool grants or role-name bypass was added.
- Tool invocation and underlying Card/Bifrost access remain separate permissions, and no tool grant widens the live gateway path.
- The common finite schema-walk budget closes the prior combinator/ref recursion defect across both schema consumers while retaining conservative unknown-shape semantics.
- Secret-bearing credentials are absent from the new execution inputs and accepted-authority projection; tool outputs are represented by digest/count instead of complete raw output.

## Prior-finding closure

| Prior ID | Source-confirmed disposition |
| --- | --- |
| FIND-TASK-001-1 | **Closed.** Both wire forms decode through PermissionWire; exact tool name enum feeds scope equality; generic coverage/intersection narrows without widening; role decoding, token claims and accepted authority carry the same PermissionSet. Fresh permission and accepted-authority focused tests pass. |
| FIND-TASK-001-2 | **Closed.** BuiltinTool includes all ten approved names and validation uses from_name, eliminating the old incomplete private catalog. Fresh all-builtins public validation test passes, including the omitted Cards/Bifrost discovery names. Unknown/check-position/workspace refusals remain. |
| FIND-TASK-001-3 | **Partially closed; SEC-R1-001 remains.** Generated Verdict comparison replaces the former required-name-only guard, and incompatible structures/local-ref positive coverage pass. The sibling-allOf refusal above prevents complete compatibility closure. |
| FIND-TASK-001-4 | Security-relevant contract portion present: shared typed record carries optional output_sha256 and returned_bytes; no raw-output capture required. Complete handoff acceptance is owned by implementation reviewers. |
| FIND-TASK-001-5 | **Closed.** Every recursive admits/fits/values call spends the same Walk budget; resolve spends that same budget on every ref hop. Cycles stop normally; unknown selectors admit and Verdict fit refuses, rather than overflowing. Both schema consumers call the shared owner. Fresh public cyclic validation and known-missing focused tests pass. |
| FIND-TASK-001-6 | Mandatory rustdoc completeness is owned by repository/maintainer reviewers; no security closure inferred from lint alone. |

## Verification and limits

Fresh review commands, both exit 0:

```sh
mise exec -- cargo nextest run --locked -p wyrd-runtime --lib -E 'test(=permission::tests::tool_invoke_is_exact_name_scoped_and_independent) | test(=request_context::tests::accepted_authority_keeps_exact_tool_scope)'
# 2 passed; 64 unselected.
mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=card::blocks::validate::tests::verifier_spec_admits_every_builtin_step_tool) | test(=card::blocks::validate::tests::verifier_spec_requires_check_agents_to_produce_a_verdict) | test(=card::blocks::validate::tests::verifier_spec_bounds_cyclic_schema_references) | test(=card::blocks::schema_path::tests::schema_path_refuses_only_known_missing_selectors)'
# 4 passed; 865 unselected.
```

Reviewed existing proof source as supporting evidence, without treating implementation evidence claims as test logs. Did not run the full suite, cloud/DB/HTTP/provider execution, fuzzing or a live exploit; no such boundary is implemented by this task. No production or prior artifact was changed. The proposed new case is source-proven and needs its focused regression proof during remediation.
