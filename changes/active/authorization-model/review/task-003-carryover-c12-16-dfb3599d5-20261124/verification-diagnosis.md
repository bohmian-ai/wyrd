# Fresh onboarding UI execution: SMTP refusal diagnosis

Candidate remains `dfb3599d5fbfb3d863ecb68382257cb30abfd79b`. Read-only diagnostic follow-up requested by the orchestrator after its fresh `mise run test:identity:ui` execution reported six onboarding failures, all waiting for Check your email after `/auth/signup` returned 503. No source changed and no test rerun was attempted.

## Observation and expected behavior

Root observed repeated `Connection refused (os error 111)` relay failures at approximately 12:41:53, 12:42:03 and 12:42:13 UTC. These are not expected negative cases in `onboarding.integration.test.ts`: `enableMailAndSignup` always configures the fixture's real Mailpit at lines 57–67, and the email-signup helpers and all dependent flows require actual delivered messages. That file has no deliberately refused-relay/503 scenario. Its seven browser tests therefore did not freshly pass; the pre-existing reported 2/2 identity lane pass is not interchangeable with this run.

## Traced configuration and error path

1. `scripts/postgres/with-test-postgres.sh:60` starts Postgres and Mailpit with Compose `--wait`. At lines 123–124 it exports SMTP/API endpoints directly from `docker compose port`.
2. `identity_ui_e2e.rs:697–700` reads `WYRD_TEST_MAILPIT_SMTP` and splits the final colon. Lines 715–718 place that host and parsed port in the `WYRD_UI_ONBOARDING` fixture.
3. Browser test `enableMailAndSignup` posts that fixture host/port with `tls: none` to the platform mail owner. There is no rewritten port or intentional refusal in this path.
4. `ApplicationMail::relay` (`wyrd-auth/src/mail.rs`) reads stored `host` and `port`, selects `builder_dangerous` for `none`, and applies `.port(port)`. `send_notice` logs the SMTP error and converts it to `AuthVerifyUnavailable`. A failed SMTP connect therefore explains the observed generic 503 and missing Check your email state without assuming a UI defect.

## Read-only environment observations

- Docker context: `colima`, daemon socket `unix:///home/thorrester/.config/colima/default/docker.sock`.
- First Docker snapshot: `wyrd-test-2227317-2227323-wyrd-mailpit-1` published `127.0.0.1:34316→1025`, `127.0.0.1:34317→8025`; Postgres published `127.0.0.1:34318→5432`.
- Mailpit logs: at 12:41:11 UTC SMTP started on `[::]:1025`, HTTP on `[::]:8025`. Docker health inspection showed healthy with recent successful readiness checks through 12:42:56 UTC. Compose health uses `/mailpit readyz` inside the container, not host-endpoint reachability.
- A direct read-only host TCP connection to `127.0.0.1:34316` refused independently of the Wyrd client. The following Docker snapshot showed no containers, and later SMTP/API/Postgres probes all refused. Thus lane teardown had occurred by that later snapshot; the direct probe alone cannot establish a persistent pre-teardown port-forward defect.
- One optional diagnostic invocation of `mise exec -- python` could not launch because `python` is unavailable; the equivalent Bash TCP probe supplied the observation above. This is not a product/test failure.

## Diagnosis and limits

The failure is an unexpected inability to connect to the configured test relay, not a deliberately exercised SMTP negative case. The traced fixture path explains how its unavailable endpoint produces all six failures at their first email-dependent step. No evidence supports reporting those six downstream flows as newly failed product semantics or as fresh passes.

The repository lifecycle already documents and handles delayed/missed Colima host port forwarding for Postgres: `start_postgres` validates the host endpoint and rebuilds once if it never accepts. It exports Mailpit endpoints without the equivalent host reachability check. The healthy-inside/refused-host pattern is consistent with that environment mechanism, but the exact cause of the SMTP endpoint's refusal is **UNRESOLVED** because no contemporaneous host/socket/VM trace was captured before teardown. Do not promote the forwarding explanation to a proven root cause, prescribe production changes, or claim a rerun result from these observations.

The next bounded verification step, if requested, is to preserve the existing lane's diagnostics and confirm the exported SMTP and API endpoints are reachable from the same host while their container is still running, then rerun the unchanged focused onboarding journey through its existing lifecycle. This report neither implements a harness fix nor adds a review finding.
