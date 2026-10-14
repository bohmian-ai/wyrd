# Security Audit — integrated `example-journey-fixes`

## Review result

**Overall: FAIL.** The cumulative `ef03f624a1f3a6bcd51bbd17060271f0b5e8cabf..c4ba2b0502f8968a78e5e960bcc9eba43e960643` change has one exploitable credential-disclosure path and one external-input validation defect. The authorization, tenancy, secret-at-rest, MCP discovery, and Bifrost error-opacity controls reviewed below otherwise match the binding authorities.

## Reviewed boundary

This review traced the changed security-relevant paths end to end:

- inbound gateway `wyrd-run-id` / `wyrd-card-uid` parsing, Card-scope authorization, persisted caller attribution, and direct/manual/internal Verifier requester semantics;
- OD-1's SYSTEM authority boundary, including the explicitly open observation-triggered judge-caller question;
- managed-secret admission, tenant/name/provider authenticated binding, deployment-keyring sealing/opening, boot rewrap, local-development key provisioning, and redaction;
- Jev credential injection, upstream refusal handling, retry classification, response relay, and capture scrubbing;
- MCP caller-dependent discovery and dispatch-time authorization;
- Bifrost table authorization before materialization and caller-visible planner-error mapping.

The three owner-held items were checked but are not findings: the pre-release result-column migration has no compatibility path; a tenant with no active SYSTEM principal may record a null internal requester; and observation-triggered judge calls retain `writer_caller` pending the OD-1 decision (`crates/wyrd/wyrd-server/src/verification/eval.rs:253`).

## Authority and source coverage

- Approved change authority: `changes/active/example-journey-fixes/spec.md` revision 4, especially REQ-001/002/003/004/005/007, INV-001/002/003, and AC-001..004.
- Binding owner decisions: OD-1 through OD-7 in `changes/active/example-journey-fixes/owner-decisions.md`; each was evaluated. OD-1/2/3/5/6/7 are satisfied in the reviewed security boundary except for the malformed-tracestate defect below. OD-4's arbitrary upstream-body relay is the direct precondition for SEC-001 and needs owner reconciliation with INV-002.
- Task evidence: TASK-001 through TASK-005, including the replacement TASK-005 evidence, was compared with the complete cumulative diff rather than accepted as proof by summary.
- Repository authority: `AGENTS.md`, `architecture/agent-rules.md`, `architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`, `architecture/wyrd-security-posture.md`, and the relevant Oracle/authorization sections of `architecture/bifrost-design.md`.

## Critical

None.

## High

### SEC-001 — VIOLATION — Jev authentication-body relay can return a reversibly encoded managed credential

- **Authority:** INV-002 says provider credentials are never returned (`changes/active/example-journey-fixes/spec.md:176`); the security posture independently prohibits secrets in errors (`architecture/wyrd-security-posture.md:25`). OD-4 permits the upstream 401/403 body only "with the credential removed" (`changes/active/example-journey-fixes/owner-decisions.md:74`).
- **Evidence:** Jev routes 401/403 into the shared body relay (`crates/wyrd/wyrd-gateway/src/adapter/http.rs:458`, `crates/wyrd/wyrd-gateway/src/adapter/http.rs:479`). The scrubber removes only literal occurrences (`crates/wyrd/wyrd-gateway/src/adapter/mod.rs:314`, `crates/wyrd/wyrd-gateway/src/adapter/mod.rs:317`) and recognizes only a whole standard-base64 string or whole base64 data URL (`crates/wyrd/wyrd-gateway/src/adapter/mod.rs:392`, `crates/wyrd/wyrd-gateway/src/adapter/mod.rs:398`, `crates/wyrd/wyrd-gateway/src/adapter/mod.rs:402`). The acceptance tests cover literal and whole standard-base64 forms only (`crates/wyrd/wyrd-gateway/src/adapter/tests.rs:2981`, `crates/wyrd/wyrd-gateway/src/adapter/tests.rs:3634`). Percent encoding, hex, URL-safe base64, base64 embedded in surrounding text, and split/concatenated representations pass unchanged.
- **Exploit path:** A compromised or malicious configured Jev endpoint receives the real managed credential in the outbound authentication header. It answers 401 or 403 with, for example, the credential percent-encoded in a diagnostic field. An inference caller needs gateway invocation permission but not `gateway:read` or `gateway:write`; the server relays that field, and the caller decodes it and reuses the provider credential outside Wyrd. Literal removal and current standard-base64 tests both still pass.
- **Consequence:** Provider authority crosses from the deployment's managed-secret boundary to an ordinary model caller. This can enable direct provider use, spend, data access, or further compromise under the stolen provider identity.
- **Testable correction:** Do not relay arbitrary provider-controlled auth-refusal strings when Wyrd injected a credential. Replace them with a Wyrd-owned, secret-free 401/403 envelope, or have the owner narrow OD-4 to a strictly allowlisted structured projection whose values cannot carry attacker-selected strings. Add native and unified-route journey cases for percent encoding, hex, URL-safe/embedded base64, and split representations, asserting that neither the response nor captured evidence contains a reversible credential representation and that no retry occurs. Because arbitrary opaque body relay and an absolute "credentials are never returned" invariant cannot both be guaranteed against a malicious upstream, this correction requires an explicit owner reconciliation of OD-4 with INV-002 rather than another finite encoding blacklist.

