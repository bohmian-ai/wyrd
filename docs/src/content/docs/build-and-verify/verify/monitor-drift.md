---
title: Monitor drift
description: Compare the inputs your service sees with a healthy baseline on a schedule, and get paged when they shift.
pillar: wyrd
section: Build and verify
topic: Verify
order: 3
draft: true
status: runnable
---

A drift Verifier compares what a component sees in production with a baseline you register once. Your service records samples with `submit`; a `schedule` Trigger judges each window and runs your Operators when a feature shifts. Drift is never judged in real time. This guide checks a churn model's inputs against last quarter's healthy data every hour.

## Prerequisites

- A registered Model Card, here `churn-classifier@1.4.2` ([register one from code](/understand/cards-versions-and-relationships/#register-cards-from-code)), and `editor` to register Cards.
- A running service that loads its bundle, as in [Record a run and observations](/build-and-verify/observe/record-a-run-and-observations/).
- A sample of healthy inputs, with the same column names your service will record.

## 1. Register the baseline

The baseline is a Data Card. From Python, register a DataFrame of healthy inputs:

```python
import pandas as pd
from wyrd.cards import Cards
from wyrd.data import DataCard, PandasInterface

healthy = pd.read_parquet("churn_inputs_2026q2.parquet")[["tenure_months", "monthly_spend"]]
baseline = DataCard(
    PandasInterface(data=healthy),
    space="default",
    name="churn-baseline",
    version="1.0.0",
)
Cards().data.register(baseline)
```

Registration uploads the data and records its schema and statistics. Keep the columns to the features you will score: every feature the Verifier names must be a column here.

## 2. Write the Verifier

A drift Verifier has one `samples` slot naming the measurements it judges, and a `drift` check on that slot:

```yaml title="service/churn-input-drift.yaml"
apiVersion: wyrd/v1
kind: Verifier
metadata:
  space: default
  name: churn-input-drift
  version: 1.0.0
spec:
  description: Churn model inputs stay distributed like the healthy baseline.
  inputs:
    current: { type: samples, features: [tenure_months, monthly_spend] }
  requirements:
    - id: churn-inputs-stable
      statement: No churn model input drifted from the healthy baseline
      check:
        kind: drift
        on: inputs.current
        method: Psi
        signal:
          kind: Distribution
          baseline_ref: { kind: Data, name: churn-baseline, version: 1.0.0, space: default }
          features: [tenure_months, monthly_spend]
        condition: { kind: Statistical }
        profile:
          kind: Psi
          binning_strategy: { kind: Quantile, n_bins: 10 }
          threshold: { kind: Fixed, value: 0.25 }
```

Pick the method by what you are watching:

| `method` | Scores | `profile` |
| --- | --- | --- |
| `Psi` | How far each feature's distribution has moved from the baseline. The usual choice for model inputs | `binning_strategy` (`EqualWidth` or `Quantile`, 2 to 1000 bins) and `threshold`: `Fixed { value }`, or `ChiSquare { alpha }` / `Normal { alpha }` to derive one statistically |
| `Spc` | Control-chart signals on numeric features, for a value that should stay stable over time | `{ sample_size }`, at least 2 |
| `Custom` | One metric you compute against a fixed number | `{ metric_name, baseline_value, alert_threshold }`, with `signal: { kind: Metric, name }` and `metric_name` equal to that name. Needs no baseline Data Card |

`condition` must be `Statistical`. A feature is drifted when its score exceeds the threshold, and the requirement fails if any feature drifted. For PSI, 0.1 is a small shift and 0.25 a large one; start at 0.25 and tighten once you know how noisy your inputs are.

Registration refuses a Verifier that mixes `samples` slots or `drift` checks with any other slot or check type: a window Verifier and a per-run Verifier are judged on different evidence.

## 3. Bind it to a schedule and register

List the model as a component of a Service and bind the Verifier to it. The binding's `inputs` map tells the schedule which recorded measurements fill each `samples` slot; names are never inferred:

```yaml title="service/churn-service.yaml"
apiVersion: wyrd/v1
kind: Service
metadata: {name: churn-service, version: 1.0.0, space: default}
spec:
  service_type: model
  components:
    - alias: churn_model
      ref: {kind: Model, name: churn-classifier, version: 1.4.2}
      verified_by:
        - verifier: ./churn-input-drift.yaml
          runs_on: {kind: schedule, cron: "0 * * * *"}
          inputs:
            current: {features: [tenure_months, monthly_spend]}
```

A `Custom` slot reads one metric instead, such as `latency: {metric: latency_p95}`. Every required `samples` slot needs an entry, and the names must match the slot's `features`. To page someone when it fails, see [Respond to a failed verification](/build-and-verify/verify/respond-to-a-failed-verification/).

```bash
wyrd apply service/churn-service.yaml
```

`schedule` is the only Trigger that runs a Verifier on its own. `cron` is evaluated in UTC unless you set `tz`, such as `tz: America/Chicago`. Each due time judges the samples recorded since the previous one, so the hourly schedule above judges `[00:00, 01:00)` at 01:00.

## 4. Wait for the baseline to fit

After registration the server fits the baseline in the background. A PSI or SPC Verifier cannot score anything until that finishes. Check its state:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```python
from wyrd.cards import CardRef, Cards

verifier = Cards().get(CardRef(kind="Verifier", name="churn-input-drift", version="1.0.0", space="default"))
print(verifier.status.verification.baseline.state)  # pending, building, ready, or failed
```

</LangTab>
<LangTab lang="rust">

```rust
use wyrd_ai::cards::{CardKind, CardSelector, Cards};

let selector = CardSelector::named(CardKind::Verifier, "default".parse()?, "churn-input-drift".parse()?)
    .with_version("1.0.0".parse()?);
let verifier = Cards::connect().await?.get(selector).await?;
let baseline = verifier.status.and_then(|status| status.verification).and_then(|v| v.baseline);
println!("{:?}", baseline.map(|b| b.state)); // Pending, Building, Ready, or Failed
```

</LangTab>
<LangTab lang="typescript">

```typescript
import { Cards } from "wyrd-ai";

const verifier = await (await Cards.connect()).get({
  kind: "Verifier",
  name: "churn-input-drift",
  version: "1.0.0",
  space: "default",
});
console.log(verifier.status?.verification?.baseline?.state); // pending, building, ready, or failed
```

</LangTab>
</LangTabs>

`failed` comes with `baseline.error` explaining why, usually a feature missing from the baseline or a text column under `Spc`, which only charts numbers. Fix the Card and register a new version. A window judged before the baseline is `ready` is `inconclusive` and runs no Operator.

## 5. Record samples

Submit one row of feature values per prediction, on the component the binding sits under:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```python
with state.run("churn_model") as run:
    features = {"tenure_months": customer.tenure_months, "monthly_spend": customer.monthly_spend}
    prediction = churn_classifier.predict(features)  # your model code
    run.observe.submit("churn-input-drift", current=features)
```

</LangTab>
<LangTab lang="rust">

```rust
use wyrd_ai::verify::{Input, Inputs, Submission};

let run = state.run_for_card("churn_model")?;
let features = json!({ "tenure_months": customer.tenure_months, "monthly_spend": customer.monthly_spend });
let prediction = churn_classifier.predict(&features); // your model code
let submission = run
    .observe()
    .submit("churn-input-drift", Inputs::named([("current", Input::from(features))]))
    .await?;
assert!(matches!(submission, Submission::Recorded));
```

</LangTab>
<LangTab lang="typescript">

```typescript
const run = state.run("churn_model");
const features = { tenure_months: customer.tenureMonths, monthly_spend: customer.monthlySpend };
const prediction = churnClassifier.predict(features); // your model code
await run.observe.submit("churn-input-drift", { current: features });
```

</LangTab>
</LangTabs>

Values must be flat: strings, booleans, finite floats, or integers. `submit` writes the row through the same buffered observation path as other telemetry and returns `Recorded`, not an execution: there is nothing to wait on, because the schedule judges the window later. `verify` on this Verifier is refused with `WYRD_VERIFICATION_422_INPUT_UNSUPPORTED` before anything is sent.

A scheduled binding runs only while its Service is live. The server treats the binding as active when the Service's credential has authenticated in the last 24 hours, so a schedule on a Service that is not deployed does nothing. Missed windows, such as during a server outage, are skipped, not backfilled. Each window runs as the binding owner's authority.

## 6. Read the result

Each judged window writes one row to `vala.verification.results`, tied to the binding and the window:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```python
from wyrd.bifrost import Bifrost

rows = Bifrost().sql(
    """
    SELECT window_start, window_end, verdict, verifier_version
    FROM vala.verification.results
    WHERE binding_id = $1
    ORDER BY window_start DESC
    LIMIT 20
    """,
    [binding_id],
)
print(rows.to_pandas())
```

</LangTab>
<LangTab lang="rust">

```rust
use wyrd_ai::{Bifrost, bifrost::{BifrostOptions, QueryParam}};

let rows = Bifrost::connect(BifrostOptions::default())
    .await?
    .sql(
        "SELECT window_start, window_end, verdict, verifier_version \
         FROM vala.verification.results \
         WHERE binding_id = $1 \
         ORDER BY window_start DESC \
         LIMIT 20",
        &[QueryParam::String(binding_id.to_string())],
    )
    .await?;
```

</LangTab>
<LangTab lang="typescript">

```typescript
import { Bifrost } from "wyrd-ai";

const bifrost = await Bifrost.connect();
const rows = await bifrost.sql(
  `SELECT window_start, window_end, verdict, verifier_version
     FROM vala.verification.results
    WHERE binding_id = $1
    ORDER BY window_start DESC
    LIMIT 20`,
  [bindingId],
);
```

</LangTab>
</LangTabs>

`binding_id` is in the Service's Card status (`status.verification.binding_ids`). A scheduled window has no `run_id`, because no application run produced it; filter on `binding_id` or `card_uid` instead. The `churn-inputs-stable` requirement's `detail` holds every feature's score, threshold, sample counts, and drift flag, with the window and baseline it used.

## What can go wrong

| Symptom or code | Cause | Fix |
| --- | --- | --- |
| Registration refuses the Verifier | It mixes `samples` or `drift` with other slot or check types | Split it into a window Verifier and a per-run Verifier |
| Registration refuses the binding | A window Verifier without a `schedule` Trigger, or an `inputs` entry missing or naming other measurements than its slot | Bind with `runs_on: { kind: schedule, ... }` and one `inputs` entry per `samples` slot |
| `WYRD_VERIFICATION_422_INPUT_UNSUPPORTED` | `verify` on a window Verifier | Use `submit` |
| Baseline stuck in `failed` | A scored feature is missing from the baseline, or is text under `Spc` | Read `baseline.error`, fix the Verifier or baseline, and register a new version |
| No results on schedule | The Service has not authenticated in 24 hours, or no samples landed in the window | Deploy the Service; submit on the bound component's alias |
| Every window `inconclusive` | The baseline was not ready, or the window had too few samples to score | Wait for `ready`; read the requirement `reason` |
