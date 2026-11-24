# TASK-006 domain review

Subject: base `e6d7d2681e588f118ec7a8e9bbcfab24757a654b`, candidate `bfbdf016a5487cac17e8dc6fbb73cc8ae17a147f`, branch `task/verification-workflow-ui/TASK-005`. HEAD and tracked source remained at candidate. First review, not remediation.

Scope: exact approval identity, current authorization and credential boundaries, tenant separation, browser session/Origin forwarding, safe problem projection, and durable decision/replay integration. Governing sources: root AGENTS.md; SQL AGENTS.md for the inspected query capability; changes/spec-driven-development.md; agent_docs/maintainer-style.md; TESTING.md; wyrd-ui skill; approved verification-workflow-ui spec revision 8 REQ-008/014 and AC-008; approved authorization-model revision 9 REQ-004/005/006; architecture/wyrd-design.md:1794–1811 and architecture/wyrd-security-posture.md:195–203, 697–714. No .codegraph directory exists.

## Boundary coverage

- The component emits `approval.decide` with the selected execution and server-issued occurrence ID; schemas.ts registers a closed host-action input. Unsupported hosts have no writable callback, and no role-name or copied permission evaluator is added. Without reliable hints the supported host can submit and show refusal, as REQ-008 explicitly permits.
- The new `approvals/[approvalId]/+server.ts:16–24` requires the browser session, validates a strict approved/rejected body plus optional bounded reason, encodes both path IDs and calls the existing operation. Credentials remain in BrowserSession's private cookie field; the browser receives only the receipt/problem.
- `hooks.server.ts:23–33` selects the tenant's session on every request. `browser-sessions.ts:309–328` selects only that tenant cookie and metadata. Backend `routes.rs:730–743` decodes typed IDs/body and enters `VerificationControl::decide_approval`.
- `service/approvals.rs:80–97` authorizes retained execution and selects the occurrence from that exact execution's accepted state. `service/inspection.rs:511–624` resolves trusted verifier/parent scopes from tenant-owned queue or accepted identity, checks exact Verifier approve and executions-read on the retained parent, and queries decisions with the caller tenant. `VerifierExecutionQueue::execution_status` takes `&mut TenantConn`, not a raw pool; retained inspection acquires it using `caller.data_tenant_id`. Existing audit authorization is reused. No new grants, credential ceilings or continuation authority are created by the UI.
- `service/approvals.rs:114–207` preserves replay identity, execution deadline and terminal expiry, durable receipt acceptance, and same-execution wake. The new host does not implement durability or scheduling.
- `problem.ts:63–75` reuses the former cancel projection without changing its allowed fields: JSON success, otherwise stable code/title/detail/status. Cancel now calls this shared owner. No credential or raw exception is newly projected.

## Fresh executable evidence

Ran through mise against repository-managed Postgres and the existing built SDK/server:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- env WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/ui-library.integration.test.ts -t TASK-006
```

Result: exit 0, **1 passed, 23 filtered/skipped**, 8.07 seconds. The selected test is `TASK-006: each reached approval occurrence is read without the Verifier and decided only with approve on that Verifier and read on its Repository` (`ui-library.integration.test.ts:2485`). Inspected its assertions, not only its name: real Node/MCP occurrence reads without Verifier definition; browser-session host route read-only/approve-only/wrong-Verifier 403; foreign-tenant 404; invalid-body400; pending state unchanged; custom approve plus parent read succeeds; same actor/body receipt replay; changed decision409; repeated iteration receives a different ID; readback and retained receipt after discarded answer; same execution continuation; grant removal refuses in the existing session; cancellation expires the outstanding occurrence and decision409 while progress remains readable.

This does not claim full browser or identity-suite reruns. Parent reports fresh ui:check117 and codegen checks separately.

## Proposed finding DOM-1 — VIOLATION: incoming Origin is replaced before approval mutation

**Authority:** architecture/wyrd-security-posture.md:708–712 requires every cookie-authenticated mutation to reject missing/mismatched trusted public Origin, and says Node cannot replace hostile/missing Origin with trusted authority. TASK-006's Inputs/owners section requires reuse of the established Origin/CSRF/session boundary. This is an explicit boundary violation, not a new inferred product requirement.

**Location:** new `src/routes/t/[tenantKey]/verify/executions/[executionId]/approvals/[approvalId]/+server.ts:16–24`; shared forwarding owner `src/lib/server/auth/browser-sessions.ts:46–52,168–173`; `src/hooks.server.ts:23–34` has no incoming Origin check.

**Starting condition and trigger:** a live tenant browser-session cookie accompanies a JSON decision POST whose Origin is absent or mismatches the BFF's public origin. The session's principal otherwise has decision authority. Expected: reject before forwarding the mutation. Actual: the handler parses the body and invokes `session.api`; shared `call` supplies `origin: url.origin` regardless of the incoming header. Rust therefore receives trusted Origin even for a request the authority says must be refused.

**Runtime reproduction:** executed `mise exec -- node --input-type=module` with an inline script using Node http and child_process. It started a loopback controlled upstream returning valid session metadata for `GET /auth/browser/session?tenant=acme` and a sentinel decision receipt for the `/v1/.../approvals/...` POST; started the production `node build` BFF with HOST, PORT, ORIGIN and WYRD_SERVER_URL configured to those loopback listeners; then sent two requests to `/t/acme/verify/executions/execution/approvals/approval` with Cookie `wyrd_session_acme=opaque`, Content-Type `application/json`, and body `{"decision":"approved"}`. First request omitted Origin; second supplied `https://evil.example`. Both returned **200** and the upstream captured both decision POSTs with **Origin set to the trusted BFF origin**, not the missing/hostile input. The controlled upstream proves forwarding and Origin replacement, not real Rust persistence or browser exploitability. No files or implementation were changed for the reproduction.

**Materiality and limits:** the mandatory Origin trust boundary is bypassed on a newly exposed permission-sensitive mutation. Do not describe this as a demonstrated browser CSRF exploit: normal hostile cross-origin JSON fetches face CORS preflight; SvelteKit blocks text/plain/form mismatched Origins, and adapter-node discards a body without Content-Type (the attempted simple no-type case returned400). Those supplementary defenses do not establish the explicitly required missing/mismatched-Origin refusal. The new approval route inherits a shared forwarding weakness; the finding does not claim that weakness originated in this change.

**Smallest testable correction:** enforce the existing exact incoming-Origin rule at the shared authenticated mutation forwarding/request boundary before replacing headers, so this approval route and affected sibling mutation callers cannot launder Origin. Preserve tenant/session selection, current Rust permission checks, non-mutating reads, and bound callback exceptions. Add focused production-BFF HTTP evidence that missing/hostile Origin cannot reach the decision operation, while matching Origin permits the same authorized decision. No new auth API or permission evaluator is needed. Stable finding IDs and retention remain the independent validator's responsibility.

## Limits

No backend or schema changes, no new hot-path waits, and no benchmark requirement arise from this UI forwarding diff. TDD and visual/mock acceptance are primary-review coverage; this specialist does not reinterpret the maintainer's accepted/deferred mock differences. No production edits or broad suite runs were performed. DOM-1 needs independent validation; all other inspected security/domain boundaries have source plus focused real-server evidence.
