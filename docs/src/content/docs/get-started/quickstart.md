---
title: Run your first verification
description: Verify a finance report against three requirements, including a model that reads a sharing policy, in one call, and release it only when the verdict passes. Python, Rust, or TypeScript.
pillar: wyrd
section: Get started
order: 3
---

Your application is about to send a customer report to an outside auditor. Before it does, it asks Wyrd one question: does this release meet the requirements? Wyrd returns a verdict, and your code releases the report or holds it for review.

You build one Service, `finance-release`, verified by one Verifier, `release-check`, with three requirements:

| Requirement | How it is judged |
| --- | --- |
| `recipient-authorized` | An assertion on the recipient record your application supplies |
| `follows-policy` | A model reads the company sharing policy and judges the report |
| `destination-restricted` | An assertion on the destination record your application supplies |

One call judges all three. The reports, recipients, and destinations are synthetic, and nothing is sent anywhere: the program prints each verdict.

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

Save the declaration and the program in `quickstart`, and run the program. The declaration builds the Service, the Verifier, and the judge Agent; the program declares them and verifies two releases. The second report names a bank account:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

The program declares its inputs as Pydantic models. Add Pydantic with the SDK's extra:

```bash
uv add "wyrd-ai[pydantic]==0.4.1"
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

<CodeFromFile file="examples/finance-release/finance_service.ts" lang="typescript" title="finance_service.ts" />

<CodeFromFile file="examples/finance-release/finance_release.ts" lang="typescript" title="finance_release.ts" />

```bash
node finance_release.ts
```

</LangTab>
</LangTabs>

The program saves the policy as `company-sharing-policy.md`, uploads it, and declares the Service, the Verifier, and the judge Agent. Then it verifies each release with one call and prints its verdict:

```text
passed: Synthetic customer report: quarterly total $120.
failed: Synthetic customer report: paid from BANK-ACCOUNT-DEMO-001.
```

The second release passes both assertions and fails `follows-policy`: the judge read the policy and found the bank account.

Only a `passed` verdict releases a report. Wyrd never sends or blocks anything itself; your code decides.

Run the program again. Nothing changed, so it declares nothing new.

## 3. Check the other requirements

In `finance_release`, set the recipient's `authorized` to `false` and run again. Both releases fail `recipient-authorized`. Set the destination's `restricted` to `false` instead, and `destination-restricted` fails. Each requirement is judged on its own.

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

## 5. Update the policy

Edit `company-sharing-policy.md` and run the program again. The server registers the edited file as a new policy version, and with it new versions `release-check@0.1.1` and `finance-release@0.1.1`. `release-check@0.1.0` still mounts the policy it judged against.

## If it fails

| You see | Fix |
| --- | --- |
| `WYRD_GATEWAY_404_MODEL_UNAVAILABLE` | Create the `openai` deployment as in step 1 |
| `WYRD_CLIENT_401_NO_CREDENTIALS` | Use the same `WYRD_CONFIG_HOME` as the server terminal |

## Next

[How verification works](/understand/how-verification-works/) explains requirements, checks, and verdicts. [ASGI apps](/connect/integrate-asgi/) puts the same call inside a FastAPI or Starlette request. [What can I build?](/get-started/what-can-i-build/) matches your problem to a larger workflow. The [Verifier reference](/reference/verifier-card/) lists every field you can declare.
