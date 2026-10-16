---
title: Register and inspect a Service
description: Register a Service and every Card it references in one call, then read back its exact versions and the relationships the server derived.
pillar: wyrd
section: Build and verify
topic: Declare
order: 3
status: runnable
---

Registering the Service registers every Card it references, dependencies first, and creates its tables. The server then records which Card references which, so you can read the graph back from any Card.

## Register

From the CLI:

```bash
wyrd apply examples/support-desk-app/service/support-desk.yaml
```

It prints `registered:` and the Service's reference, then one line per Card with its outcome and status:

```text
registered: default/Service/support-desk@1.0.0#<uid>
  default/Prompt/support-prompt@1.0.0#<uid>: registered (active)
  ...
```

<Aside type="caution" title="Unverified">The line format is derived from the CLI source. A captured run replaces it on the first verified run.</Aside>

From code, `register_from_path` does the same and returns the receipt. The example's deploy step then downloads what it registered:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

<CodeFromFile file="examples/support-desk-app/python/deploy.py" lang="python" region="deploy" title="deploy.py" />

</LangTab>
<LangTab lang="rust">

<CodeFromFile file="examples/support-desk-app/rust/lib.rs" lang="rust" region="deploy" title="lib.rs" />

</LangTab>
<LangTab lang="typescript">

<CodeFromFile file="examples/support-desk-app/typescript/deploy.ts" lang="typescript" region="deploy" title="deploy.ts" />

</LangTab>
</LangTabs>

`receipt.root` is the Service's exact reference, and `receipt.outcomes` lists every Card with its outcome:

| Outcome | Means |
| --- | --- |
| `registered` | A new version was stored |
| `idempotent_noop` | That exact version is already registered with the same content |
| `deduplicated` | The Card has no exact version, and the latest one in its line has the same content |

Registering is safe to rerun: unchanged Cards come back `idempotent_noop`. If the loader finds any error, nothing is sent.

## Inspect

List what is registered:

```bash
wyrd list --kind Service
```

Fetch one Card with the relationships the server derived. You never write `relationships`; the server reads every reference in each `spec` and records the edge both ways:

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

`outbound_refs` lists every Card this one references, with the component `alias` where there is one. `inbound_refs` lists every Card that references it, so fetching `support-prompt` shows both the Agent and the Service that use it. `status.phase` is the Card's lifecycle state, such as `active`.

To download the Service and everything it references into a directory, as the app does at deploy time:

```bash
wyrd get --kind Service --space default --name support-desk --version 1.0.0 --output-dir .wyrd-bundle
```

## Change a Card

An exact version is immutable. To change `support-prompt`, give it a new `version`, update the reference to it in `support-agent.yaml`, and bump the Agent and the Service the same way. Register again, and the new Service version records the new Prompt while the old one still names the old Prompt. That is what keeps an old verdict reproducible.

Leave `version` out of a Card instead, and the server picks one: `0.1.0` first, then the next patch whenever the content changes. That suits Cards you iterate on; we recommend exact versions for anything a Service pins.

## If it fails

| You see | Cause | Fix |
| --- | --- | --- |
| `WYRD_REGISTRY_409_SPEC_DRIFT` | An exact version is already registered with different content | Bump the Card's `version` |
| `WYRD_REGISTRY_422_UNRESOLVED_DEPENDENCY` | A `ref` names a registered Card that doesn't exist | Register that Card first, or fix the reference |
| `WYRD_VALA_409_BIFROST_FINGERPRINT_MISMATCH` | A table in `spec.tables` already exists with different columns | Keep the existing schema, or use a new table name |
| `WYRD_PERMISSION_403_DENIED_RBAC` | Your credential lacks `cards:write` | Use a credential that can register Cards |
| `WYRD_LOADER_400_INVALID_ENVELOPE` | A file failed local validation; nothing was sent | Run `wyrd plan` and fix the file it names |

## Next

[Record a run and observations](/build-and-verify/observe/record-a-run-and-observations/) against the registered Service.
