---
title: Run your first verification
description: Verify a finance report against three requirements, including a model that reads a policy PDF, in one call, and release it only when the verdict passes. Python, Rust, or TypeScript.
pillar: wyrd
section: Get started
order: 3
---

Your application is about to send a customer report to an outside auditor. Before it does, it asks Wyrd one question: does this release meet the requirements? Wyrd returns a verdict, and your code releases the report or holds it for review.

You build one Service, `finance-release`, verified by one Verifier, `release-check`, with three requirements:

| Requirement | How it is judged |
| --- | --- |
| `recipient-authorized` | An assertion on the recipient record your application supplies |
| `follows-policy` | A model reads the company sharing policy PDF and judges the report |
| `destination-restricted` | An assertion on the destination record your application supplies |

One call judges all three. The reports, recipients, and destinations are synthetic, and nothing is sent anywhere: the program prints what it would do.

## Prerequisites

- The server from [Set up Wyrd](/get-started/setup/), running in its own terminal, and the `quickstart` directory from setup with the SDK for your language installed.
- An OpenAI API key in `OPENAI_API_KEY`. The policy judge calls `gpt-4o`, which costs a few cents per run.

<Aside type="caution" title="What the model sees">

The policy judge sends the report text, the recipient record, and the policy's extracted text to OpenAI through your server. Use the synthetic data in this guide, not real customer data.

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

## 2. Save the policy and the releases

Download the company sharing policy into `quickstart`:

```bash
curl -LO https://raw.githubusercontent.com/bohmian-ai/wyrd/main/examples/finance-release/company-sharing-policy.pdf
```

It allows reports to be shared only for external audits and forbids bank account identifiers such as `BANK-ACCOUNT-DEMO-001`.

Save the two releases your application proposes as `releases.json`. The second report names a bank account:

<CodeFromFile file="examples/finance-release/releases.json" lang="json" title="releases.json" />

## 3. Verify each release

Save the program in `quickstart` and run it:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

The program declares its inputs as Pydantic models. Add Pydantic with the SDK's extra:

```bash
uv add "wyrd-ai[pydantic]==0.4.1"
```

<CodeFromFile file="examples/finance-release/finance_release.py" lang="python" title="finance_release.py" />

```bash
uv run finance_release.py
```

</LangTab>
<LangTab lang="rust">

<CodeFromFile file="examples/finance-release/finance_release.rs" lang="rust" title="src/main.rs" />

```bash
cargo run
```

</LangTab>
<LangTab lang="typescript">

<CodeFromFile file="examples/finance-release/finance_release.ts" lang="typescript" title="finance_release.ts" />

```bash
node finance_release.ts
```

</LangTab>
</LangTabs>

The program uploads the policy and declares the Service, the Verifier, and the judge Agent. Then it verifies each release with one call and prints its decision:

```text
quarterly-report: passed (release-check@0.1.0) -> released
report-with-bank-account: failed (release-check@0.1.0) -> held for review
  follows-policy failed: the report names a bank account identifier
```

The second release passes both assertions and fails `follows-policy`: the judge read the policy and found the bank account. Its reason is in the model's own words, so yours differs.

Only a `passed` verdict releases a report. A `failed` or `inconclusive` verdict, or an error such as evidence that does not match the Verifier's inputs, holds it. Wyrd never sends or blocks anything itself; your code decides.

Run the program again. Nothing changed, so it declares nothing new and the verdicts name the same `release-check@0.1.0`.

## 4. Check the other requirements

Edit `releases.json` so the first release goes to `"authorized": false`, and run again. `recipient-authorized` fails and the release is held. Set `"restricted": false` on a destination, and `destination-restricted` fails instead. Each requirement is judged on its own, so the verdict names the one that failed.

## 5. Confirm the stored verdicts

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

## 6. Update the policy

Replace `company-sharing-policy.pdf` with a revised policy and run the program again. The server registers the new file as a new policy version, and with it new versions of `release-check` and `finance-release`, so the verdicts name `release-check@0.1.1`. Verdicts from `0.1.0` still name the policy they were judged against.

## If it fails

| You see | Fix |
| --- | --- |
| `WYRD_GATEWAY_404_MODEL_UNAVAILABLE` | Create the `openai` deployment as in step 1 |
| `WYRD_LOADER_400_INVALID_ENVELOPE` | Run from the directory that holds `company-sharing-policy.pdf` |
| `WYRD_CLIENT_401_NO_CREDENTIALS` | Use the same `WYRD_CONFIG_HOME` as the server terminal |
| `follows-policy` is `inconclusive` | The judge could not settle the release; check the reason it gave |

## Next

[How verification works](/understand/how-verification-works/) explains requirements, checks, and verdicts. [ASGI apps](/connect/integrate-asgi/) puts the same call inside a FastAPI or Starlette request. [What can I build?](/get-started/what-can-i-build/) matches your problem to a larger workflow. The [Verifier reference](/reference/verifier-card/) lists every field you can declare.
