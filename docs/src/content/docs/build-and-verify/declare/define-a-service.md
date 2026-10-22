---
title: Services
description: Declare a Service from YAML files, with each component at an exact version and a table for the rows it records, then read back what the server stored.
pillar: wyrd
section: Build and verify
topic: Declare
order: 1
status: runnable
---

Declare the Service your app runs: the components it is built from, each at an exact version, and the tables it records rows into. Every verdict then names exactly what it checked.

You need a running server from [Set up Wyrd](/get-started/setup/). This page builds the `support-desk` Service; the finished files are in `examples/support-desk-app/service/`.

## 1. Write the components

Each component is its own YAML file with an exact `metadata.version`. The support desk has an Agent and the Prompt it runs, and the Agent points at the Prompt by relative path:

<Tabs>
<Tab label="support-agent.yaml">

<CodeFromFile file="examples/support-desk-app/service/support-agent.yaml" lang="yaml" title="support-agent.yaml" />

</Tab>
<Tab label="support-prompt.yaml">

<CodeFromFile file="examples/support-desk-app/service/support-prompt.yaml" lang="yaml" title="support-prompt.yaml" />

</Tab>
</Tabs>

`execution_route` says how the Agent reaches its model: `wyrd_gateway` sends it through the [LLM gateway](/connect/set-up-the-llm-gateway/) deployment named `openai`. Use `{kind: native}` when your own code calls the model.

A hosted model such as `gpt-4o` needs no file of its own. Declare a `Model` only for a model you own and version, such as a trained classifier, so verdicts attach to its exact version.

## 2. Write the Service

The Service gives each component an `alias`, the name your code uses at runtime, such as `run.for_card("agent")`:

```yaml title="support-desk.yaml"
apiVersion: wyrd/v1
kind: Service
metadata:
  name: support-desk
  version: 1.0.0
  space: default
spec:
  service_type: agent
  tables:
    - name: tickets  # becomes vala.datasets.tickets
      schema:
        type: object
        properties:  # in column order
          question: {type: string}
          answer: {type: string}
        required: [question, answer]
  components:
    - alias: agent
      ref: ./support-agent.yaml  # relative to this file
    - alias: prompt
      ref: ./support-prompt.yaml
```

A `ref` can also name a component that is already declared, such as `{kind: Model, name: churn-classifier, version: 1.4.2}`. Every reference names one exact version, never a range.

## 3. Check it

```bash
wyrd declare --check support-desk.yaml
```

It loads the Service and every file it references, validates them without contacting the server, and lists each one. Fix anything it reports before you declare.

## 4. Declare it

```bash
wyrd declare support-desk.yaml
```

It declares every file the Service references, dependencies first, creates the `tickets` table, and prints one line per file with its outcome. Run it again: unchanged files come back `idempotent_noop`, so declaring is safe to repeat.

<Aside type="note" title="From code">

`WyrdState.declare("support-desk.yaml")` declares the same files and returns the Service ready to use, as in [Write a Verifier](/build-and-verify/verify/author-a-verifier/#3-call-it-and-read-the-verdict).

</Aside>

## 5. Read it back

The server records which component references which, in both directions. Fetch the Service and list its components:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```python
from wyrd.cards import CardRef, Cards

card = Cards().get(CardRef("Service", "support-desk", "1.0.0", space="default"))
for edge in card.relationships["outbound_refs"]:
    print(edge.get("alias"), edge["ref"])
```

</LangTab>
<LangTab lang="rust">

```rust
use wyrd_ai::cards::{CardRef, CardSelector, Cards};

let service: CardRef = "default/Service/support-desk@1.0.0".parse()?;
let card = Cards::connect().await?.get(CardSelector::exact(service)).await?;
for edge in &card.relationships.outbound_refs {
    println!("{edge:?}");
}
```

</LangTab>
<LangTab lang="typescript">

```typescript
import { Cards } from "wyrd-ai";

const card = await (await Cards.connect()).get("default/Service/support-desk@1.0.0");
for (const edge of card.relationships.outbound_refs) console.log(edge.alias, edge.ref);
```

</LangTab>
</LangTabs>

It prints one line per component, with its alias and exact version. Fetching `support-prompt` instead shows, under `inbound_refs`, both the Agent and the Service that use it.

## 6. Change a component

A declared version never changes. To change the Prompt, set a new `version` in `support-prompt.yaml`, then bump the Agent and the Service that reference it, since they now point at different content. Declare again: the new Service version records the new Prompt, and verdicts from the old version still name the old one.

<Aside type="note" title="Set the space once">

`space` groups related files, such as a team's. Instead of writing it in every file, put a `wyrd.toml` next to them or in a parent directory:

```toml
[defaults]
space = "support"
```

A file's own `metadata.space` still wins.

</Aside>

## If it fails

| You see | Fix |
| --- | --- |
| `WYRD_LOADER_400_INVALID_ENVELOPE` | Fix the file `details.diagnostics` names: a missing `apiVersion` or `kind`, a path to no file, or a missing exact `version` |
| `WYRD_REGISTRY_409_SPEC_DRIFT` | That version is already declared with different content: bump its `version` |
| `WYRD_REGISTRY_422_UNRESOLVED_DEPENDENCY` | Declare the referenced component first, or fix the `ref` |
| `WYRD_VALA_409_BIFROST_FINGERPRINT_MISMATCH` | The table exists with different columns: keep its schema, or use a new table name |

## Next

[Bind a Verifier](/build-and-verify/declare/bind-a-verifier/) to say how the Service is checked.
