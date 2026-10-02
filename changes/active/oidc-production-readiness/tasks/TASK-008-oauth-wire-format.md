---
id: TASK-008
kind: implementation
status: ready
spec: SPEC-oidc-production-readiness
spec_revision: 10
requirements: [REQ-021, REQ-011, REQ-012, REQ-013, AC-004, AC-005, AC-009]
depends_on: [TASK-004]
---

# Standard OAuth wire format

## Outcome and Value

A standard OAuth client (any off-the-shelf library) can use Wyrd's token,
platform token, revocation, and device authorization endpoints unchanged,
because they speak the OAuth wire format instead of a Wyrd-specific one.

## Owners, Scope, Consumers, and Prohibited Changes

The server owns `/auth/token`, `/auth/platform/token`, `/auth/revoke`, and
`/auth/device_authorization`. `wyrd-spec` owns their wire types. Consumers are
`wyrd-client` (and through it the Rust, Python, and TypeScript SDKs), the CLI,
the BFF, journeys, generated schemas, and docs. Change only the wire format:
grant semantics, authorization, tenancy, audit, and token contents stay as
they are. Do not keep a JSON alternative, add a compatibility route, or add
anything the RFCs do not have (no discovery document, no new grant).

## Approach

1. Requests: accept `application/x-www-form-urlencoded` bodies with the RFC
   parameter names for every grant on these endpoints (RFC 6749 §3.2 and §6,
   RFC 7009 §2.1, RFC 8628 §3.1 and §3.4, RFC 8693 §2.1, RFC 7523 §2.1).
2. Success: the RFC 6749 §5.1 JSON body (`access_token`, `token_type`,
   `expires_in`, optional `refresh_token`, `scope`; RFC 8693 adds
   `issued_token_type`), with `Cache-Control: no-store`. RFC 8628 §3.2 for the
   device authorization response; RFC 7009 §2.2 for revocation.
3. Errors: the RFC 6749 §5.2 JSON body (`error`, optional `error_description`)
   with registered codes (`invalid_request`, `invalid_client`,
   `invalid_grant`, `unauthorized_client`, `unsupported_grant_type`,
   `invalid_scope`, plus RFC 8628 and RFC 8693 codes), HTTP 400, or 401 with
   `WWW-Authenticate` for `invalid_client`. Map existing Wyrd failures onto
   those codes; record the mapping in the docs.
   These endpoints are the one sanctioned exception to the `WyrdError`
   problem+json rule (AGENTS.md §9); record that exception in the
   architecture authority. Every other endpoint keeps problem+json, and
   server-side logging and audit keep the Wyrd error codes.
4. Move `wyrd-client`, the CLI, BFF, and all three SDK paths to the form
   format through the shared client, then update journeys, schemas, and docs.

## Acceptance Criteria

Each endpoint accepts form-encoded requests and returns RFC-shaped success and
error bodies; a JSON request body is refused with `invalid_request`. An
off-the-shelf OAuth client library completes a device-code login and a refresh
against a real server in a journey. All existing human, CLI, SDK, API-key, and
workload journeys still pass through the shared client. Generated schemas and
docs show the RFC wire format.

## Verification and Evidence

Run the identity journey lane and its targets, `test:shared`,
`test:wyrd-sdk`, `test:cli:journey`, `test:principals:integration`,
`py:test:integration`, `ts:test:integration`, `test:wyrd`, `codegen:check`,
`docs:check`, `fmt`, `lints`, `py:format`, `py:lints`, `py:test:unit`,
`py:typecheck`, `ts:test:unit`, `ts:typecheck`, and the boundary checks. Name
the off-the-shelf OAuth client used and its exact journey command.

## Material Stop Conditions

Stop if conforming would change grant semantics, authorization, or tenancy,
or would require a non-RFC extension.

## Authority Links

[Approved spec](../spec.md); [AGENTS.md](../../../../AGENTS.md);
[RFC 6749](https://www.rfc-editor.org/rfc/rfc6749);
[RFC 7009](https://www.rfc-editor.org/rfc/rfc7009);
[RFC 8628](https://www.rfc-editor.org/rfc/rfc8628);
[RFC 8693](https://www.rfc-editor.org/rfc/rfc8693);
[RFC 7523](https://www.rfc-editor.org/rfc/rfc7523).
