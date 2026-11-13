---
title: Publish CI results to a pull request
description: Declare a Verifier input that arrives after review starts, watch the pull request's execution wait for it, and publish it from any CI system with wyrd evidence publish.
pillar: wyrd
section: Build and verify
topic: Verify
order: 4
---

Have a pull request's Verifier wait for results your CI produces, then publish them from the CI job so the same execution resumes and judges them.

## Before you start

- A repository connected and declared as in [Repositories](/build-and-verify/declare/declare-a-repository/).
- A credential your CI job can use that holds `verifier:run` on the Verifier.

## 1. Declare a deferred input

Mark the input `deferred: true`. Its value is not passed when the pull request's execution starts; it is published later for the pull request's head commit:

<CodeFromFile file="fixtures/cards/deferred_evidence/ci-gate.yaml" lang="yaml" title="ci-gate.yaml" />

Bind it to the repository's pull requests:

<CodeFromFile file="fixtures/cards/deferred_evidence/repository.yaml" lang="yaml" title="repository.yaml" />

```bash
wyrd declare repository.yaml
```

A deferred input may be `json` or `media`.

## 2. Watch the execution wait

Open a pull request. Find its execution as in [Watch a pull request run](/build-and-verify/declare/declare-a-repository/#5-watch-a-pull-request-run), then inspect it:

```bash
wyrd execution status --execution-id <execution_id>
```

It shows `"status": "waiting"` and `"waiting": ["ci"]`. Checks that do not read `ci` run without it.

## 3. Publish from CI

When the CI job has its results, publish them for the commit it tested:

```bash
export WYRD_SERVER_URL=https://wyrd.example.com
export WYRD_API_KEY=<ci_credential>
export WYRD_REPOSITORY=acme/web
export WYRD_COMMIT=<head_commit_sha>
wyrd evidence publish ci --json results.json
```

Set `WYRD_REPOSITORY` and `WYRD_COMMIT` from whatever your CI system provides, or pass `--repository` and `--commit`. In a checkout of the repository you can omit both: `wyrd` reads `origin` and the checked-out commit from `.git`. For a `media` input, pass `--file report.pdf` instead of `--json`; the file is uploaded first.

It prints the receipt:

```json
{
  "evidence_id": "…",
  "repository": { "host": "github.com", "repository_id": "2" },
  "tags": { "commit": "<head_commit_sha>" },
  "input": "ci",
  "status": "accepted",
  "matched_subscriptions": 1,
  "recorded_at": "…"
}
```

A receipt means Wyrd stored the evidence durably for every Verifier that declares `ci` on this repository. Inspect the execution again: it resumes and ends `completed` with a verdict. Publishing the same results again answers `"status": "replayed"` with the same `evidence_id`, so a retried CI step is safe.

Evidence published before the pull request's execution exists is kept and used when it starts. Publishing never starts an execution and never reopens a finished one. An execution whose evidence never arrives ends `timed_out`, never passed.

<Aside type="note" title="From code or an agent">

The SDKs publish the same way: `verify.Evidence().publish("ci", value)` in Python, `verify::Evidence::publish` in Rust, and `verify.Evidence.connect()` then `publish` in TypeScript. Agents use the `verification.publish_evidence` [MCP tool](/reference/mcp-tools/).

</Aside>

## If it fails

| You see | Fix |
| --- | --- |
| `WYRD_VERIFICATION_400_INPUT_INVALID` naming `WYRD_COMMIT` or `WYRD_REPOSITORY` | Set the variable, pass the flag, or run in a checkout of the repository |
| `WYRD_VERIFICATION_409_EVIDENCE_UNMATCHED` | No Verifier bound to the repository declares that input as deferred: check the input name and the selected policy |
| `WYRD_VERIFICATION_409_EVIDENCE_CONFLICT` | Different results were already published for this commit; push a new commit to publish new results |
| `WYRD_VALA_503_BIFROST_WRITER_UNAVAILABLE` | The outcome is unknown: publish again; a stored publication replays |

## Next

[Bind more Verifiers](/build-and-verify/declare/bind-a-verifier/) to the repository's pull requests.
