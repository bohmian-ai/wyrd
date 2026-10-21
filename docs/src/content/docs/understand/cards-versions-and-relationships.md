---
title: Cards, versions, and relationships
description: A Service declaration is a set of versioned Cards, each one a defined component of your system.
pillar: wyrd
section: Understand
order: 1
---

A Card is a versioned declaration: one model, prompt, agent, or dataset in your system, or one rule for checking them. You declare a whole service as a folder of Card files in YAML, whatever language the service is written in, and a Service Card ties the set together by naming its components.

Wyrd has 14 Card kinds. These are the ones a service declaration uses most:

| Card | Declares |
| --- | --- |
| `Service` | The deployed service and the components it is built from |
| `Agent` | An agent: its prompt, model, and the tools it may call |
| `Prompt` | A versioned prompt template |
| `Model` | A trained model and its artifact |
| `Data` | A dataset, such as a training set or a baseline |
| `Verifier` | A check on how a component behaves |
| `Trigger` | When a Verifier runs |
| `Operator` | What happens when a check fails |

Not every Card starts in YAML. In a traditional ML workflow, you declare `Data` and `Model` Cards from code as they are produced, with the Python SDK.

The examples on this page use the support-desk Service in `examples/support-desk-app/service/`: an agent that answers customer questions, the prompt it runs, three Verifiers that check it, and the judge Agent one of those Verifiers calls.

## Every Card has the same shape

Each Card is a YAML document with six top-level fields. You write the first four; the server writes the rest. Switch tabs to see how the Service, its Agent, and the Prompt fit together:

<Tabs>
<Tab label="support-desk.yaml">

<CodeFromFile file="examples/support-desk-app/service/support-desk.yaml" lang="yaml" title="service/support-desk.yaml" />

</Tab>
<Tab label="support-agent.yaml">

<CodeFromFile file="examples/support-desk-app/service/support-agent.yaml" lang="yaml" title="service/support-agent.yaml" />

</Tab>
<Tab label="support-prompt.yaml">

<CodeFromFile file="examples/support-desk-app/service/support-prompt.yaml" lang="yaml" title="service/support-prompt.yaml" />

</Tab>
</Tabs>

| Field | Who writes it | Holds |
| --- | --- | --- |
| `apiVersion` | You | Always `wyrd/v1`; a Card without it is rejected |
| `kind` | You | One of the Card kinds above |
| `metadata` | You | `space`, `name`, and optionally `version`, `bump`, `labels`, and `annotations` |
| `spec` | You | The kind-specific body |
| `relationships` | The server | `outbound_refs` and `inbound_refs`: every Card this one references and every Card that references it |
| `status` | The server | `phase` (`pending`, `active`, `deprecated`, `failed`, `expired`, or `deleted`), `message`, `updated_at`, and `verification`, the state of its bindings and Triggers |

