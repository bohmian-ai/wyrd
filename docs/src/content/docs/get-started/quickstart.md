---
title: Run your first verification
description: Verify a finance report against three requirements, including a model that reads a sharing policy PDF, in one call, and release it only when the verdict passes. Python, Rust, or TypeScript.
pillar: wyrd
section: Get started
order: 3
---

Your application is about to send a customer report to an outside auditor. Before it does, it asks Wyrd one question: does this release meet the requirements? Wyrd returns a verdict, and your code releases the report or holds it for review.

You build one Service, `finance-release`, verified by one Verifier, `release-check`, with three requirements:

| Requirement | How it is judged |
| --- | --- |
| `recipient-authorized` | An assertion on the recipient record your application supplies |
| `follows-policy` | A model reads the company sharing policy PDF and judges the report and its purpose |
| `destination-restricted` | An assertion on the destination record your application supplies |

One call judges all three. The reports, recipients, and destinations are synthetic, and nothing is sent anywhere: the program prints each decision.

## Prerequisites

- The server from [Set up Wyrd](/get-started/setup/), running in its own terminal, and the `quickstart` directory from setup with the SDK for your language installed.
- An OpenAI API key in `OPENAI_API_KEY`. The policy judge calls `gpt-4o`, which costs a few cents per run.

<Aside type="caution" title="What the model sees">

The policy judge sends the report text, the recipient record, and the policy text to OpenAI through your server. Use the synthetic data in this guide, not real customer data.

</Aside>

## 1. Connect a model provider

Skip this step if you accepted the gateway offer during `wyrd init`. Otherwise, run `wyrd` as in setup (`uv run wyrd` in Python, `pnpm wyrd` in TypeScript) to store your key on the server and create the `openai` deployment the judge calls:

```bash
wyrd gateway credential put --file - <<JSON
{"name": "openai-key", "provider": "openai", "source": {"managed_secret": {"secret": "$OPENAI_API_KEY"}}}
JSON
wyrd gateway deployment put --file - <<'JSON'
{"name": "openai", "adapter": "openai", "auth": {"bearer": {"credential": "openai-key"}}}
JSON
```

The key stays on the server. [LLM gateway](/connect/set-up-the-llm-gateway/) explains both commands.

## 2. Declare and verify

Download the sharing policy into `quickstart`:

```bash
curl -LO https://raw.githubusercontent.com/bohmian-ai/wyrd/v0.4.2/examples/finance-release/company-sharing-policy.pdf
```

Save the declaration and the program beside it, and run the program. The declaration builds the Service, the Verifier, and the judge Agent; the program declares them and verifies four releases. One report names a bank account, and two are shared for purposes other than an external audit:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

The program declares its inputs as Pydantic models. Add Pydantic with the SDK's extra:

```bash
uv add "wyrd-ai[pydantic]==0.4.2"
```

<CodeFromFile file="examples/finance-release/finance_service.py" lang="python" title="finance_service.py" />

<CodeFromFile file="examples/finance-release/finance_release.py" lang="python" title="finance_release.py" />

```bash
uv run finance_release.py
```

</LangTab>
<LangTab lang="rust">

<CodeFromFile file="examples/finance-release/finance_service.rs" lang="rust" title="src/finance_service.rs" />

<CodeFromFile file="examples/finance-release/finance_release.rs" lang="rust" title="src/main.rs" />

```bash
cargo run
```

</LangTab>
<LangTab lang="typescript">

The program declares its inputs as a Zod schema. Add Zod:

```bash
pnpm add zod
```

<CodeFromFile file="examples/finance-release/finance_service.ts" lang="typescript" title="finance_service.ts" />

<CodeFromFile file="examples/finance-release/finance_release.ts" lang="typescript" title="finance_release.ts" />

```bash
node finance_release.ts
```

</LangTab>
</LangTabs>

