# Security domain review — TASK-001

Subject: base `1d3b5ffa0d34f75fa5b22c9b68f6ec0fc8d0cc70` → candidate `ef2fe8bda2071f7b32cb6803b7f9f4599f0d4183`. Source inspected only in immutable `/tmp/wyrd-task-review-ef2fe8bd`; cumulative diff and subject supplied beside this report. No CodeGraph index exists there. No source edits, tests, builds, or other reviewers' reports were used.

Reviewed boundary: gateway permissions and delegation, open/granted deployment access, target/fallback/model/batch visibility, provider credentials and egress, Agent route overrides/callbacks, server judge subject/actor authority, and tenant identity through authentication, persistence, execution and capture.

## Authority and source coverage

| Boundary | Governing authority read | Source traced | Assessment |
|---|---|---|---|
| Review contract | AGENTS.md; agent-rules.md; reference router; spec-driven-development; maintainer-style; task-review skill; subject; approved spec r5; original TASK-001 | Full cumulative security-related diff and surrounding owners, not implementation summary | Independent static review; adjacent approved changes were not rejected for scope alone |
| Permission identity and grant semantics | Spec REQ-002, INV-001/007, AC-002; wyrd-design identity/delegation sections; security-posture principal/token/delegation sections; architecture-constraints/patterns; agent-harness | wyrd-spec/auth/permission_scope.rs; wyrd-runtime/{permission,permission_check,builtin_roles}.rs; wyrd-auth/{issuance,exchange_api_key}.rs; server auth/{principal,caller}_extractor and gateway ingress | **FAIL: SEC-001** |
| Requested, fallback and discovery decisions | Spec REQ-001/002 and INV-006/007; security-posture authorization/audit | gateway invocation.rs admit_call/route/decide/models; gateway routing.rs CallPlan; gateway routes.rs/ingress.rs | Same effective principal and snapshot used; denied requested target refuses before dispatch; fallback decisions stage separately; SEC-001 propagates here |
| Batch and file authority | Spec REQ-002/AC-002; security-posture tenant data access | gateway batches.rs upload/create/settle/retrieve/cancel/list/file/file_content/delete_file/refresh; adapter/batch.rs BatchInput; SQL gateway batch query paths | TenantConn/RLS; exact resolved address and deployment pin retained; list and object decisions share decide; SEC-001 propagates here |
| Credentials and endpoint trust | Spec REQ-003/004, INV-001/002, AC-003; security-posture credential/egress/crypto; agent-rules SSRF; patterns external network | gateway service.rs credential/deployment/snapshot/source/sealing; gateway credential.rs/snapshot.rs/managed.rs; adapter/http.rs; Skald endpoint.rs and Google client URL construction; defining gateway migration and SQL queries/rows | No additional material regression found: active/provider/assignment checks, share lock, tenant-bound sealing, redacted views, screened DNS and no redirects/proxy survive |
| Route selection, callback and local/server split | Spec REQ-009/010, INV-004/007, AC-005/007; wyrd-design Agent execution route | skald-agent gateway.rs and loop_runtime.rs; shared client observe/invoke.rs and gateway_caller.rs; server gateway caller.rs; vala-eval orchestrator/judge.rs | Gateway registry retains deployment and Prompt model independently of callback-mutated request; no hidden native fallback; explicit invocation route uses fresh Agent state; server judge refuses native routes |
| Judge execution and tenant attribution | Security-posture accepted authority/SYSTEM; spec REQ-002, INV-001/007; relevant protocol/telemetry authority | server verification authority.rs/eval.rs; caller_extractor.rs accepted/executed_by_system/attribution; gateway caller.rs/invocation.rs/capture.rs | Captured principal permissions remain authority; SYSTEM changes actor only; tenant registry/media and TenantConn ownership survive. SEC-001 also affects accepted delegated gateway authority |
| Language/CLI entry and supply chain | Spec REQ-003/011/013, INV-002/005; agent-harness | CLI gateway.rs; Rust gateway admin journey; shared gateway caller; native SDK projection; changed manifest/lockfile diffs | Per-item server decisions retained; read/output is redacted. Manifest changes reuse already-locked dependencies and remove Workflow owner; no new third-party package/version introduced in inspected lockfile diff |

## Security Audit

### Critical

None found.

### High

**SEC-001 — INCORRECT / VIOLATION: the new open/granted distinction is absent from shared permission union and intersection.**

