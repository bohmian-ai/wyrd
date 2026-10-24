---
title: Run your first verification
description: Declare a Verifier and the Service it checks in two YAML files, then verify a passing and a failing support answer from one Python, Rust, or TypeScript program.
pillar: wyrd
section: Get started
order: 3
status: runnable
---

You write two small YAML files and one short program in Python, Rust, or TypeScript. The program declares them to your server, then asks Wyrd to verify two support answers against one requirement: the answer never promises a guaranteed refund. One answer passes and one fails, and each verdict names the exact versions that produced it.

No model, provider key, or application is involved: you supply each answer as text.

## Prerequisites

- The server from [Set up Wyrd](/get-started/setup/), running in its own terminal.
- The `quickstart` directory from setup, with the SDK for your language installed. Run everything below there, in a second terminal.

## 1. Declare the requirement

Save the Verifier in your `quickstart` directory as `no-refund-promise.yaml`. Its one requirement checks that the input named `answer` never contains `guaranteed refund`:

<CodeFromFile file="examples/quickstart/no-refund-promise.yaml" lang="yaml" title="no-refund-promise.yaml" />

Save the subject beside it as `support-reply.yaml`. It is a Service, and `verified_by` binds the Verifier to it by relative path:

<CodeFromFile file="examples/quickstart/support-reply.yaml" lang="yaml" title="support-reply.yaml" />

## 2. Verify two answers

Save the program and run it from the `quickstart` directory, so it finds both YAML files there:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

<CodeFromFile file="examples/quickstart/quickstart.py" lang="python" title="quickstart.py" />

```bash
uv run quickstart.py
```

</LangTab>
<LangTab lang="rust">

<CodeFromFile file="examples/quickstart/quickstart.rs" lang="rust" title="src/main.rs" />

```bash
cargo run
```

</LangTab>
<LangTab lang="typescript">

<CodeFromFile file="examples/quickstart/quickstart.ts" lang="typescript" title="quickstart.ts" />

```bash
node quickstart.ts
```

</LangTab>
</LangTabs>

`verify` sends each answer as the Verifier's `answer` input and returns its verdict. Every language prints the same:

```text
passed: the comparison held
  subject:  Service/support-reply@1.0.0#<uid>
  verifier: default/Verifier/no-refund-promise@1.0.0#<uid>
failed: operator NotContains returned false
  subject:  Service/support-reply@1.0.0#<uid>
  verifier: default/Verifier/no-refund-promise@1.0.0#<uid>
```

Each verdict names the exact version of the Service and Verifier it checked. Your `uid` values differ; the server assigns them.

The second verdict is `failed`, and it is still a successful call: the check ran, and the answer promises a guaranteed refund.

Run the program again. Nothing changed, so you get the same verdicts and `uid` values.

## 3. Confirm the stored verdicts

The server keeps every verdict as evidence, grouped by the Run that produced it. Run `wyrd` as in setup (`uv run wyrd` in Python, `pnpm wyrd` in TypeScript). Read the Service to get its `uid`, the one the verdicts printed after `#`:

```bash
wyrd get --kind Service --name support-reply --version 1.0.0
```

Its `metadata.uid` is the Service's `uid`. Save this query as `stored-verdicts.sql`, with `<uid>` replaced by that value:

<CodeFromFile file="examples/quickstart/stored-verdicts.sql" lang="sql" title="stored-verdicts.sql" />

Run it against the address `wyrd init` printed:

```bash
wyrd query --server http://127.0.0.1:8080 --file stored-verdicts.sql
```

It prints one JSON row per stored verdict, newest first:

```text
{"result_id":"<id>","run_id":"<run_id>","subject_card_uid":"<uid>","verdict":"failed"}
{"result_id":"<id>","run_id":"<run_id>","subject_card_uid":"<uid>","verdict":"passed"}
```

The `failed` and `passed` rows with the same `run_id` are your program's two verdicts, and that `run_id` identifies its Run. The server publishes rows shortly after the program ends: if a verdict is missing, repeat the query for up to a minute.

## 4. Change the requirement

Edit `no-refund-promise.yaml` to `expected: guaranteed` and run the program again. It stops with `WYRD_REGISTRY_409_SPEC_DRIFT`: `no-refund-promise@1.0.0` already means the declaration that checks `guaranteed refund`, and a version never changes meaning.

To change the requirement, give it a new version: set `version: 1.1.0` in `no-refund-promise.yaml`. The Service now references different content too, so set its `version: 1.1.0` as well. Run again, and the verdicts name `support-reply@1.1.0` and `no-refund-promise@1.1.0`.

## If it fails

| You see | Fix |
| --- | --- |
| `transport unavailable` | Start the server as in [Set up Wyrd](/get-started/setup/#1-start-the-server) |
| `WYRD_CLIENT_401_NO_CREDENTIALS` | Use the same `WYRD_CONFIG_HOME` as the server terminal |
| `WYRD_LOADER_400_INVALID_ENVELOPE` | Run from the directory that holds both YAML files |
| No rows after a minute | Check that `<uid>` is the `metadata.uid` that `wyrd get` printed |

## Next

[How verification works](/understand/how-verification-works/) explains requirements, checks, and verdicts. The [Verifier reference](/reference/verifier-card/) lists every field you can declare.
