---
title: Review a pull request
description: Have an Agent review each pull request in the standard code_review report, judge the report with your own requirement, and read the Check and inline comments Wyrd posts on GitHub.
pillar: wyrd
section: Build and verify
topic: Verify
order: 6
---

Have an Agent review every pull request, decide the verdict from its report with your own rule, and see both on the pull request as a Check and inline review comments.

## Before you start

- A repository connected as in [Connect GitHub](/operate/connect-github/).
- A gateway deployment for your model; the example uses one named `openai`.
- A credential holding `cards:write` and `verifier:run`, as for [Repositories](/build-and-verify/declare/declare-a-repository/).

## 1. Ask the Agent for a report

Give the reviewer's Prompt `response_format: code_review`:

<CodeFromFile file="examples/review-a-pull-request/reviewer.yaml" lang="yaml" title="reviewer.yaml" />

The model must answer in the standard code review report: a `summary`, `coverage` (`complete` and its `limitations`) and `findings`. Each finding has an `id`, a `severity` (`critical`, `high`, `medium` or `low`), a `confidence` (`high`, `medium` or `low`), a `title`, an `explanation`, source `locations`, and an optional `remediation` whose `suggested_edit` replaces an exact line range. A location names the `path`, the `commit` and the inclusive `start_line` and `end_line`. A code step may return the same report.

## 2. Decide the verdict from the report

The report never decides the verdict. Your requirement does:

<CodeFromFile file="examples/review-a-pull-request/verifier.yaml" lang="yaml" title="verifier.yaml" />

The `pr` step extracts the pull request; `review` hands its source to the Agent. `acceptable-review` fails on a `critical` or `high` finding and is `inconclusive` when the review did not cover the whole change, so an empty report from an incomplete review never passes. Change the rule to suit your team.

## 3. Bind it to pull requests

<CodeFromFile file="examples/review-a-pull-request/repository.yaml" lang="yaml" title="repository.yaml" />

Set `repository_id` to your repository's numeric GitHub id, then declare it:

```bash
wyrd declare repository.yaml
```

## 4. Open a pull request

Open a pull request on the repository. Wyrd queues the review as in [Watch a pull request run](/build-and-verify/declare/declare-a-repository/#5-watch-a-pull-request-run). The pull request shows one Check named `wyrd/code-review` on its head commit:

| Verdict | Check conclusion |
| --- | --- |
| `passed` | `success` |
| `failed`, `errored` | `failure` |
| `inconclusive` | `action_required` |
| `cancelled` | `cancelled` |
| `timed_out` | `timed_out` |

While the execution waits for [evidence](/build-and-verify/verify/publish-deferred-evidence/) or an [approval](/build-and-verify/verify/approve-a-pull-request/), the Check stays `in_progress` and says what it waits for. Its summary lists every finding with its ID, severity and confidence.

With a report, Wyrd also posts one `COMMENT` review on the reviewed commit. A finding located inside the change becomes an inline comment, and its suggested edit a GitHub suggestion you can commit. Wyrd never approves, requests changes, resolves threads or edits your source. A finding whose location is not in the reviewed source, such as a line past the end of the file or another commit, stays in the Check summary with the reason; Wyrd never moves it to another line.

## 5. Inspect the delivery

Read the execution with the MCP tool `verification.get_execution` or `GET /v1/verification/executions/{execution_id}`. Its `github_delivery` field is separate from the verdict:

```json
{
  "status": "delivered",
  "problems": [{ "finding": "F-5", "reason": "anchor_out_of_range" }]
}
```

`status` is `pending` until delivery finishes, then `delivered`, `skipped` or `failed`. `reason` says why:

| Reason | Meaning |
| --- | --- |
| `superseded` | A newer commit was pushed; the old execution is not written again |
| `access_removed` | Wyrd lost access to the repository |
| `github_refused` | GitHub refused a write |
| `github_unavailable` | GitHub stayed unavailable or rate limited through every retry |
| `report_ambiguous` | Several outputs held a report; the Check shows only the verdict |
| `report_invalid` | The report did not match the format; the Check shows only the verdict |

Delivery runs after the verdict, from the stored result: it never reruns the review. Wyrd waits out GitHub's rate-limit cooldown and retries a few times. Before writing it looks for the Check and review it may already have posted, so a retry or a server restart updates them in place. If GitHub accepted a write but its answer was lost, a rare duplicate review is possible.

## If it fails

| You see | Fix |
| --- | --- |
| No Check on the pull request | Check the execution's status; a pull request that is a draft or whose repository is not selected queues nothing |
| The Check shows only the verdict | Read `github_delivery.reason`: make the review the only step that returns a report, or fix its shape |
| A finding is in the summary, not inline | Its location is not in the reviewed change; `github_delivery.problems` names why |
| `github_delivery.status` is `failed` | Fix the App's permissions or wait for GitHub; push a commit to review again |

## Next

[Approve a pull request](/build-and-verify/verify/approve-a-pull-request/) before its review passes.
