# Human direction: real sign-in connection test

Decided by: Steven Forrester (human owner), 2026-10-01.

This direction adds one item, `HD-TASK-003-R2-1`, to
`TASK-003-R2-production-ui-remediation.md`. Every finding in that file stands
as written. Authority: `SPEC-oidc-production-readiness` revision 7, REQ-003 and
AC-006.

## HD-TASK-003-R2-1 — Replace side-effect probes with a real test sign-in

Wyrd does not infer or authenticate anything through side effects. The current
candidate test in `crates/wyrd/wyrd-auth/src/connections.rs` proves callback
registration through a non-interactive `prompt=none` redirect and client
authentication through a fabricated code expected to return `invalid_grant`.
Dex (v2.38–v2.45, dexidp/dex#4560) and classic Amazon Cognito ignore
`prompt=none`, so providers that complete standard logins cannot be activated;
RFC 6749 §5.2 does not mandate the error ordering the fake-code probe relies
on. This contradicts the provider-agnostic requirement.

Required outcome:

- Testing a candidate keeps the discovery (issuer pinning) and signing-key
  checks, then starts one real authorization-code login (PKCE, state, nonce)
  bound server-side to the exact candidate revision and the authorized caller,
  and gives the caller the provider authorization URL through the headless
  API; the UI settings test action projects the same flow.
- The existing callback redeems the real code and verifies the ID token
  exactly as a production login, including issuer binding. For a test-bound
  state it accepts that exact candidate instead of requiring the active
  connection, re-checks the caller's authority, and marks only that candidate
  revision tested. It issues no session, Wyrd credential, or `User`, and
  audits the outcome. Test state has the same single-use, expiry, and mix-up
  protections as login state.
- Delete the `prompt=none` callback probe, the fabricated-code client-auth
  probe, and their constants and tests. Add no provider-specific branch and no
  "unconfirmed" or soft-pass status.
- Activation still requires a tested candidate revision; a changed revision
  needs a new test.

Evidence required, in the existing identity lane and journeys:

- Dex, the repository's already-running second provider, is tested through a
  real sign-in and activated; this also satisfies FIND-TASK-003-6's
  different-provider topology.
- A test sign-in creates no session, credential, or `User`; a replayed,
  expired, or cross-tenant test state is refused; a wrong callback or client
  secret fails the test where a real login would.
- Generated schemas, API docs, and TASK-005-facing setup docs describe the
  interactive test.

Reversible choices (type names, state representation, route shapes inside the
existing API) belong to the implementer.