- **Changed anchor:** `crates/shared/wyrd-runtime/src/permission.rs:886` (`contains_object_scoped`), consumed by `crates/wyrd/wyrd-server/src/components/gateway/invocation.rs:722`. The producing operations are `permission.rs:866` (`insert`) and `permission.rs:917` (`intersection`).
- **Violated obligation:** REQ-002 says All-only cannot invoke Granted and delegation retains its permission intersection; actor attribution never widens subject access. INV-007 prohibits credential/permission escalation. Security posture requires delegation only narrow authority.
- **Root cause:** invocation now assigns different authority to an All grant and an object grant, but the shared permission algebra still considers All to subsume every object scope. `intersection` takes the narrower object scope at line 921 and returns it as an actual signed grant. `insert` treats an explicit grant as redundant beside All, in either insertion order. These assumptions were compatible with the old semantics and are invalid with the new check.
- **Plausible exploit, producer to sink:** subject A holds only `gateway:invoke` scope `all`; Card-bound Service/Agent B holds an explicit deployment/model invoke grant for granted deployment D. A's direct request to D correctly returns 403. B presents A's subject token and its own actor token to the existing delegation exchange (`wyrd-auth/src/exchange_api_key.rs:271`). The exchange verifies both in the same tenant, then issuance (`issuance.rs:230`) signs `B_permissions.intersection(A_permissions)`. The shared meet converts All × D into an explicit D grant. The delegated token still names A as principal. Gateway `decide` now sees an object-scoped grant and authorizes D, making provider IO and using D's upstream credential possible on behalf of a subject who was denied directly. Both All-subject/scoped-actor and scoped-subject/All-actor directions violate the intersection boundary.
- **Sibling observable failure:** role merging (`issuance.rs:778–786`) calls `PermissionSet::insert`. Giving a workload its built-in All grant plus a custom explicit deployment/model role discards the explicit grant. A subsequently minted direct token cannot invoke that Granted deployment, although the administrator explicitly granted it. A wildcard All role has the same loss. Background accepted-authority recovery uses this same issuer. Discovery, fallback, batch listing and file/batch access share the resulting authority and do not repair it.
- **Impact:** delegated subject authorization can widen into restricted deployments, including upstream spend and batch/file operations; ordinary mixed-role grants can become unusable. Tenant RLS still confines the scenario to one tenant; this is not a cross-tenant claim.
- **Smallest safe correction boundary:** correct gateway-aware authority subsumption and meet in the existing shared Permission/PermissionSet owner, then retain existing issuer and gateway decision consumers. Preserve explicit object grants alongside All during role merge. A delegation must never manufacture an object grant when either side has only All; fail closed for a mixed intersection that the approved wire scopes cannot safely represent. Retain matching explicit intersections and All × All. Do not remove the Granted check, grant actor authority at invocation, or add independent fixes to each caller. If preserving a particular mixed-scope open-only intersection requires another wire scope, that is a specification decision rather than permission to widen authority.
- **Focused closure proof:** add pure PermissionSet tests for both insertion orders of All plus deployment/model grants and both mixed intersection directions; assert no new Granted authority. Extend the existing real token/delegation + gateway journey with subject All/actor scoped and subject scoped/actor All: direct denial must stay denied after exchange and upstream request count must stay unchanged. Include positive explicit × explicit and All × All/open cases, and a real mixed-role issuance case proving the explicit grant survives. Current `gateway_permissions_are_typed_and_scope_checked` tests each grant separately; `deployment_invocation_access_and_visibility` builds separate All/deployment/model callers and never performs delegation or mixed-role merging, so its recorded pass cannot cover this seam.

### Medium

No additional material findings.

### Low / Defense In Depth

No optional hardening proposed; this is a bounded acceptance review.

### Positive Controls

- Gateway ingress authenticates before parsing protected call metadata, derives tenant from verified principal, rejects ambiguous/URL credentials, and never forwards caller headers upstream.
- All relevant management and runtime SQL paths use TenantConn; defining credential/deployment tables enable FORCE RLS and use tenant-qualified composite credential foreign keys.
- Provider secrets remain wrapped/redacted, managed envelopes bind tenant/name/provider, operator-owned assignments check tenant/provider/exact external host, and runtime resolution refuses revoked/missing/mismatched credentials.
- Endpoint transport screens every DNS answer and connects using those answers; production scheme/port/internal-network checks, metadata blocking, TLS verification, no proxy, and redirect refusal remain intact.
- Requested/fallback authorization and administrative decisions stage audit through the existing outbox; inference does not wait for audit persistence.
- Server judges retain accepted subject authority and use SYSTEM only for attribution; native server credential escape is explicitly refused.

## Verification limits

Static review only as required. Recorded task test/check passes were treated as claims, not executed or independently reproduced. No live network, database, runtime exploitation, vulnerability-database lookup, or deployment audit was performed. The finding follows executable source control/data flow and does not depend on undocumented third-party behavior. Local untrusted mise configuration prevented initial shim navigation; subsequent read-only rg calls supplied process-local trust without modifying snapshot files. This did not block source coverage.

This report covers the security domain, not acceptance of all Prompt/media/compiler semantics or whole-repository standards. No independent finding relies on those unreviewed semantics. The approved adjacent changes were accepted as scope authority, while sensitive seams remained reviewable.

**Result: FAIL — SEC-001.**
