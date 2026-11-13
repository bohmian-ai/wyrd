---
title: Approve a pull request
description: Add a step that waits for a person, find its pending approval on the pull request's execution, and approve or reject it so the same execution finishes.
pillar: wyrd
section: Build and verify
topic: Verify
order: 5
---

Have a pull request's Verifier wait for a person to sign off, then approve it so the same execution finishes with a verdict.

## Before you start

- A repository connected and declared as in [Repositories](/build-and-verify/declare/declare-a-repository/).
- A credential for the approver that holds `verifier:approve` on the Verifier, such as one with the `editor` role.

## 1. Add an approval step

Add a step with `action: {kind: approval, message}`, and a requirement that passes only when it is approved:

<CodeFromFile file="fixtures/cards/approvals/signoff.yaml" lang="yaml" title="signoff.yaml" />

Bind it to the repository's pull requests:

<CodeFromFile file="fixtures/cards/approvals/repository.yaml" lang="yaml" title="repository.yaml" />

```bash
wyrd declare repository.yaml
```

The `approved` requirement is what makes approval matter: the step alone records a decision but passes nothing.

## 2. Find the pending approval

Open a pull request. Find its execution as in [Watch a pull request run](/build-and-verify/declare/declare-a-repository/#5-watch-a-pull-request-run), then inspect it:

```bash
wyrd execution status --execution-id <execution_id>
```

It shows `"status": "waiting"` and lists the approval:

```json
"approvals": [
  {
    "approval_id": "…",
    "step_id": "signoff",
    "message": "Ship this change?",
    "status": "pending",
    "actor_id": null,
    "decided_at": null,
    "reason": null
  }
]
```

This Verifier also waits for `ci`, published as in [Publish CI results to a pull request](/build-and-verify/verify/publish-deferred-evidence/). Each wait is independent: publishing CI does not decide the approval, and approving does not supply CI.

## 3. Approve it

As the approver:

```bash
wyrd execution approve --execution-id <execution_id> --approval-id <approval_id> --reason "reviewed the diff"
```

It prints the receipt once the decision is stored:

```json
{
  "approval_id": "…",
  "status": "approved",
  "actor_id": "…",
  "decided_at": "…",
  "reason": "reviewed the diff"
}
```

Inspect the execution again: the same execution continues and ends `completed` with a verdict. Running the same command again prints the same receipt, so a retry is safe.

`wyrd execution reject` takes the same arguments. A rejection is a decision, not an error: the `approved` requirement fails and the verdict is `failed`.

An approval has no deadline of its own. If the execution times out, or a new commit supersedes it, its pending approvals become `expired` and can no longer be decided.

<Aside type="note" title="From code or an agent">

The SDKs decide on the execution handle: `execution.decide(approval_id, "approved", reason)` in Python, `Execution::decide` in Rust, and `execution.decide(approvalId, "approved", reason)` in TypeScript. Agents use the `verification.decide_approval` [MCP tool](/reference/mcp-tools/).

</Aside>

## If it fails

| You see | Fix |
| --- | --- |
| `WYRD_PERMISSION_403_DENIED_RBAC` | The approver needs `verifier:approve` on this Verifier and read access to the Card it verifies |
| `WYRD_VERIFICATION_404_APPROVAL_NOT_FOUND` | Copy the `approval_id` from this execution's `approvals`; each execution has its own |
| `WYRD_VERIFICATION_409_APPROVAL_NOT_PENDING` | It was already decided, or the execution ended; `details.status` says which. Inspect the execution |
| `WYRD_VALA_503_BIFROST_WRITER_UNAVAILABLE` | The outcome is unknown: run the same command again; a stored decision replays |

## Next

[Bind more Verifiers](/build-and-verify/declare/bind-a-verifier/) to the repository's pull requests.
