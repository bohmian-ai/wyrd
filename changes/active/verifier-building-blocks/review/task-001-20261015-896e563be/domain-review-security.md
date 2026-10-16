# Security Audit

**Result: FAIL**

**Subject:** base `08b38f74255d344e1c5ec9a6d278935a26277622` → candidate `896e563bee69ab6b5f47b31977d7de1a0ca88d57`, approved specification r6 and original TASK-001.

## Reviewed boundary and coverage

| Boundary / obligation | Source evidence | Result |
|---|---|---|
| Global or exact tool invocation grants, validation, coverage, intersection | `auth/permission_scope.rs`; runtime `permission.rs`; TASK-001 Scenario 5 | FAIL — SEC-001 |
| Role and accepted-authority preservation | `PermissionWire`, `PermissionSet`, `Principal.effective_permissions`, `AcceptedAuthority` and its round-trip test | Existing mechanisms identified; new tool scope cannot enter them |
| Registration validation of authored names and references | `blocks/{mod,reference,validate,schema_path}.rs`; typed slot/block IDs; operator validation | FAIL — SEC-002; other inspected restrictions present |
| Agent position restrictions | `validate_agent`, `validate_placeholders`, resolved Agent/Prompt seam | Tool and workspace checks exist; unresolved references intentionally require the registration resolver |
| Media declaration and compatibility | `MediaFormat`; native-kind matching; extract-source and format validation | PASS within pure-contract scope |
| Reference resolution reuse | `refs::Visit` extension for Agent, Prompt, Artifact and drift-baseline references | PASS |
| Runtime privilege and execution boundaries | `vala-eval/src/blocks.rs`; staged exports; existing authority owners | No new runtime tool implementation or active alternate protocol |
| Request authority | `verification/execution.rs` | No caller-selected tenant/principal field; typed evidence identities |
| Dependencies and deployment | Complete cumulative diff/stat | No manifest, lockfile, SQL, deployment or secret-provider changes |

Authority consulted: supplied AGENTS.md, agent rules, reference router, Wyrd design's operation/object authorization and delegation rules, security posture, permission-model reference, spec-driven-development and maintainer references, approved r6 security requirements and original task. CodeGraph preceded independent source discovery. Inspected working source matched the pinned candidate in the reviewed production paths. No other reviewers' reports were read.

### Critical

None established.

### High

None established. The candidate stages contracts and does not expose the new registration/execution path through the active server. Findings below do not assert a currently exploitable remote endpoint.

### Medium

#### SEC-001 — MISSING: Required tool permission contracts are absent

**Locations:** `crates/wyrd-spec/src/auth/permission_scope.rs:44`; `crates/shared/wyrd-runtime/src/permission.rs:73`, `:333`, `:954`.

**Violated obligation:** r6 REQ-008 explicitly requires `{"resource":"tool","action":"invoke","scope":"all"}` and `{"resource":"tool","action":"invoke","scope":{"tool":"bifrost.query"}}`. TASK-001 assigns their pure representation, validation, coverage, intersection and authority round-trip proof to this task, including Scenario 5.

**Evidence:** `PermissionScope` contains only All, Bifrost, Gateway and Verifier. `Resource` has no Tool variant. Neither owner changes in the cumulative diff. Existing strict decoding therefore rejects both required permission forms. The candidate's private `STEP_TOOLS` string catalog contains six names; the approved catalog contains ten.

**Observable consequence:** downstream tasks cannot consume the approved independent tool-invocation authority through Wyrd's existing permission model. Roles and accepted authority cannot express or preserve an exact tool grant; global/exact intersection and disjoint tool-name scopes cannot be represented. This is a missing security contract, not evidence that existing runtime calls bypass checks.

**Minimal correction:** extend the existing permission owners with the tool resource and a typed exact built-in-name scope. Reuse existing `Permission::validate`, three-axis coverage, `PermissionSet` intersection and serde authority mechanisms. Preserve ordinary wildcard coverage and the independent domain-resource checks. Do not add a permission store or require Task-003's runtime tool harness here.

