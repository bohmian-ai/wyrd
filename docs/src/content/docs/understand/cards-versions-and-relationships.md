---
title: Cards, versions, and relationships
description: A Service declaration is a set of versioned Cards, each one a defined component of your system.
pillar: wyrd
section: Understand
order: 1
draft: true
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

Not every Card starts in YAML. In a traditional ML workflow, you register `Data` and `Model` Cards from code as they are produced, with the Python SDK.

The examples on this page use `support-desk`, the service in [`examples/support-desk/`](https://github.com/bohmian-ai/wyrd/tree/main/examples/support-desk): an agent that answers customer questions, the prompt it runs, and two Verifiers that check its answers. The quickstart and the SDK journey tests run the same Cards.

## Every Card has the same shape

Each Card is a YAML document with six top-level fields. You write the first four; the server writes the rest.

```yaml title="service/support-agent.yaml"
apiVersion: wyrd/v1
kind: Agent
metadata:
  name: support-agent
  version: 1.0.0
  space: default
spec:
  prompt: ./support-prompt.yaml  # a Prompt Card file with its own version
  tool_names: []
  run_config:
    max_iterations: 1
```

| Field | Who writes it | Holds |
| --- | --- | --- |
| `apiVersion` | You | Always `wyrd/v1`; a Card without it is rejected |
| `kind` | You | One of the Card kinds above |
| `metadata` | You | `space`, `name`, and optionally `version`, `labels`, and `annotations` |
| `spec` | You | The kind-specific body |
| `relationships` | The server | Every Card this one references and every Card that references it |
| `status` | The server | Lifecycle state, such as whether uploads finished and the Card is active |

A Card is identified by `space`, `kind`, `name`, and `version`. A `space` groups related Cards, such as a team or a domain. Leave it out and the CLI and SDKs fill it in from your `wyrd.toml` defaults, or `default` if none is set.

## A Service names its components

The Service Card is the root of the declaration. Each component gets an `alias`, the name your code uses to reach it at runtime.

```yaml title="service/support-desk.yaml"
apiVersion: wyrd/v1
kind: Service
metadata:
  name: support-desk
  version: 1.0.0
  space: default
spec:
  service_type: agent
  tables:
    - name: tickets
      schema:
        type: object
        properties:
          ticket_id: {type: string}
          question: {type: string}
          answer: {type: string}
          refund: {type: boolean}
        required: [ticket_id, question, answer, refund]
  components:
    - alias: agent
      ref: ./support-agent.yaml
    - alias: prompt
      ref: ./support-prompt.yaml
```

Both components point at files in the declaration folder, so Wyrd registers those Cards first. `tables` declares `vala.datasets.tickets`, which registration creates in Bifrost. The real file also attaches Verifiers to `agent`; [How verification works](/understand/how-verification-works/#bindings-attach-verifiers-to-what-they-check) covers that part.

Register the whole folder with one command:

```bash
wyrd apply examples/support-desk/service/
```

`wyrd apply` reads every Card in the folder, registers them in dependency order, and makes the Service the root. The folder must have exactly one root. If any Card fails validation, nothing is registered.

To validate the folder locally first, without contacting the server, run `wyrd plan examples/support-desk/service/`.

## Three ways to reference another Card

Wherever one Card points at another, you can write the reference three ways.

| Form | Example | Use it when |
| --- | --- | --- |
| Registered Card | `ref: { kind: Model, name: churn-classifier, version: 1.4.2 }` | The Card already exists in the registry |
| File path | `ref: ./support-agent.yaml` | The Card lives in the same folder; Wyrd registers it first |
| Inline | `prompt: { model: ..., request: ... }` | The body only makes sense inside its parent; it gets no identity of its own, except an Agent's Prompt |

A registered reference always names an exact `version`. A path is relative to the file that contains it, not to the folder you run `wyrd apply` from, and the Card in that file must set its own `metadata.version`. An inline body is written directly under the field, with no wrapper key. Inline bodies are allowed only where the field accepts them: an Agent's `prompt`, an LLM judge's agent, and a verification binding's Trigger and Operators. An Agent's inline `prompt` is the exception to having no identity: registering the Agent also registers it as a Prompt Card named `<agent-name>-prompt`, in the Agent's space with its own version line, so `state.agent(alias).prompt()` returns it like any other Prompt Card.

## Versions are exact

Every reference names an exact version, so a registered Service records exactly which components it was built from. On the Card itself, `version` is optional:

| You write in `metadata` | Wyrd records |
| --- | --- |
| No `version`, first registration | `0.1.0` |
| No `version`, content changed | The next patch after the latest registered version |
| No `version`, content unchanged | Nothing new; the existing version stands |
| `version: 1` or `version: 1.2` | The next version within that line, or `1.0.0` / `1.2.0` if the line is empty |
| `version: 2.1.0` | Exactly `2.1.0`; re-registering it with different content fails with `WYRD_REGISTRY_409_SPEC_DRIFT` |

So `support-desk@1.0.0` is stored with `support-agent@1.0.0` and `support-prompt@1.0.0`. The same holds for a registered reference: if the desk referenced `churn-classifier@1.4.2`, registering `churn-classifier@1.5.0` later would not change the Service. To pick up the new model, change the reference to `1.5.0` and re-apply; the Service content changed, so Wyrd records a new Service version.

This is what makes a verdict reproducible. A verdict always names the Service version it checked, and that version names exactly which model, prompt, and agent ran.

To see what a name currently resolves to:

```bash
wyrd latest --kind Agent --space default --name support-agent
```

## Register Cards from code

`Data` and `Model` Cards carry bytes, such as a training set or model weights, so you usually register them from the code that produces them. `support-desk` has no trained model, but say you add one that scores how likely a customer is to churn. The Python SDK saves the artifact, uploads it, and registers the Card in one call.

```python title="train.py"
import pandas as pd
from sklearn.linear_model import LogisticRegression

from wyrd import Cards
from wyrd.data import DataCard, PandasInterface
from wyrd.model import ModelCard, ModelCardMetadata, ModelSignature, SklearnInterface

cards = Cards()  # server URL and credential come from your Wyrd config

train = pd.read_parquet("churn-training.parquet")
data = DataCard(PandasInterface(data=train), space="default", name="churn-training", version="1.4.2")
cards.data.register(data)

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
cards.model.register(card)
```

A `ModelCard` needs a signature, the names and types of the model's inputs and outputs; without one, construction fails with `WYRD_MODEL_400_MISSING_SIGNATURE`. Code-built Cards default to version `0.1.0`, so set `version` yourself. `register` updates the card object in place with its server-assigned `uid`. The Service can now list it as a component with `ref: { kind: Model, name: churn-classifier, version: 1.4.2 }`.

The Rust and TypeScript SDKs do not build `Data` or `Model` Cards in code. They register Card files, including any `artifacts:` the file lists, with `register_from_path` and `registerFromPath`.

## Relationships come from references

You never write `relationships` yourself. When a Card is registered, the server reads every reference in its `spec` and records the edge in both directions. For the components of `support-desk` that graph is:

```text
Service support-desk
├── Agent support-agent
│   └── Prompt support-prompt
└── Prompt support-prompt
```

Two things follow from that graph:

- **Lineage.** From any Card you can walk to everything it depends on and everything that depends on it. Before changing `support-prompt`, you can see every Agent and Service that uses it.
- **Observation scope.** A running service may only record observations about Cards in its own graph. `support-desk` can record observations for `support-agent` or `support-prompt`, but not for a Card it does not reference. See [Runs, observations, and retained evidence](/understand/runs-observations-and-evidence/).

## What can go wrong

| Error code | Cause |
| --- | --- |
| `WYRD_LOADER_400_INVALID_ENVELOPE` | A Card file is missing a required top-level field, such as `apiVersion` or `kind` |
| `WYRD_REGISTRY_409_SPEC_DRIFT` | An exact `version` is already registered with different content |
| `WYRD_REGISTRY_400_UNRESOLVED_PATH_REF` | A `ref:` path does not point at a Card file |
| `WYRD_REGISTRY_400_DEPENDENCY_CYCLE` | Two Cards reference each other, directly or through others |
| `WYRD_REGISTRY_400_UNRESOLVED_INLINE_REFERENCE` | An inline body references a Card that does not exist |

Next, [How verification works](/understand/how-verification-works/) shows how Verifiers attach to these Cards.