A Card is identified by `space`, `kind`, `name`, and `version`. A `space` groups related Cards, such as a team or a domain. Leave it out and the CLI and SDKs fill it in from `wyrd.toml`, [`[kind.<Kind>]` before `[defaults]`](/build-and-verify/declare/define-a-service/#set-the-space-once), or use `default`.

## A Service names its components

The Service Card is the root of the declaration. Each component gets an `alias`, the name your code uses to reach it at runtime, such as `run.for_card("agent")`. `verified_by` binds Verifiers: on the Service itself, or on one component. `tables` declares `vala.datasets.tickets`, which registration creates in Bifrost.

Declaring `support-desk.yaml` declares every Card it reaches, dependencies first. One root is required: Cards nothing references are allowed only if exactly one of them is a Service, and with a Service root every Verifier in the folder must be bound. [Define a Service](/build-and-verify/declare/define-a-service/) and [Declare and inspect](/build-and-verify/declare/declare-and-inspect/) walk through it.

## Three ways to reference another Card

Wherever one Card points at another, you can write the reference three ways.

| Form | Example | Use it when |
| --- | --- | --- |
| Declared Card | `ref: {kind: Model, name: churn-classifier, version: 1.4.2}` | The Card already exists in the registry |
| File path | `ref: ./support-agent.yaml` | The Card file is in your declaration; Wyrd declares it first |
| Inline | `prompt: {provider: openai, model: gpt-6-luna, messages: [...]}` | The body only makes sense inside its parent |

Rules for each form:

| Form | Rule |
| --- | --- |
| Declared Card | Names an exact `version`, such as `1.4.2` |
| File path | Relative to the file that contains it, not to where you run `wyrd declare`; the Card in that file sets its own `metadata.version` |
| Inline | Written directly under the field, with no wrapper key; allowed only for an Agent's `prompt`, a Verifier's `agent`, and a binding's `runs_on` and `on_failure` |

An inline body gets no identity of its own, with one exception: an Agent's inline `prompt` is declared as a Prompt Card named `<agent-name>-prompt`, in the Agent's space, so `state.agent(alias).prompt()` returns it like any other Prompt Card.

## Versions are exact

Every reference names an exact version, so a declared Service records exactly which components it was built from. On the Card itself, `version` is optional:

| You write in `metadata` | Wyrd records |
| --- | --- |
| No `version`, first declaration | `0.1.0` |
| No `version`, content changed | The next patch after the latest declared version |
| No `version`, content unchanged | Nothing new; the existing version stands |
| `version: 1` or `version: 1.2` | The next version within that line, or `1.0.0` / `1.2.0` if the line is empty |
| `version: 2.1.0` | Exactly `2.1.0`; re-declaring it with different content fails with `WYRD_REGISTRY_409_SPEC_DRIFT` |
| `bump: minor` with no `version` or a partial one | Bumps that part instead of the patch; `major`, `minor`, and `patch` (the default) are allowed, and an exact `version` refuses it |

So `support-desk@1.0.0` is stored with `support-agent@1.0.0`, `support-prompt@1.0.0`, and the exact Verifier versions it binds. The same holds for a declared reference: if the desk referenced `churn-classifier@1.4.2`, declaring `churn-classifier@1.5.0` later would not change the Service. To pick up the new model, change the reference to `1.5.0` and re-apply; the Service content changed, so Wyrd records a new Service version.

This is what makes a verdict reproducible. A verdict always names the Service version it checked, and that version names exactly which model, prompt, and agent ran.

## Declare Cards from code

`Data` and `Model` Cards carry bytes, such as a training set or model weights, so you usually declare them from the code that produces them. `support-desk` has no trained model, but say you add one that scores how likely a customer is to churn. The Python SDK saves the artifact, uploads it, and declares the Card in one call.

```python title="train.py"
import pandas as pd
from sklearn.linear_model import LogisticRegression

from wyrd import Cards
from wyrd.data import DataCard, PandasInterface
from wyrd.model import ModelCard, ModelCardMetadata, ModelSignature, SklearnInterface

cards = Cards()  # server URL and credential come from your Wyrd config

train = pd.read_parquet("churn-training.parquet")
data = DataCard(PandasInterface(data=train), space="default", name="churn-training", version="1.4.2")
cards.data.declare(data)

features = train[["tenure_months", "monthly_spend"]]
model = LogisticRegression().fit(features, train["churned"])
signature = ModelSignature.from_data(inputs=features, outputs=model.predict_proba(features)[:, 1])
card = ModelCard(
    SklearnInterface(model=model),
    space="default",
    name="churn-classifier",
    version="1.4.2",
    metadata=ModelCardMetadata(task_type="binary_classification", signature=signature),
)
cards.model.declare(card)
```

A `ModelCard` needs a signature, the names and types of the model's inputs and outputs; without one, construction fails with `WYRD_MODEL_400_MISSING_SIGNATURE`. Code-built Cards default to version `0.1.0`, so set `version` yourself. `declare` updates the card object in place with its server-assigned `uid`. The Service can now list it as a component with `ref: { kind: Model, name: churn-classifier, version: 1.4.2 }`.

Python can declare a Prompt from code the same way, with `cards.prompt.declare`. The Rust and TypeScript SDKs do not build Cards in code; they declare Card files, including `Data` and `Model` files that list `artifacts:`, with `declare_from_path` and `declareFromPath`.

## Relationships come from references

You never write `relationships` yourself. When a Card is declared, the server reads every reference in its `spec`, components and bindings alike, and records the edge in both directions. For `support-desk` that graph is:

```text
Service support-desk
├── agent:  Agent support-agent
│           └── Prompt support-prompt
├── prompt: Prompt support-prompt
├── Verifier no-refund-promise
├── Verifier ticket-complete
└── Verifier follows-policy
            └── Agent policy-judge
```

Components carry their alias in `outbound_refs`; binding edges have none.

The graph gives you lineage. From any Card you can walk to everything it depends on and everything that depends on it, so before changing `support-prompt` you can see every Agent and Service that uses it.

It also scopes observations. A running service may only record observations about Cards in its own graph: `support-desk` can record them for `support-agent` or `support-prompt`, but not for a Card it does not reference. See [Runs, observations, and retained evidence](/understand/runs-observations-and-evidence/).

## What can go wrong

| Error code | Cause | Fix |
| --- | --- | --- |
| `WYRD_CLI_400_CARD_LOAD` from `wyrd declare --check` or `wyrd declare`, `WYRD_LOADER_400_INVALID_ENVELOPE` from an SDK | A file failed to load; each problem is listed with its file | Fix the files listed |
| `WYRD_REGISTRY_400_UNRESOLVED_PATH_REF`, after `WYRD_LOADER_400_IO` | A `ref:` path points at no file; the message names the file that holds the `ref` | Fix the path, relative to that file |
| `WYRD_SPEC_400_INVALID_SERVICE_COMPONENT_KIND` | A Verifier, Trigger, Operator, Audit, or Source is listed as a component | Bind it with `verified_by` instead |
| `WYRD_SPEC_400_UNBOUND_VERIFIER_PEER` | A Verifier file in the folder is not bound to anything | Bind it, or move it out of the folder |
| `WYRD_REGISTRY_409_SPEC_DRIFT` | An exact `version` is already declared with different content | Bump the `version` |
| `WYRD_REGISTRY_422_UNRESOLVED_DEPENDENCY` | A declared-Card reference names a Card that does not exist | Declare it first, or fix the reference |
| `WYRD_PERMISSION_403_DENIED_RBAC` | Your credential lacks `cards:write` | Use a credential that can declare Cards |

Next, [How verification works](/understand/how-verification-works/) shows how Verifiers attach to these Cards.
