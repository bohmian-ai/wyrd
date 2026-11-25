---
title: Check on a schedule
browserTitle: Continuous verification on a schedule | Wyrd
description: Have the Wyrd server judge a Service's recorded samples every minute, then read which windows passed, failed, or had no evidence.
pillar: wyrd
section: Build and verify
topic: Verify
order: 4
---

Have the server check a checkout Service's latency on a schedule, with no call from your application.

The application only records samples. A `schedule` Trigger tells the server when to judge them: each run judges the samples recorded since the previous one, called a window.

## Before you start

Finish [Set up Wyrd](/get-started/setup/). Work in its `quickstart` directory with the server running.

## 1. Declare the check

Save three Cards: the `checkout` Service, the `latency-budget` Verifier it is checked by, and the `every-minute` Trigger that runs it.

<CodeFromFile file="examples/scheduled-check/checkout/checkout.yaml" lang="yaml" title="checkout/checkout.yaml" />

<CodeFromFile file="examples/scheduled-check/checkout/latency-budget.yaml" lang="yaml" title="checkout/latency-budget.yaml" />

<CodeFromFile file="examples/scheduled-check/checkout/every-minute.yaml" lang="yaml" title="checkout/every-minute.yaml" />

The check fails a window whose mean latency is above 100 ms. Declare the Cards and pull the Service, as a deployment does:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```bash
uv run wyrd declare checkout/checkout.yaml
uv run wyrd pull --kind Service --name checkout --version 1.0.0 --output-dir .wyrd-bundle
```

</LangTab>
<LangTab lang="rust">

```bash
wyrd declare checkout/checkout.yaml
wyrd pull --kind Service --name checkout --version 1.0.0 --output-dir .wyrd-bundle
```

</LangTab>
<LangTab lang="typescript">

```bash
pnpm wyrd declare checkout/checkout.yaml
pnpm wyrd pull --kind Service --name checkout --version 1.0.0 --output-dir .wyrd-bundle
```

</LangTab>
</LangTabs>

## 2. Create the Service's account

A schedule runs as the Service's own account, and only while that account has recorded for the Service in the last 24 hours. Create it:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```bash
uv run wyrd principal credential create-principal --name checkout-runtime --grant workload:all --workload-name checkout
```

</LangTab>
<LangTab lang="rust">

```bash
wyrd principal credential create-principal --name checkout-runtime --grant workload:all --workload-name checkout
```

</LangTab>
<LangTab lang="typescript">

```bash
pnpm wyrd principal credential create-principal --name checkout-runtime --grant workload:all --workload-name checkout
```

</LangTab>
</LangTabs>

Copy the credential it prints into an environment variable for the next step:

```bash
export CHECKOUT_KEY=<credential>
```

## 3. Write the programs

`record` records five checkout requests that each took the number of milliseconds you pass. `results` prints the newest three windows the server judged.

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

<CodeFromFile file="examples/scheduled-check/record.py" lang="python" title="record.py" />

<CodeFromFile file="examples/scheduled-check/results.py" lang="python" title="results.py" />

</LangTab>
<LangTab lang="rust">

<CodeFromFile file="examples/scheduled-check/record.rs" lang="rust" title="src/bin/record.rs" />

<CodeFromFile file="examples/scheduled-check/results.rs" lang="rust" title="src/bin/results.rs" />

</LangTab>
<LangTab lang="typescript">

`results.ts` parses its rows with Zod. Add it:

```bash
pnpm add zod
```

<CodeFromFile file="examples/scheduled-check/record.ts" lang="typescript" title="record.ts" />

<CodeFromFile file="examples/scheduled-check/results.ts" lang="typescript" title="results.ts" />

</LangTab>
</LangTabs>

## 4. Record a fast minute and a slow one

Record fast requests as the Service, wait for the next minute to start, then record slow ones:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```bash
WYRD_API_KEY=$CHECKOUT_KEY uv run record.py 80
# wait for the next minute
WYRD_API_KEY=$CHECKOUT_KEY uv run record.py 250
```

</LangTab>
<LangTab lang="rust">

```bash
WYRD_API_KEY=$CHECKOUT_KEY cargo run --bin record 80
# wait for the next minute
WYRD_API_KEY=$CHECKOUT_KEY cargo run --bin record 250
```

</LangTab>
<LangTab lang="typescript">

```bash
WYRD_API_KEY=$CHECKOUT_KEY node record.ts 80
# wait for the next minute
WYRD_API_KEY=$CHECKOUT_KEY node record.ts 250
```

</LangTab>
</LangTabs>

```text
recorded 5 requests at 80 ms
recorded 5 requests at 250 ms
```

`submit` returns once the samples are recorded. Nothing is judged yet: the server judges each minute when it ends.

## 5. Read the results

Wait two more minutes, so the window after the slow one passes with nothing recorded, then read the results with your own credential:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```bash
uv run results.py
```

</LangTab>
<LangTab lang="rust">

```bash
cargo run --bin results
```

</LangTab>
<LangTab lang="typescript">

```bash
node results.ts
```

</LangTab>
</LangTabs>

<CodeFromFile file="examples/scheduled-check/results-output.txt" lang="text" title="output" />

Your times differ. Each line is one window:

- **passed**: the fast requests averaged 80 ms, within the budget.
- **failed**: the slow requests averaged 250 ms.
- **inconclusive**: nothing was recorded, so there was no evidence to judge. An inconclusive window is never a pass.

Each stored result names the exact Verifier and Service versions it judged.

<Aside type="note" title="Choose a real schedule">

A minute keeps this guide short. A production check usually runs hourly or daily, such as `cron: "0 * * * *"`. `cron` is evaluated in UTC unless the Trigger sets `tz`. A window missed while the server was down is skipped, not judged later.

</Aside>

## If it fails

| You see | Fix |
| --- | --- |
| `results` prints no windows | Record with `CHECKOUT_KEY`, the Service's account, not your own credential. The schedule runs only while that account records |
| The fast window is `inconclusive` | The samples landed in the next minute. Wait for the next window and read again |

## Next

[Investigate a failed verification](/build-and-verify/verify/run-a-verification-and-inspect-its-evidence/) reads why a result failed. The [Verifier reference](/reference/verifier-card/#checks) lists the other drift methods, which compare samples with a baseline.
