---
title: Author in code
description: Build a Service, its Agent and Prompt, and a Verifier in Python, Rust, or TypeScript, declare them without versions, verify two answers, and read back the rows each Run wrote.
pillar: wyrd
section: Build and verify
topic: Declare
order: 3
---

Build your Cards in code instead of YAML files. One program defines a Service, the Agent it runs, and a Verifier bound to that Agent, declares them, verifies a passing and a failing answer, and records each answer in the Service's table.

You need the server and the `quickstart` directory from [Set up Wyrd](/get-started/setup/). Run everything below there.

## 1. Write the program

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

The Python program uses Pydantic models. Add them with the `pydantic` extra:

```bash
uv add "wyrd-ai[pydantic]==0.4.1"
```

Without Pydantic, map each input slot to its own schema, `inputs={"answer": {"type": "string"}}`. `Table` takes the row's whole object schema, and `write` takes a dictionary row.

<CodeFromFile file="examples/author-in-code/author.py" lang="python" title="author.py" />

</LangTab>
<LangTab lang="rust">

<CodeFromFile file="examples/author-in-code/author.rs" lang="rust" title="src/main.rs" />

</LangTab>
<LangTab lang="typescript">

<CodeFromFile file="examples/author-in-code/author.ts" lang="typescript" title="author.ts" />

</LangTab>
</LangTabs>

Every definition takes a name and no version. The Prompt goes directly into the Agent and registers with it. `inputs` names the Verifier's evidence slot, `answer`, and `verify` supplies the evidence under that name.

The component alias, `agent`, is how the program selects the Agent at runtime. The Verifier is bound to that component, so the program verifies on the Agent's view: `state.run("agent")`. `Table` declares `bifrost.datasets.refund_replies` with one column per schema property, in order, and declaration creates it.

The program supplies the answers itself and never calls the Agent's model, so it needs no gateway deployment.

## 2. Run it

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```bash
uv run author.py
```

</LangTab>
<LangTab lang="rust">

```bash
cargo run
```

</LangTab>
<LangTab lang="typescript">

```bash
node author.ts
```

</LangTab>
</LangTabs>

Every language prints the same:

```text
service: Service/refund-replies@0.1.0#<uid>
agent:   Agent/refund-agent@0.1.0#<uid>
passed: Your refund request is approved.
failed: You have a guaranteed refund.
```

The server selected each version at declaration, and the state `declare` returns holds the registered Cards at those exact versions. Your definitions stay unversioned; they are what you declared, not what the registry stored. To use Cards that are already declared, load them with `WyrdState.from_path` or reference them by exact `CardRef`.

The `failed` verdict is a result, not an error: the check ran, and the answer promises a guaranteed refund.

Run the program again. Nothing changed, so it prints the same versions and `uid` values.

## 3. Read back the rows

`run.bifrost.write` writes the Service's one declared table, so it names none. It queues the row and returns before it is stored. `shutdown` sends everything still queued, so a program that exits without it can lose rows.

Save this query as `replies.sql`:

<CodeFromFile file="examples/author-in-code/replies.sql" lang="sql" title="replies.sql" />

Run it against the address `wyrd init` printed, with `wyrd` run as in setup:

```bash
wyrd query --server http://127.0.0.1:8080 --file replies.sql
```

It prints one JSON row per answer, newest first:

```text
{"answer":"You have a guaranteed refund.","verdict":"failed","subject_card_uid":"<uid>","run_id":"<run_id>"}
{"answer":"Your refund request is approved.","verdict":"passed","subject_card_uid":"<uid>","run_id":"<run_id>"}
```

`subject_card_uid` is the `uid` the `agent:` line printed, and each answer has its own `run_id`. Rows become queryable shortly after the program ends: if one is missing, repeat the query for up to a minute.

## 4. Change the Verifier

In the program, change the checked text from `"guaranteed refund"` to `"guaranteed"` and run it again. The `service:` line shows a new version, and the `agent:` line is unchanged.

The Service binds the Verifier, so its content changed too: declaration gave the Verifier and the Service new versions. Verdicts from earlier runs still name the versions they checked.

<Aside type="note" title="Choose a version yourself">

Pass a version to a constructor to set it, as `metadata.version` does in YAML. [Cards and versions](/understand/cards-versions-and-relationships/#versions-are-exact) explains how chosen and selected versions behave.

</Aside>

## 5. Use Pydantic models (Python)

In Python, a Pydantic model class can be a slot's schema, and an instance of it is that slot's evidence. This Service has two components, each with its own Agent and Verifier, in separate modules. Its Agent checks call a model, so running it needs an `openai` deployment in the [LLM gateway](/connect/set-up-the-llm-gateway/).

In `quickstart`, create a `service` directory and save these five files in it, at the paths their tabs show:

<Tabs>
<Tab label="components/report_writer/agent.py">

<CodeFromFile file="examples/finance-release/service/components/report_writer/agent.py" lang="python" title="components/report_writer/agent.py" />

</Tab>
<Tab label="components/report_writer/verifier.py">

<CodeFromFile file="examples/finance-release/service/components/report_writer/verifier.py" lang="python" title="components/report_writer/verifier.py" />

</Tab>
<Tab label="components/release_reviewer/agent.py">

<CodeFromFile file="examples/finance-release/service/components/release_reviewer/agent.py" lang="python" title="components/release_reviewer/agent.py" />

</Tab>
<Tab label="components/release_reviewer/verifier.py">

<CodeFromFile file="examples/finance-release/service/components/release_reviewer/verifier.py" lang="python" title="components/release_reviewer/verifier.py" />

</Tab>
<Tab label="main.py">

<CodeFromFile file="examples/finance-release/service/main.py" lang="python" title="main.py" />

</Tab>
</Tabs>

- `inputs={"report": Report}` maps a slot to its model. `inputs=Inputs` passes one model whose fields are the slots.
- `ResponseFormat.verdict()` asks the judge for the standard verdict. `verdict(data=JudgeData)` adds a typed `data` field to it; `data` also takes a JSON Schema object, so Pydantic is optional.
- Importing a module only builds definitions. Nothing registers or runs until `main.py` calls `WyrdState.declare`.

From `service`, run:

```bash
uv run main.py
```

```text
Report quality: passed
Release controls: passed
```

`ComparisonOperator` and `ExecutionRoute` are typed forms of the YAML values, and `AgentRunConfig`, also from `wyrd.verify`, sets an Agent's run limits through `run_config`:

<CodeFromFile file="examples/finance-release/finance_release.py" region="run-config" lang="python" title="finance_release.py" />

The [Verifier reference](/reference/verifier-card/) lists every operator and field.

## If it fails

| You see | Fix |
| --- | --- |
| `WYRD_SDK_404_UNKNOWN_VERIFIER` | Verify on the view the Verifier is bound to: `state.run("agent")` |
| `WYRD_SDK_400_BIFROST_NOT_STARTED` | Call `start_bifrost` before the first `write` |
| `WYRD_VALA_409_BIFROST_FINGERPRINT_MISMATCH` | The table exists with different columns: keep its schema, or use a new table name |
| No rows after a minute | Check that the program reached `shutdown` without an error |

## Next

[Bindings](/build-and-verify/declare/bind-a-verifier/) covers where else a Verifier can attach and who runs it. [Runs and observations](/build-and-verify/observe/record-a-run-and-observations/) wraps real requests in Runs.
