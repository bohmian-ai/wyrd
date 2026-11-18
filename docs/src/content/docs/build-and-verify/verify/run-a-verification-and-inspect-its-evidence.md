---
title: Investigate a failed verification
description: Find a stored failed result, read which requirement failed, the evidence it judged, and the exact Verifier and subject versions, without rerunning any check.
pillar: wyrd
section: Build and verify
topic: Verify
order: 3
---

Start from a stored failure and find out why it failed: which requirement, against what evidence, judged by which versions.

The result `verify` returns is compact: the verdict and each requirement's outcome. Wyrd also keeps the execution's details, which hold each check as written, the value it found, the pinned evidence, and the exact Cards involved. Reading them does not run any check again.

## Before you start

Finish [Verify an agent's workflow](/build-and-verify/verify/verify-an-agent-workflow/) in the same directory. Its `skipped` case stored a failed `refund-workflow` result.

## 1. Write the investigation

The program finds the newest failed `refund-workflow` result in `bifrost.verification.results`, then reads its execution's details and prints each requirement that did not pass:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

<CodeFromFile file="examples/refund-agent/investigate.py" lang="python" title="investigate.py" />

</LangTab>
<LangTab lang="rust">

<CodeFromFile file="examples/refund-agent/investigate.rs" lang="rust" title="src/bin/investigate.rs" />

</LangTab>
<LangTab lang="typescript">

The query parses its rows with Zod. Add it:

```bash
pnpm add zod
```

<CodeFromFile file="examples/refund-agent/investigate.ts" lang="typescript" title="investigate.ts" />

</LangTab>
</LangTabs>

A result becomes queryable shortly after its check, not at the moment `verify` returns, so the program looks for up to a minute. A result that is not visible yet is not treated as a pass or a failure.

## 2. Run it

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```bash
uv run investigate.py
```

</LangTab>
<LangTab lang="rust">

```bash
cargo run --bin investigate
```

</LangTab>
<LangTab lang="typescript">

```bash
node investigate.ts
```

</LangTab>
</LangTabs>

<CodeFromFile file="examples/refund-agent/investigate-output.txt" lang="text" title="output" />

## 3. Read the result

Each line answers one question:

- **What judged what.** `refund-workflow@0.1.0` judged `refund-agent@0.1.0`. These are exact versions: a later version of either Card does not change this record.
- **Which requirement failed.** `looked-up-order`. `reply-names-order` passed, so it is not printed.
- **What it found.** The check expected `look-up-order` among the trace's span names, and the stored trace held only `answer-refund` and `draft-reply`. The agent never ran its lookup.

The details also hold the pinned evidence under `outcome.evidence`: the reply text and the trace ID the check read. Over MCP, the same read is [`verification.get_execution_details`](/reference/mcp-tools/).

A read sees only executions in the caller's own tenant. An unknown execution and one in another tenant both answer `WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND`, so a refused read reveals nothing about other tenants.

## If it fails

| You see | Fix |
| --- | --- |
| `no failed refund-workflow result yet` | Run the refund agent from [Verify an agent's workflow](/build-and-verify/verify/verify-an-agent-workflow/) first, against the same server |
| `WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND` | Read with a credential from the tenant that ran the check, such as the one `wyrd init` saved |

## Next

[What Wyrd retains](/understand/how-verification-works/#what-wyrd-retains-and-how-requirements-change) explains which tables hold results, inputs, and outputs. [Bifrost](/understand/bifrost/) covers querying them with SQL.