**Closure proof:** pure tests for both approved wire forms; all ten approved registered built-in names (`cards.list`, `cards.get`, `cards.get_by_ref`, `bifrost.list_tables`, `bifrost.describe_table`, `bifrost.query`, `fs.list`, `fs.read`, `fs.search`, `code.run`); unknown/prefix/path names; wrong resource/action/scope combinations; exact-name mismatch; disjoint intersections; global/exact attenuation in both orders; existing covering wildcard behavior; and preservation through role permission JSON and `AcceptedAuthority` serialization without promotion to All.

#### SEC-002 — INCORRECT: Cyclic schema references cause unbounded recursion during pure registration validation

**Location:** `crates/wyrd-spec/src/card/blocks/schema_path.rs:28`; producer/consumer path at `blocks/mod.rs`'s JSON slot and `validate.rs:663`, `:743`.

**Violated obligation:** TASK-001 requires safe pure validation of authored input schemas and references; the walker explicitly promises to treat indeterminate schemas as unknown. Security posture requires bounded processing of untrusted inputs.

**Concrete input:**

```json
{
  "inputs": {
    "data": {
      "type": "json",
      "schema": {"oneOf": [{"$ref": "#"}]}
    }
  },
  "requirements": [{
    "id": "r",
    "statement": "Check data",
    "check": {
      "kind": "assert",
      "path": "inputs.data.x",
      "operator": "is_not_null"
    }
  }]
}
```

**Precise recursion trace:** serde accepts the schema as a JSON value. `validate_slot` permits it because it is an object. The whole-Verifier assertion reaches `validate_reference`, which calls `admits(schema, schema, [Field("x")])`. The root's `oneOf` invokes `admits` on its branch without consuming the selector. `resolve` follows `"$ref":"#"` using `root.pointer("")`, returning the root schema. The same `oneOf` recursion repeats indefinitely with the same path. `MAX_REF_HOPS` bounds only consecutive `$ref` hops inside one invocation; each recursive call starts a fresh budget.

**Observable consequence:** calling the required public `VerifierSpec::validate` on this small authored value exhausts the thread stack and can abort its process. This is already a defect in the task's required pure contract surface. A remote server exploit is conditional on the later activation task wiring it into registration.

**Existing alternatives considered:** Skald's `check_schema`/`check_subschema` validate finite schema syntax but never dereference `$ref`; they do not solve selector traversal or prevent this cycle. Both JSON input schemas and resolved Agent output schemas use this new walker.

**Minimal correction:** bound the complete schema traversal, including recursive branches and local-reference resolution, at this shared walker. On a cycle or exhausted traversal budget, return the existing unknown/admitted result; preserve rejection of provably absent fields. Do not introduce network schema resolution or duplicate guards at each consumer.

**Closure proof:** focused pure tests exercising public Verifier validation with self-referential `oneOf` and mutually referential `anyOf`, demonstrating bounded completion. Retain tests for known-missing fields and ordinary local `$ref` paths. No shared process crash is needed to establish the defect.

### Low / Defense In Depth

No additional recommendations proposed outside approved task scope.

### Positive Controls

- New request bodies exclude tenant and principal authority supplied by callers.
- Digest, execution, trace, slot and block identifiers reuse typed contracts.
- Most new authoring/wire shapes refuse unknown fields.
- Check-position Agents reject tools and workspace mounts when resolved.
- Workspace keys reject empty, `.` and `..` path segments.
- Native media checks distinguish image/document bindings and reject incompatible formats.
- Regex validation reuses the existing bounded-pattern owner.
- Reference visitors extend the existing loader seam.
- Existing accepted authority retains effective permissions and delegation while dropping credential attribution.
- No new production transport, SQL, filesystem execution, arbitrary fetch or secret-handling path appears.

## Verification limits

Static review; no exploit was executed and no stack exhaustion was induced. Existing tests cover ordinary schema paths but omit cyclic combinator/reference traversal. The orchestrator reports a fresh 1069/1069 library lane; that result does not exercise the two gaps above. Runtime authorization, sandbox confinement, content sniffing, request-time schema validation and cross-tenant journeys belong to later tasks.

No production edits, full runtime security tests, or dependency vulnerability scan were performed. The staged boundary limits current exploitability; the two required contract defects remain blocking.
