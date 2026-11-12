---
title: Repositories
description: Declare a connected GitHub repository's review policy from wyrd.toml, check it offline, watch its pull requests run, and roll it back or empty it.
pillar: wyrd
section: Build and verify
topic: Declare
order: 3
---

Declare which Verifiers review a connected GitHub repository's pull requests, from a `wyrd.toml` file in its checkout.

## Before you start

- A tenant connected to GitHub, with the repository selected, from [Connect GitHub](/operate/connect-github/).
- A checkout of the repository whose `origin` remote points at it on GitHub.
- A Verifier file, such as one from [Write a Verifier](/build-and-verify/verify/author-a-verifier/).

## 1. Write the policy

Save the Verifier in the checkout:

<CodeFromFile file="fixtures/cards/repository_declaration/code-review.yaml" lang="yaml" title="code-review.yaml" />

Save `wyrd.toml` at the checkout's root, listing every Verifier that reviews its pull requests:

<CodeFromFile file="fixtures/cards/repository_declaration/policies/minimal.toml" lang="toml" title="wyrd.toml" />

Each `verifier` entry is a path relative to `wyrd.toml` or a declared Verifier such as `{ kind = "Verifier", name = "answer-check", version = "1.0.0" }`.

## 2. Check it offline

```bash
wyrd declare --check wyrd.toml
```

It lists the Repository, named after the repository, and the `code-review` Verifier. It contacts no server, so it reports the checks it left for declaration as deferred: the repository's identity, declared references, your permissions, and the version.

## 3. Declare it

```bash
wyrd declare wyrd.toml
```

Wyrd reads the repository from the `origin` remote, confirms that your tenant's GitHub connection reaches it, and declares the Repository with one binding per Verifier. It prints `declared: Repository/<name>@<version>`. That version becomes the repository's selected policy only once the whole declaration succeeds. Read it back:

```bash
wyrd get --kind Repository --name <name> --version <version>
```

Its `status.repository` shows `"selected": true`, the `selected_version`, and the repository's GitHub `full_name`.

Declaring an unchanged `wyrd.toml` again reuses the same version. Changing the list declares a new version and selects it; a declaration that fails keeps the previous selection.

## 4. Roll back or stop reviewing

To return to an earlier policy, pin its exact version with the same content:

<CodeFromFile file="fixtures/cards/repository_declaration/policies/rollback.toml" lang="toml" title="wyrd.toml" />

To stop automatic reviews without disconnecting the repository, declare an empty list:

<CodeFromFile file="fixtures/cards/repository_declaration/policies/empty.toml" lang="toml" title="wyrd.toml" />

<Aside type="note" title="Outside a checkout, or a different name">

Pass the remote with `wyrd declare wyrd.toml --repository https://github.com/acme/api.git`, or set the GitHub repository ID under `[repository.spec]` as `repository_id = "42"`. Set `name` under `[repository.metadata]` only before the first declaration: the Repository keeps its first name, through GitHub renames too. A Repository can also be a YAML Card with `kind: Repository` and the same `verified_by` list.

</Aside>

## 5. Watch a pull request run

Open a pull request on the repository. When GitHub delivers the event, Wyrd queues one execution per binding of the selected policy. Find the newest one from the binding the Repository lists under `status.verification.binding_ids`:

```bash
wyrd execution status --execution-id <execution_id>
```

Its `pull_request` field shows what the execution was pinned to when GitHub's delivery was accepted: the pull request `number`, the `base` and `head` branch and exact commit, the head repository (it differs for a fork), and the `delivery_id` that admitted it. The policy is always the repository's selected version at that moment, never `wyrd.toml` from the pull request's own branch.

Opening, reopening, marking ready for review, pushing commits and changing the base branch queue a new run. Draft pull requests and title or description edits queue nothing. When a newer run replaces an unfinished one, the older one ends `cancelled` with one of these error codes and keeps its pins:

| Code | Why |
| --- | --- |
| `pull_request_superseded` | A newer head or base was pushed |
| `pull_request_closed` | The pull request was closed or merged |
| `pull_request_draft` | The pull request was converted to a draft |
| `repository_access_lost` | The repository was removed from the installation, the installation was suspended, or the tenant disconnected GitHub |

Declaring a new policy changes only runs queued afterwards; runs already queued keep the policy they started with. Runs act as the principal that declared the selected policy: if it can no longer run Verifiers, each run ends `errored` with the permission refusal and nothing executes.

<Aside type="note" title="When GitHub reports a failed delivery">

Wyrd answers GitHub only after the run is durably queued, so an interrupted delivery shows as failed in the App's delivery log. The server checks that log every minute and asks GitHub to redeliver each failed delivery, up to three times. A redelivery reuses the run queued for the original delivery, so nothing runs twice. You can also select **Redeliver** on the delivery in the App's settings.

</Aside>

## If it fails

| You see | Fix |
| --- | --- |
| `WYRD_GITHUB_403_REPOSITORY_INACCESSIBLE` | Select the repository for the GitHub installation, then declare again |
| `WYRD_REGISTRY_409_REPOSITORY_IDENTITY_CONFLICT` | The repository is already declared under another name: remove `name` from `[repository.metadata]` |
| `WYRD_REGISTRY_409_SPEC_DRIFT` | A pinned `version` already holds other content: pin the content it holds, or remove `version` |
| `WYRD_REGISTRY_422_UNRESOLVED_DEPENDENCY` | Declare the referenced Verifier first, or list its file path instead |
| A pull request queues no run | Mark it ready for review if it is a draft, and check that the repository is still selected for the installation |

## Next

[Write a Verifier](/build-and-verify/verify/author-a-verifier/) for the checks your pull requests need.
