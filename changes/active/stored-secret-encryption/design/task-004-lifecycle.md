# Task 004: provider-agnostic lifecycle recommendation

Status: finalized; maintainer agreed the design and explicitly approved the
corresponding spec/task revisions on 2026-11-19. [Spec revision 2](../spec.md)
records the contract; [task 004](../tasks/TASK-004-wrapping-key-lifecycle.md)
carries its implementation and proof. This document retains design rationale.

## Problem and outcome

The readiness review missed two material gaps: REQ-011 names a Wyrd-initiated
disable without defining its mechanism, and REQ-010 promises resumable recovery
without defining affected-value selection. Task 004 inherited those gaps.
Existing lifecycle helpers do not settle these decisions.

An administrator must be able to replace a deployment wrapping key and renew
every potentially exposed DEK without automatic maintenance hiding affected
values, while preserving resource identity, concurrency fences, and owner
runtime semantics. Backend choice must not change this workflow.

## Recommended contract

1. **Keep durable containment in deployment administration.** Remove the
   Wyrd-initiated key-disable action from this packet. Wyrd's trusted active/
   retained configuration already determines which logical keys it may use.
   Removing a reference and restarting/replacing every affected process makes
   that refusal survive restarts and clears process-held prepared material.
   External key-service access can also be denied by deployment administrators.
   Do not add key-admin operations to wrap/unwrap adapters, a database denylist,
   or an action that claims deployment-wide disable after local invalidation.
   A configuration rollout is not instantaneous containment: for immediate
   containment stop affected serving processes before restarting with the
   restricted configuration. Existing admitted-operation rules still apply.
2. **Use a new logical key for compromise recovery.** Provision independent
   safe material under a new, never-reused logical reference; changing material
   under the same reference is insufficient for portable selection. Configure
   it as active everywhere, retaining old references for controlled recovery
   reads. Recover all envelopes not naming that destination. Treat all values
   under retained references as affected rather than claim native-version
   precision the common envelope does not provide.
3. **Make envelope transitions explicit.** Remove automatic envelope rewrap
   from boot and Operator background scheduling. Activation changes new writes;
   it does not rewrite existing envelopes. Explicit platform maintenance selects
   routine rewrap or fresh-DEK recovery through the common facade. This avoids
   adding a pause flag or recovery-mode coordination state. Reuse existing
   bounded scans, cancellation, CAS, reports, and owner refresh; preserve boot's
   independent missing-KMS safety checks. Provider-native automatic key rotation
   is separate and remains supported.
4. **Define start/progress as a bounded synchronous pass.** Ship the maintenance
   action through HTTP and CLI under platform authority, matching resource-action
   surface scope; no SDK/MCP action or background job/status API. Return per-pass
   current/changed/remaining observations and redacted failures. Repeat the same
   mode/destination after interruption; committed envelope references are the
   progress record. Counts do not prove global completion or backup safety.

Explicit rewrap is an operational tradeoff: an administrator must invoke and
repeat maintenance. Removing automation is a Wyrd design recommendation, not a
claim that security standards prohibit automated rotation.

## Connected recovery flow and limits

Contain the incident and remediate compromised service access first. Prepare a
fresh destination and complete the configuration rollout before maintenance;
stop/drain old maintenance workers and writers so no old process continues
rewrapping or writing old references. For an incident discovered after some
rewrapping, select another fresh destination: existing references cannot prove
whether their DEKs were renewed. Do not reuse an earlier routine destination.

Run recovery passes against the new active destination. Each owner reads the
existing envelope and trusted binding metadata, renews it through `WyrdKms`
outside database locks, commits by existing authority/envelope fences, and
refreshes existing runtime material. A crash after commit leaves a destination
reference that the next recovery pass skips; a failed commit leaves old state
eligible for retry. Concurrent credential replacement seals a fresh DEK under
the destination and must win over stale recovery work. Concurrent revocation
must never be undone. Do not run routine rewrap into this recovery destination:
the operational procedure must isolate incident recovery from routine passes.
This proposal does not add a durable incident lock preventing an authorized
administrator from deliberately selecting the wrong mode.

If containment denied old-key access, renewal cannot decrypt those values.
Either permit tightly controlled recovery access after containment or replace
the actual credentials through their existing owner actions without decrypting
the old envelopes. Missing keys are failures, never permission to invent values.
Restore and backup dependencies remain separate retirement constraints.

Renewing encryption cannot repair disclosure of the underlying credential.
Replace/revoke potentially exposed credentials at their issuers as well; never
report encryption renewal as upstream credential revocation.

## Evidence and proof to carry into the revised task

Inspected repository evidence:

- `spec.md` REQ-004/005 stores logical references and limits key selection to
  active/retained configuration; REQ-010/011 leaves the decisions above open.
- `crates/wyrd/wyrd-kms/src/lib.rs`: `is_current` compares only the logical
  reference; `renew` creates a fresh DEK while `rewrap` retains it.
- `crates/wyrd/wyrd-server/src/boot/mod.rs`: `validate_startup` invokes
  `rewrap_sealed_secrets`; its missing-configuration refusal must survive
  removal of automatic transitions.
- `crates/wyrd/wyrd-server/src/verification/operators.rs`: `run` schedules
  `rewrap` alongside delivery. Remove that scheduling, preserve delivery.
- `architecture/wyrd-security-posture.md`, Stored secrets, requires fresh-DEK
  compromise recovery and preserves separate owner lifecycle and issuer limits.

The revised task should prove through real server/Postgres HTTP and CLI paths:
activation and restart do not rewrite old envelopes; routine passes retain DEKs;
recovery passes renew them across every current owner and tenant/platform scope;
interruption, denied keys, concurrent replacement/revocation, and owner refresh
preserve the existing fences and runtime behavior. Verify configuration removal
and process restart refuse old references without claiming global instantaneous
cache erasure. No new table, migration, dependency, or backend admin method is
needed for this recommendation.

Primary guidance checked for this recommendation:

- [AWS rotation](https://docs.aws.amazon.com/kms/latest/developerguide/rotate-keys.html)
  distinguishes key-material rotation from changing DEKs or payload encryption.
- [Google re-encryption](https://docs.cloud.google.com/kms/docs/re-encrypt-data)
  distinguishes rotation, rewriting existing data, and subsequent retirement.
- [AWS disabling](https://docs.aws.amazon.com/kms/latest/developerguide/enabling-keys.html)
  explains that denial of key-service use does not erase already available DEKs.
- [OWASP secrets management](https://cheatsheetseries.owasp.org/cheatsheets/Secrets_Management_Cheat_Sheet.html)
  separates lifecycle actions and least-privilege access responsibilities.

These support the security distinctions; the explicit-pass workflow is the
proposed application of those distinctions to Wyrd's existing constraints.

## Handoff

The maintainer approved the spec/task revisions. Revision 2 and Task 004 now
record surface scope, recovery selection and preconditions, containment, and
removal of automatic transitions. Tasks 001–003 retain revision-1 evidence;
Task 004 owns changed boot/scheduling tests and operational documentation.
No production code, authority document, or schema changed during this revision.
No runtime tests or cloud experiments were run.
