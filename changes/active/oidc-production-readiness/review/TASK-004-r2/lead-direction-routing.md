# Lead direction: routing the open TASK-004 round-2 findings

Decided by: the wyrd-run lead, 2026-10-02, following spec revision 11
(approved by Steven Forrester, 2026-10-02) and
[`research/auth-standards-recommendation.md`](../../research/auth-standards-recommendation.md).

There is no TASK-004 round 3. Spec revision 11 replaces or deletes the code
these findings touch: the hand-written token exchange, the CLI browser
launcher, the sealed device completion, and the BFF channel. Each open finding
is now an explicit acceptance item in the task that owns that code. It closes
through that task's review and the final change review.

| Finding | Subject | Owning task |
|---|---|---|
| FIND-TASK-004-5 | Parsed cleartext target check | [TASK-012](../../tasks/TASK-012-client-oauth2.md) |
| FIND-TASK-004-8 | Stale source and generated docs | [TASK-005](../../tasks/TASK-005-qualification-and-docs.md) |
| FIND-TASK-004-9 | Saved server identity is a URL origin; userinfo refused | [TASK-012](../../tasks/TASK-012-client-oauth2.md) |
| FIND-TASK-004-10 | Credential directory rejects group/world write | [TASK-012](../../tasks/TASK-012-client-oauth2.md) |
| FIND-TASK-004-11 | Withdrawn by [lead-direction-FIND-TASK-004-11](lead-direction-FIND-TASK-004-11.md) | none |
| FIND-TASK-004-12 | Device grant terminal state; mint at redemption | [TASK-010](../../tasks/TASK-010-authorization-server-grants.md) |
| FIND-TASK-004-13 | No redirects on secret-bearing calls | [TASK-009](../../tasks/TASK-009-oidc-relying-party.md) (screened relying-party adapter) and [TASK-012](../../tasks/TASK-012-client-oauth2.md) (client) |
| FIND-TASK-004-14 | Windows launcher without a command interpreter | [TASK-012](../../tasks/TASK-012-client-oauth2.md) via `webbrowser` |

The diagnoses in
[`TASK-004-R2-production-readiness-fixes.md`](TASK-004-R2-production-readiness-fixes.md)
stand as context. Its prescribed fixes are superseded where the owning task
replaces the code with a vetted library.
