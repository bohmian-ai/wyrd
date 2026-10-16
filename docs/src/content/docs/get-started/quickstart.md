---
title: Verify your first agent
description: Run the support-desk example against your local server, get verdicts from two Verifiers, and query the evidence behind them.
pillar: wyrd
section: Get started
order: 3
draft: true
status: partial
---

You will run `support-desk`, a small customer-support agent, against the local server from [Set up Wyrd](/get-started/setup/). It answers 100 customer questions through `gpt-4o`. Wyrd checks every answer two ways: a rule your code calls before the answer goes out, and a judge Agent the server runs in the background. At the end you have a verdict for every answer, each tied to the exact agent version that produced it, and you can query the evidence with SQL.

The same example is in Python, Rust, and TypeScript. Pick one; the steps are identical.

## Prerequisites

- A local server running from [Set up Wyrd](/get-started/setup/), in another terminal. The example reads the endpoint and credential that setup saved, so you export nothing.
- An OpenAI API key with access to `gpt-4o`. One run makes about 200 calls: one per question, and one per answer for the judge.
- `git`, plus the toolchain for your language:

| Language | Needs |
| --- | --- |
| Python | Python 3.11+ and `pip install "wyrd-ai[otel]" pydantic`. The `otel` extra sends the example's trace spans to Wyrd |
| Rust | [mise](https://mise.jdx.dev); the repository pins the Rust version |
| TypeScript | [mise](https://mise.jdx.dev); the repository pins Node and pnpm |

## 1. Get the example

```bash
git clone https://github.com/bohmian-ai/wyrd.git
cd wyrd
```

Everything lives in `examples/support-desk/`:

| Path | Holds |
| --- | --- |
| `service/` | The Cards: the `support-desk` Service, its Agent and Prompt, and two Verifiers |
| `python/`, `rust/`, `typescript/` | The same program in each language |

## 2. Connect the model

The agent calls `gpt-4o` through Wyrd's gateway, so the server needs your OpenAI key and a deployment that routes to it. Store the key first. It goes to the server over stdin, never into a file or your shell history:

```bash
printf '{"name":"openai-key","provider":"openai","source":{"managed_secret":{"secret":"%s"}}}' \
  "$OPENAI_API_KEY" | wyrd gateway credential put --file -
```

Wyrd encrypts the key with the server's sealing key, and no read returns it. Now name an `openai` deployment on it; agents address its models as `openai/<model>`:

```bash
wyrd gateway deployment put --file - <<'JSON'
{
  "name": "openai",
  "adapter": "openai",
  "auth": { "bearer": { "credential": "openai-key" } }
}
JSON
```

Skip this step and the example stops before calling anything: `the support Agent's model openai/gpt-4o is not deployed`.

## 3. Read what is declared

The Service declaration is the contract. It names the agent, the table it records tickets into, and the Verifiers bound to the agent:

<CodeFromFile file="examples/support-desk/service/support-desk.yaml" lang="yaml" />

The two bindings differ in one line:

- `answer-quality` is judged in the background. Your code calls `submit` on each answer and moves on; the server queues the execution, and a tool-less judge Agent decides whether the answer is helpful and on-policy.
- `no-refund-promise` has no `runs_on`, so the server never runs it. Your code calls it on each answer before replying. It is an assertion: the answer must not contain `guaranteed refund`.

[How verification works](/understand/how-verification-works/) covers both paths in depth.

## 4. Run it

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```bash
python examples/support-desk/python/support_desk.py
```

</LangTab>
<LangTab lang="rust">

```bash
mise install
mise run examples:rust:support-desk
```

</LangTab>
<LangTab lang="typescript">

```bash
mise install
mise run examples:typescript:support-desk
```

</LangTab>
</LangTabs>

The program does four things in order:

1. **Deploy.** Registers the Cards under `service/`, which creates the `vala.datasets.tickets` table. It also turns on gateway metadata capture so each model call is recorded.
2. **Serve.** Answers 100 questions. Every tenth asks for a refund.
3. **Wait.** Polls until both Verifiers have judged all 100 answers, for up to 5 minutes.
4. **Explain.** Prints the joined evidence for one passing answer and, if there is one, one failing answer.

Each answer is one pass through this loop:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

<CodeFromFile file="examples/support-desk/python/support_desk.py" lang="python" range="122-138" title="support_desk.py" />

</LangTab>
<LangTab lang="rust">

<CodeFromFile file="examples/support-desk/rust/support_desk.rs" lang="rust" range="166-199" title="support_desk.rs" />

</LangTab>
<LangTab lang="typescript">

<CodeFromFile file="examples/support-desk/typescript/support-desk.ts" lang="typescript" range="109-131" title="support-desk.ts" />

</LangTab>
</LangTabs>

A run is one request handled by one exact Card version. Inside it the agent answers through the gateway, the ticket goes to your table, and `submit` queues `answer-quality` on the answer. Then `verify` asks `no-refund-promise` for a verdict and gets it back in the same call.

## 5. Read the verdicts

When every answer is judged, the program prints verdict counts per Verifier as `(passed, failed)`:

<Aside type="caution" title="Unverified">Output is captured on the first verified run.</Aside>

Don't expect any failures from `no-refund-promise`. `gpt-4o` almost never writes the words "guaranteed refund", so it passes every answer. The repository's journey tests use a scripted model that does promise refunds, and there 10 of 100 answers fail. The check is the same; only the model changed. `answer-quality` is a model judging a model, so its count can differ from run to run.

After the counts, the program prints one row of evidence per explained answer. That row joins four tables on the run ID: the ticket, the trace span, the gateway call, and both verdicts:

<Aside type="caution" title="Unverified">Output is captured on the first verified run.</Aside>

## 6. Query the evidence yourself

The verdicts are rows in Bifrost, Wyrd's warehouse, and they outlive the program. Query them from the CLI:

```bash
wyrd query --sql "SELECT card_uid, binding_id IS NOT NULL AS server_run, verdict, COUNT(*) AS n
  FROM vala.verification.results
  GROUP BY card_uid, binding_id IS NOT NULL, verdict"
```

Each line of output is one JSON object. `card_uid` is the Verifier that judged. Every verdict here came from your code, so `server_run` is false: `binding_id` is set only on verdicts a schedule produced. [Runs, observations, and retained evidence](/understand/runs-observations-and-evidence/#query-the-evidence) shows the joins from a verdict back to what ran.

<Aside type="caution" title="Unverified">Output is captured on the first verified run.</Aside>

## If it fails

| Symptom | Cause | Fix |
| --- | --- | --- |
| `the support Agent's model openai/gpt-4o is not deployed` | No gateway deployment serves `openai/gpt-4o` | Run [step 2](#2-connect-the-model) |
| `WYRD_CLIENT_401_NO_CREDENTIALS` | The program found no saved credential | Run it with the same `WYRD_CONFIG_HOME` as the server from setup |
| `WYRD_VALA_409_BIFROST_FINGERPRINT_MISMATCH` | A `vala.datasets.tickets` table with different columns already exists in this tenant | Use a fresh tenant, such as `wyrd server dev --tenant quickstart` |
| `verdicts still incomplete after 300 s` | The server has not judged every answer yet | Check the server terminal for gateway errors from OpenAI, such as an invalid key or rate limit |

## Clean up

Remove the deployment and the stored key:

```bash
wyrd gateway deployment delete gpt-4o
wyrd gateway credential delete openai-key
```

Then stop the server with Ctrl-C. Your Cards and verdicts stay in the database and `.wyrd/`; delete those as described in [Set up Wyrd](/get-started/setup/#stop-and-restart) to start over.

## Next

- [Verify a request in real time](/build-and-verify/verify/verify-a-request-in-real-time/): act on a failed verdict before the answer goes out.
- [Evaluate agent behavior](/build-and-verify/verify/evaluate-agent-behavior/): write your own eval Verifier.
- [Connect an agent through MCP](/connect/connect-an-agent-through-mcp/): ask your coding agent why an answer failed.
