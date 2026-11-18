---
title: Verify during a request
description: Call the Quickstart's release check from a web API, send a report only after its verdict passes, and hold every other outcome with the reason.
pillar: wyrd
section: Build and verify
topic: Verify
order: 2
---

Put the [Quickstart](/get-started/quickstart/)'s release check inside a web API, so each request is verified before the API sends anything.

## Before you start

Finish the Quickstart. Its `finance-release` Service, `release-check` Verifier, and the gateway deployment its judge calls must be declared. Work in the same `quickstart` directory.

## 1. Pull the Service

A deployed application loads the Service it serves from a bundle instead of declaring it. Pull the version the Quickstart declared:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```bash
uv run wyrd pull --kind Service --name finance-release --version 0.1.0 --output-dir .wyrd-bundle
```

</LangTab>
<LangTab lang="rust">

```bash
wyrd pull --kind Service --name finance-release --version 0.1.0 --output-dir .wyrd-bundle
```

</LangTab>
<LangTab lang="typescript">

```bash
pnpm wyrd pull --kind Service --name finance-release --version 0.1.0 --output-dir .wyrd-bundle
```

</LangTab>
</LangTabs>

## 2. Write the API

The handler verifies the release, waits for the verdict, and only then decides. It sends the report only when the verdict is `passed`; a `failed` or `inconclusive` verdict holds it for review, and so does an error, because nothing was judged:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```bash
uv add fastapi uvicorn
```

<CodeFromFile file="examples/finance-release/api.py" lang="python" title="api.py" />

</LangTab>
<LangTab lang="rust">

```bash
cargo add axum
```

<CodeFromFile file="examples/finance-release/api.rs" lang="rust" title="src/bin/api.rs" />

</LangTab>
<LangTab lang="typescript">

<CodeFromFile file="examples/finance-release/api.ts" lang="typescript" title="api.ts" />

</LangTab>
</LangTabs>

Each request opens its own Run. The response carries the Run's `run_id` and the `execution_id` of the verdict that decided it, so every decision can be traced to the judgment behind it.

## 3. Start the API

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```bash
uv run uvicorn api:api
```

</LangTab>
<LangTab lang="rust">

```bash
cargo run --bin api
```

</LangTab>
<LangTab lang="typescript">

```bash
node api.ts
```

</LangTab>
</LangTabs>

The API listens on `http://127.0.0.1:8000`.

## 4. Send two releases

Save a permitted release and one whose report names a bank account:

<CodeFromFile file="examples/finance-release/permitted.json" lang="json" title="permitted.json" />

<CodeFromFile file="examples/finance-release/bank-account.json" lang="json" title="bank-account.json" />

In a second terminal, send each one:

```bash
curl -s localhost:8000/reports/send -H 'content-type: application/json' -d @permitted.json; echo
curl -s localhost:8000/reports/send -H 'content-type: application/json' -d @bank-account.json; echo
```

<CodeFromFile file="examples/finance-release/api-output.txt" lang="text" title="output" />

Your IDs differ. The permitted report is released. The bank-account report is held, and the response names `follows-policy`, the requirement it failed. An `inconclusive` verdict holds the same way, with `"verdict":"inconclusive"`.

When the call itself fails, such as a credential without `verifier:run`, the API answers `503` with the error code and sends nothing:

```json
{"decision":"held for review","error":"WYRD_PERMISSION_403_DENIED_RBAC"}
```

<Aside type="note" title="Checks that can wait">
A request waits for every check in the Verifier, including the model judge. When a decision can be made after the response, `submit` the Verifier instead and read the result later, as in [Record a run and observations](/build-and-verify/observe/record-a-run-and-observations/#3-verify-the-answer). [Fit checks to the request's deadline](/build-and-verify/verify/act-on-verdicts-in-production/#2-fit-checks-to-the-requests-deadline) covers the trade-off.
</Aside>

## If it fails

| You see | Fix |
| --- | --- |
| The API exits on startup with a bundle error | Pull the Service into `.wyrd-bundle` in the directory you start the API from |
| `"error":"WYRD_PERMISSION_403_DENIED_RBAC"` | Use a credential with `verifier:run`, such as the one `wyrd init` saved |
| Every release is `inconclusive` on `follows-policy` | The judge cannot reach its model. Create the `openai` deployment as in [Quickstart step 1](/get-started/quickstart/#1-connect-a-model-provider) |

## Next

[Act on verdicts in production](/build-and-verify/verify/act-on-verdicts-in-production/) covers timeouts, provider exposure, and cost for this handler. [ASGI apps](/connect/integrate-asgi/) records the request itself as evidence. [Verify an agent's workflow](/build-and-verify/verify/verify-an-agent-workflow/) checks the steps behind an answer, not only the answer.
