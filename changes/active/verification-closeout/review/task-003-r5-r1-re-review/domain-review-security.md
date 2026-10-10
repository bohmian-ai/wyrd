# TASK-003 R1 security, RBAC, and tenancy domain review

**Subject:** original base `7f79fb3417db651adedac194ada8908f0a0372d7`, cumulative candidate `9a8f9f7eef95f70d356c037a192b7d7b90a37f31`; remediation from `f6c841d57`. **Result: PASS.** No material finding in this domain.

## Authority and boundary

I reviewed `AGENTS.md`, `architecture/agent-rules.md`, `architecture/wyrd-design.md`, `architecture/wyrd-security-posture.md`, the applicable architecture constraints, approved `changes/active/verification-closeout/spec.md`, original TASK-003 r4, prior verdict and TASK-003-R1, the cumulative source diff, and the R1 fix diff. The relevant rules are verified caller identity, tenant-scoped Card resolution, signed Card scope for bound principals, exact gateway and Verifier permissions, and a blocking permission decision with nonblocking audit staging.

| Path | Source evidence and result |
|---|---|
| Public gateway ingress | `components/gateway/ingress.rs::requested_subject` requires paired Run and Card UID headers, rejects duplicates and malformed values; ingress verifies the Wyrd access token and does not forward the headers. PASS. |
| Gateway attribution and capture | `components/gateway/invocation.rs::admit_call` checks model permission and calls `attribute` before snapshot routing and admission; `attribute` checks signed `CardRefScope::uids()` for a bound caller or `is_observation_target` through `tenant_conn(caller.data_tenant_id)` for an unbound caller. `capture.rs` writes that authorized UID. `pg_invocation_tests.rs::gateway_correlation_is_authorized_before_dispatch_and_captured` checks the captured UID and both denial branches before dispatch. PASS. |
| Token and queued judge authority | `wyrd-auth-issue::build_access_claims` validates the principal binding and re-derives signed Card scope; `verification/authority.rs::writer_caller` recovers the observation writer with current grants through a tenant connection. Gateway judge calls use that caller. PASS. |
| Direct Verifier authorization | `components/verification/service.rs::execute` requires `verifier:run` on the selected UID and stages the allow or deny decision before tenant-scoped target resolution or execution. PASS. |
| R1 deployment identity | The three support-desk `deploy` functions now match the Prompt route's exact gateway provider/model pair (`openai` plus the Prompt model); each journey exercises an `anthropic/gpt-4o` collision before adding the correct deployment. The changed examples neither grant permissions nor select a tenant. The production gateway trust boundary was not modified in R1. PASS. |

## Verification limits

This is a source review; I did not rerun security or journey tests. The task records the focused Rust, Python, and TypeScript support-desk journeys as passing. The R1 changes to Oracle shutdown, Forge recovery, Scribe test setup, SQL test wrapper, and documentation do not alter gateway authorization or token issuance. I found no new SQL production signature using a privileged pool in the R1 diff. The checked-in gateway denial test proves non-dispatch and non-capture for the two Card-attribution refusal cases; it does not independently prove every provider dialect's HTTP rendering.