The program uploads the policy with its extracted text and declares the Service, the Verifier, and the judge Agent. Then it verifies each release with one call and prints the verdict, with the reason for each requirement that did not pass:

<CodeFromFile file="examples/finance-release/output.txt" lang="text" title="output" />

The judge's wording varies. The failed releases pass both assertions and fail `follows-policy`: the judge read the policy, found the bank account, and found that the policy permits sharing only for external audits.

`verify` returns before your code acts, so the verdict is part of the request. Assertions add little time; the policy judge adds up to three model calls to that request. If the judge can't decide, the verdict is `inconclusive`, which is never a pass. [`submit`](/understand/how-verification-works/#verify-and-submit) queues the same check when the request can't wait.

Run the program again. Nothing changed, so it declares nothing new.

## 3. Check the other requirements

In the program (`src/main.rs` in Rust), set the recipient's `authorized` to `false` and run again. Every release fails `recipient-authorized`. Set the destination's `restricted` to `false` instead, and `destination-restricted` fails. Each requirement is judged on its own. Set both back to `true` before you continue.

## 4. Confirm the stored verdicts

The server keeps every verdict as evidence. Read the Service to get its `uid`:

```bash
wyrd get --kind Service --name finance-release --version 0.1.0
```

Save this query as `stored-verdicts.sql`, with `<uid>` replaced by the Service's `metadata.uid`:

<CodeFromFile file="examples/finance-release/stored-verdicts.sql" lang="sql" title="stored-verdicts.sql" />

Run it against the address `wyrd init` printed:

```bash
wyrd query --server http://127.0.0.1:8080 --file stored-verdicts.sql
```

It prints one JSON row per stored verdict, newest first. The server publishes rows shortly after the program ends: if a verdict is missing, repeat the query for up to a minute.

Each row names its `execution_id`. To see the exact policy file a verdict used, read that execution's details: `Execution.details()` in the SDK, or [`verification.get_execution_details`](/reference/mcp-tools/) over MCP. They name the Verifier version and the policy Artifact version its judge read.

## 5. Update the policy

The revised policy also permits sharing for regulator requests. Download it over the original, and run the program again:

```bash
curl -L -o company-sharing-policy.pdf https://raw.githubusercontent.com/bohmian-ai/wyrd/v0.4.2/examples/finance-release/company-sharing-policy-revised.pdf
```

The server registers the changed file as a new policy version, and with it `release-check@0.1.1` and `finance-release@0.1.1`. The regulator request now passes:

```text
external audit · Synthetic customer report: quarterly total $120. → passed
external audit · Synthetic customer report: paid from BANK-ACCOUNT-DEMO-001. → failed
  follows-policy: the report names a bank account identifier, which the policy forbids
marketing partnership · Synthetic customer report: quarterly total $120. → failed
  follows-policy: the policy does not permit sharing for this purpose
regulator request · Synthetic customer report: quarterly total $120. → passed
```

The judge's wording varies. `release-check@0.1.0` still mounts the original policy, and the details of each execution it ran name that file.

## If it fails

| You see | Fix |
| --- | --- |
| `WYRD_GATEWAY_404_MODEL_UNAVAILABLE` | Create the `openai` deployment as in step 1 |
| `WYRD_CLIENT_401_NO_CREDENTIALS` | Use the same `WYRD_CONFIG_HOME` as the server terminal |
| `WYRD_LOADER_400_INVALID_ENVELOPE` | Run from the directory that holds `company-sharing-policy.pdf` |

## Next

[Real-time verification](/understand/real-time-verification/) explains what happens inside the call, and [Write verifiable requirements](/build-and-verify/declare/write-verifiable-requirements/) turns your own goals into requirements like these three. [ASGI apps](/connect/integrate-asgi/) puts the same call inside a FastAPI or Starlette request. [What can I build?](/get-started/what-can-i-build/) matches your problem to a larger workflow. The [Verifier reference](/reference/verifier-card/) lists every field you can declare.