## Medium

### SEC-002 — INCORRECT — malformed W3C `tracestate` is accepted and persisted

- **Authority:** REQ-005 requires W3C `traceparent` and `tracestate` handling and says malformed headers are ignored (`changes/active/example-journey-fixes/spec.md:141`). OD-3 limits trace context to observability linkage (`changes/active/example-journey-fixes/owner-decisions.md:57`).
- **Evidence:** The HTTP boundary validates only that each `tracestate` header is visible ASCII and concatenates repeated values (`crates/wyrd/wyrd-server/src/http/middleware/trace_context.rs:52`, `crates/wyrd/wyrd-server/src/http/middleware/trace_context.rs:63`). `TraceContext::parse_traceparent` accepts the resulting `trace_state` without validating W3C member grammar, duplicate keys, member count, or total length (`crates/wyrd-spec/src/trace.rs:24`, `crates/wyrd-spec/src/trace.rs:39`). Capture then writes the unvalidated value into OTLP spans (`crates/wyrd/wyrd-server/src/components/gateway/capture.rs:365`, `crates/wyrd/wyrd-server/src/components/gateway/capture.rs:377`, `crates/wyrd/wyrd-server/src/components/gateway/capture.rs:867`). Existing middleware tests exercise malformed `traceparent`, not malformed `tracestate` (`crates/wyrd/wyrd-server/src/http/middleware/trace_context.rs:115`).
- **Consequence:** Any authenticated inference caller can inject non-W3C vendor state into retained trace evidence. The request remains non-fatal, but the candidate does not ignore malformed state as required and can produce trace records that standards-compliant downstream processors reject or interpret inconsistently.
- **Testable correction:** Validate the combined header against W3C tracestate grammar and limits at ingress. Preserve a valid `traceparent` while dropping malformed `tracestate`; do not persist or propagate the rejected value. Add focused cases for invalid keys/values, duplicate keys, more than 32 members, over-length state, and invalid repeated-header combinations, plus a valid repeated-header case.

## Low / Defense In Depth

None.

## Positive Controls

- Gateway correlation headers are single-valued and bounded/typed at ingress (`crates/wyrd/wyrd-server/src/components/gateway/ingress.rs:263`), and Card attribution is authorized before provider routing against signed scope or a tenant-local registered observation target (`crates/wyrd/wyrd-server/src/components/gateway/invocation.rs:734`). Run ID remains optional context and is not treated as authority.
- Managed secrets are sealed with the shared deployment keyring in a payload authenticated to tenant, credential name, and provider; decrypted and serialized plaintext is zeroized, and secret-bearing `Debug` is redacted (`crates/wyrd/wyrd-gateway/src/managed.rs:22`, `crates/wyrd/wyrd-gateway/src/managed.rs:58`, `crates/wyrd/wyrd-gateway/src/managed.rs:99`, `crates/wyrd/wyrd-gateway/src/managed.rs:125`). Runtime resolution rechecks active state, provider identity, assignment/binding, and nonempty content (`crates/wyrd/wyrd-gateway/src/credential.rs:106`).
- Local development key creation uses random 32-byte material, an owner-only adjacent temporary file, file sync, and no-clobber persistence without printing the key (`crates/wyrd/wyrd-cli/src/server/dev.rs:525`, `crates/wyrd/wyrd-cli/src/server/dev.rs:557`, `crates/wyrd/wyrd-cli/src/server/dev.rs:568`).
- MCP write-tool discovery is caller-dependent while dispatch still owns authorization; cache metadata is private and immediately stale (`crates/wyrd/wyrd-server/src/mcp/mod.rs:200`, `crates/wyrd/wyrd-server/src/mcp/mod.rs:210`, `crates/wyrd/wyrd-server/src/mcp/mod.rs:227`).
- Oracle authorizes every resolved table UID before opening metadata, manifests, or data (`crates/vala/vala-bifrost-redux/src/oracle/planner.rs:130`, `crates/vala/vala-bifrost-redux/src/oracle/mod.rs:3426`). Only caller-fault planner details over the already-authorized session catalog are exposed; other failures retain opaque mapping (`crates/vala/vala-bifrost-redux/src/oracle/mod.rs:3762`).

## Verification limits

- HEAD was confirmed as immutable candidate `c4ba2b0502f8968a78e5e960bcc9eba43e960643`; the complete base-to-candidate diff was reviewed. The only working-tree content observed was the review output directory.
- Focused `mise exec -- cargo nextest run` for the two existing gateway refusal tests could not start: the sandbox redirected mise state successfully, but mise then attempted to provision Rust 1.98.1 and the restricted runtime aborted `rustup` before Cargo ran. This is an environment limitation, not passing or failing product evidence.
- No Postgres-backed lane was rerun in this domain pass. Task-recorded evidence was used as supporting evidence only; source tracing established the findings and positive controls above.
