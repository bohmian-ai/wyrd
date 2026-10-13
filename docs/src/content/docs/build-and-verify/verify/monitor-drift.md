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

A drift Verifier compares what a component sees in production with a baseline you register once. Bind it to a `schedule` Trigger and the server scores every window on its own, then runs your Operators when a feature shifts. The support desk has no trained model of its own, so this guide adds one: the `churn-classifier` from [Register Cards from code](/understand/cards-versions-and-relationships/#register-cards-from-code), which scores how likely a customer is to churn. Its inputs are checked against last quarter's healthy data every hour.

## Prerequisites

- The `support-desk` declaration from [Cards, versions, and relationships](/understand/cards-versions-and-relationships/), with `churn-classifier@1.4.2` registered from code, and `editor` to register Cards.
- A running service that records drift observations for the model, as in [Record a run and observations](/build-and-verify/observe/record-a-run-and-observations/).
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

```yaml title="service/churn-input-drift.yaml"
apiVersion: wyrd/v1
kind: Verifier
metadata:
  space: default
  name: churn-input-drift
  version: 1.0.0
spec:
  description: Churn model inputs stay distributed like the healthy baseline.
  implementation:
    kind: drift
    spec:
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

`condition` must be `Statistical`. A feature is drifted when its score exceeds the threshold, and the window fails if any feature drifted. For PSI, 0.1 is a small shift and 0.25 a large one; start at 0.25 and tighten once you know how noisy your inputs are.

## 3. Bind it to a schedule and register

Add the model to the Service as a new component and bind the Verifier to it:

```yaml title="service/support-desk.yaml (excerpt)"
- alias: churn_model
  ref: { kind: Model, name: churn-classifier, version: 1.4.2 }
  verified_by:
    - verifier: ./churn-input-drift.yaml
      runs_on: { kind: schedule, cron: "0 * * * *" }
      on_failure:
        - ./page-support-oncall.yaml
```

```bash
wyrd apply examples/support-desk/service/
```

The new component changes the Service's content, so bump `metadata.version` in `support-desk.yaml` first, or `wyrd apply` fails with `WYRD_REGISTRY_409_SPEC_DRIFT`.

`cron` is evaluated in UTC unless you set `tz`, such as `tz: America/Chicago`. Each due time scores the observations recorded since the previous one, so the hourly schedule above scores `[00:00, 01:00)` at 01:00.

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
use wyrd_sdk::cards::{CardKind, CardSelector, Cards};

let selector = CardSelector::named(CardKind::Verifier, "default".parse()?, "churn-input-drift".parse()?)
    .with_version("1.0.0".parse()?);
let verifier = Cards::from_env()?.get(selector).await?;
let baseline = verifier.status.and_then(|status| status.verification).and_then(|v| v.baseline);
println!("{:?}", baseline.map(|b| b.state)); // Pending, Building, Ready, or Failed
```

</LangTab>
<LangTab lang="typescript">

```typescript
import { Cards } from "@wyrd/sdk";

const verifier = await Cards.connect().get({
  kind: "Verifier",
  name: "churn-input-drift",
  version: "1.0.0",
  space: "default",
});
console.log(verifier.status?.verification?.baseline?.state); // pending, building, ready, or failed
```

</LangTab>
</LangTabs>

`failed` comes with `baseline.error` explaining why, usually a feature missing from the baseline or a text column under `Spc`, which only charts numbers. Fix the Card and register a new version. A real-time call before the baseline is `ready` returns `WYRD_VERIFICATION_409_BASELINE_NOT_READY`.

## 5. Record drift observations

Record one row of feature values per prediction, on the component the binding sits under:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```python
with state.run("churn_model") as run:
    features = {"tenure_months": customer.tenure_months, "monthly_spend": customer.monthly_spend}
    prediction = churn_classifier.predict(features)  # your model code
    run.observe.drift(features)
```

</LangTab>
<LangTab lang="rust">

```rust
let run = state.run_for_card("churn_model")?;
let features = json!({ "tenure_months": customer.tenure_months, "monthly_spend": customer.monthly_spend });
let prediction = churn_classifier.predict(&features); // your model code
run.observe().drift(&features, None)?;
```

</LangTab>
<LangTab lang="typescript">

```typescript
const run = state.run("churn_model");
const features = { tenure_months: customer.tenureMonths, monthly_spend: customer.monthlySpend };
const prediction = churnClassifier.predict(features); // your model code
run.observe.drift(features);
```

</LangTab>
</LangTabs>

Values must be flat: strings, booleans, finite floats, or integers.

A scheduled binding runs only while its Service is live. The server treats the binding as active when the Service's credential has authenticated in the last 24 hours, so a schedule on a Service that is not deployed does nothing. Missed windows, such as during a server outage, are skipped, not backfilled.

## 6. Read the result

Each scored window writes one row to `vala.verification.results` and one row per feature to `vala.drift.result_features`:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```python
from wyrd.bifrost import Bifrost

rows = Bifrost().sql(
    """
    SELECT window_start, window_end, feature, score, threshold, verdict
    FROM vala.drift.result_features
    WHERE binding_id = $1
    ORDER BY window_start DESC, feature
    LIMIT 20
    """,
    [binding_id],
)
print(rows.to_pandas())
```

</LangTab>
<LangTab lang="rust">

```rust
use wyrd_sdk::{Bifrost, bifrost::QueryParam};

let rows = Bifrost::from_env()
    .await?
    .sql(
        "SELECT window_start, window_end, feature, score, threshold, verdict \
         FROM vala.drift.result_features \
         WHERE binding_id = $1 \
         ORDER BY window_start DESC, feature \
         LIMIT 20",
        &[QueryParam::String(binding_id.to_string())],
    )
    .await?;
```

</LangTab>
<LangTab lang="typescript">

```typescript
import { Bifrost } from "@wyrd/sdk";

const bifrost = await Bifrost.connect();
const rows = await bifrost.sql(
  `SELECT window_start, window_end, feature, score, threshold, verdict
     FROM vala.drift.result_features
    WHERE binding_id = $1
    ORDER BY window_start DESC, feature
    LIMIT 20`,
  [bindingId],
);
```

</LangTab>
</LangTabs>

`binding_id` is in the Service's Card status (`status.verification.binding_ids`). A scheduled window has no `run_id`, because no application run produced it; filter on `binding_id` or `subject_card_uid` instead.

## Score a past window by hand

To check a window right now, such as the hour after a deploy, start a run yourself. There is no SDK method for this yet; use MCP or HTTP with a credential that holds `verifier:run`:

```json title="verification.start_run arguments"
{
  "target": { "kind": "binding", "binding_id": "<binding-id>" },
  "input": { "kind": "drift_window", "start": "2026-10-12T14:00:00Z", "end": "2026-10-12T15:00:00Z" }
}
```

The same body works for `POST /v1/verification/runs`. The window can span at most 31 days. A run that targets a binding behaves like a scheduled one: a `failed` verdict runs its Operators. To analyze without paging anyone, target the Verifier directly with `{ "kind": "verifier", "verifier_uid": "...", "subject_card_uid": "..." }`. [Run a verification and inspect its evidence](/build-and-verify/verify/run-a-verification-and-inspect-its-evidence/) covers following the run to its result.

## What can go wrong

| Symptom or code | Cause | Fix |
| --- | --- | --- |
| `WYRD_SPEC_400_TRIGGER_ACTIVATION_MISMATCH` at `wyrd apply` | `observations_ready` on a drift Verifier | Use `schedule` |
| Baseline stuck in `failed` | A scored feature is missing from the baseline, or is text under `Spc` | Read `baseline.error`, fix the Verifier or baseline, and register a new version |
| `WYRD_VERIFICATION_409_BASELINE_NOT_READY` | The baseline has not finished fitting | Wait for `ready` |
| No results on schedule | The Service has not authenticated in 24 hours, or no observations landed in the window | Deploy the Service; check `vala.drift.observations` for the window |
| Every window `inconclusive` | The window had no observations for the features, or too few to score | Record on the bound component's alias, with the feature names the Verifier lists |
| `WYRD_VERIFICATION_400_INVALID_WINDOW` on a manual run | The window is empty, inverted, or longer than 31 days | Send a shorter window with `start` before `end` |
