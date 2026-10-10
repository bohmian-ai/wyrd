# Maintainer Style

Use this guide when implementing or reviewing Wyrd code. `AGENTS.md`,
`architecture/agent-rules.md`, and the active Wyrd design take precedence.
The examples show code shape, not new APIs or required names.

## Layout: put a workflow with its owner

Bad: a catch-all module holds unrelated workflows, so callers must know where
an agent happened to place them.

```text
src/
  utils.rs       # register_card, stage_audit, flush_observations
  helpers.rs     # more steps of the same workflows
```

Good: modules follow the capability; the public handle exposes its operations.

```text
src/
  cards/
    handle.rs    # Cards::register, Cards::get
  outbox/
    writer.rs    # Outbox::stage
```

Keep a small private helper beside its caller. Create another module only when
it has a coherent responsibility. [`Cards`](../../../crates/shared/wyrd-client/src/cards/handle.rs)
is the canonical Wyrd handle shape (`AGENTS.md` §5).

## Structs and methods: own retained state once

Bad: operations that use a writer's state live outside the writer.

```rust
struct BatchWriter {
    pending: Vec<Observation>,
    batch_size: usize,
}

fn enqueue(writer: &mut BatchWriter, observation: Observation) {
    writer.pending.push(observation);
}

fn take_ready_batch(writer: &mut BatchWriter) -> Option<Vec<Observation>> {
    (writer.pending.len() >= writer.batch_size)
        .then(|| std::mem::take(&mut writer.pending))
}
```

Good: the concrete owner holds state and dependencies that survive across
operations, and callers discover that durable capability through methods.

```rust
/// Buffers observations until a batch can be sent.
struct BatchWriter {
    /// Observations waiting to be sent.
    pending: Vec<Observation>,
    /// Nonzero minimum number of observations in a ready batch.
    batch_size: NonZeroUsize,
}

impl BatchWriter {
    /// Adds an observation to this writer's pending batch.
    fn enqueue(&mut self, observation: Observation) {
        self.pending.push(observation);
    }

    /// Takes a batch when the configured minimum has been reached.
    fn take_ready_batch(&mut self) -> Option<Vec<Observation>> {
        (self.pending.len() >= self.batch_size.get())
            .then(|| std::mem::take(&mut self.pending))
    }
}
```

The owning type must have meaningful state or invariants. Do not add a
zero-sized utility struct, a single-implementation trait, or a god object to
make code look object-oriented.

A free function may coordinate multiple steps, dependencies, or awaits when
everything it uses remains caller-owned for that invocation. Move it onto a
struct only when the struct retains meaningful state, resources, identity, or
invariants across calls, or when the type prevents an invalid lifecycle.

## Function shape: show the main path

Bad: generic names hide the operation, and pure preparation is needlessly
async. Each caller now has to await work that performs no IO.

```rust
async fn process(&self, input: Input) -> Result<Receipt, Error> {
    let batch = self.do_work(input).await?;
    self.send(batch).await
}

async fn do_work(&self, input: Input) -> Result<Batch, Error> {
    if input.observations.is_empty() {
        return Err(Error::EmptyBatch);
    }
    Ok(Batch::new(input.observations))
}
```

Good: name the domain action and keep meaningful stages in order. A helper
earns its place when it isolates a distinct rule or stage, not when it merely
forwards arguments.

```rust
/// Flushes a nonempty batch of observations.
///
/// # Errors
/// Returns `Error` for an empty batch or a failed send.
async fn flush(&self, input: Input) -> Result<Receipt, Error> {
    let batch = self.prepare(input)?; // Pure work stays synchronous.
    self.send(batch).await
}

/// Validates input and builds the batch before any IO begins.
///
/// # Errors
/// Returns `Error::EmptyBatch` when there are no observations to send.
fn prepare(&self, input: Input) -> Result<Batch, Error> {
    if input.observations.is_empty() {
        return Err(Error::EmptyBatch);
    }
    Ok(Batch::new(input.observations))
}
```

Do not mechanically extract every branch. A short, locally readable branch is
clearer in place. Names such as `prepare` and `send` should
describe actual responsibilities, with failure and partial-progress behavior
visible to a caller.

## Tests: prove an outcome a caller cares about

Bad: a test passes when a response exists even if registration did not persist
the right Card.

```python
def test_register(client, card):
    assert client.cards.register(card) is not None
```

Good: assert the durable result through the public client and name the
scenario. The exact API and fixture names below are illustrative.

```python
def test_registered_card_can_be_loaded(client, card):
    receipt = client.cards.register(card)
    loaded = client.cards.get(receipt.card_ref)
    assert loaded.metadata.uid == receipt.card_ref.uid
    assert loaded.metadata.version == receipt.card_ref.version
```

