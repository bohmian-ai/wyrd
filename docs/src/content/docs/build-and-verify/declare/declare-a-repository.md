---
title: Repositories
description: Declare a connected GitHub repository's review policy from wyrd.toml, check it offline, and roll it back or empty it.
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

## If it fails

| You see | Fix |
| --- | --- |
| `WYRD_GITHUB_403_REPOSITORY_INACCESSIBLE` | Select the repository for the GitHub installation, then declare again |
| `WYRD_REGISTRY_409_REPOSITORY_IDENTITY_CONFLICT` | The repository is already declared under another name: remove `name` from `[repository.metadata]` |
| `WYRD_REGISTRY_409_SPEC_DRIFT` | A pinned `version` already holds other content: pin the content it holds, or remove `version` |
| `WYRD_REGISTRY_422_UNRESOLVED_DEPENDENCY` | Declare the referenced Verifier first, or list its file path instead |

## Next

[Write a Verifier](/build-and-verify/verify/author-a-verifier/) for the checks your pull requests need.