For a public capability, also cover its relevant rejection and edge flows at
the user-journey tier required by `AGENTS.md` §11. For pure input/output cases,
use a focused unit test and parameterize repeated cases. Share setup only
when it removes real duplication. Do not create a test harness to prove one
branch.

## Documentation: explain what a maintainer cannot infer

Bad:

```rust
/// Flushes the batch.
///
/// # Errors
/// Returns an error if an error occurs.
```

Good:

```rust
/// Publishes the buffered observations under their stable batch identity.
///
/// A retry can replay this identity after an uncertain response without
/// creating a second durable batch.
///
/// # Errors
/// Returns `WriteError` when validation fails or the server rejects the batch.
///
/// # Cancellation
/// The server may have accepted the batch before cancellation; retry is safe.
```

New and materially modified Rust items need substantive rustdoc, including
private items (`AGENTS.md` §16). A Python docstring should likewise clarify a
contract or surprising behavior:

```python
# Bad: repeats the name.
def load_local_card(path):
    """Load a card."""

# Good: tells a maintainer whether this operation crosses an IO boundary.
def load_local_card(path):
    """Read a saved Card from disk without contacting the Wyrd server."""
```

Do not leave task IDs, agent notes, or implementation history in permanent
code.

## Python and TypeScript: document the typed contract

Public names, parameter types, return types, and documentation must describe
the same operation. Documentation explains a type's meaning; it cannot replace
a missing or broad type. Preserve Wyrd wire field names for durable values.

Python bad: `x`, `y`, and `Any` leave the caller and type checker unable to tell
which table is requested or what comes back. The docstring adds no information.

```python
def describe(self, x: Any, y: Any) -> dict[str, Any]:
    """Get a table."""
    return self._client.describe_table(x, y)
```

Python good: use domain names and the actual result type. Document the
meaning of arguments and a non-obvious property of the result.

```python
def describe_table(self, namespace: str, name: str) -> TableDescription:
    """Return the stored physical schema of one registered table.

    Args:
        namespace (str): Namespace containing the registered table.
        name (str): Name of the table within that namespace.

    Returns:
        TableDescription: The table's stored schema and physical layout.
    """
    return self._client.describe_table(namespace, name)
```

Annotate every public parameter and return value. Include argument and return
types in `Args` and `Returns`; keep them identical to the annotations, which
the type checker uses. Use `T | None` only when `None` is accepted or returned;
explain what it means. Keep docstring `Args`, `Returns`, and `Raises` names and
conditions aligned with the signature and behavior. Reserve `Any` for a
genuinely open boundary, narrow it there, and return a domain type. Generated
`.pyi` files must reflect the source; change the source or generator, never
the generated stub.

TypeScript bad: broad types and generic argument names erase the contract, and
a comment claiming a return shape cannot restore it.

```ts
/** List cards. Returns card summaries. */
async list(data: any): Promise<any> {
  return this.#native.list(JSON.stringify(data));
}
```

TypeScript good: one named request type states the accepted fields, and the
return type states the result. JSDoc clarifies behavior the signature cannot.

```ts
/**
 * List metadata-only Card summaries visible to this client.
 *
 * @param request - Registry filters and pagination; omitted means the default page.
 * @returns Card summaries and a cursor when another page exists.
 */
async list(request: ListCardsRequest = {}): Promise<ListCardsResponse> {
  return lifecycleValue<ListCardsResponse>(
    await this.#native.list(JSON.stringify(request)),
  );
}
```

Give exported operations explicit return types. Use descriptive camelCase
parameter names and typed option objects when several related optional inputs
would otherwise become an unclear positional list. Use `unknown` for untrusted
input and narrow it before use; avoid `any`, `Object`, `Function`, and casts
that pretend to validate a response. Keep JSDoc `@param` names, optionality,
and `@returns` claims aligned with the declaration. N-API declarations are
generated: fix their source instead of editing `index.d.ts` by hand.

For either language, a maintainer finding is warranted when the public
signature or generated declaration allows invalid calls, hides the result
shape, or contradicts the documentation. Run the owning typecheck and codegen
checks after changing a public contract.

## Review threshold

Read the changed symbol, owning module, callers, and relevant tests before
judging it. A blocking finding must identify the changed location, the rule or
example above, a concrete cost to understanding or safely changing the code,
and the smallest correction that preserves the approved task. Prefer a nearby
Wyrd pattern when one exists. A personal preference, line-count threshold,
equally clear alternative, or unrelated old inconsistency is not a finding.
Record uncertain preferences separately for maintainer calibration.
