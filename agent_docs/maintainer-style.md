# Maintainer style

Load when implementing or reviewing code for readability. Examples show shape,
not required names. Rust ownership rules live in `agent_docs/rust.md`.

## Layout: put a workflow with its owner

Bad: catch-all modules, so callers must guess where a workflow landed.

```text
src/
  utils.rs       # declare_card, stage_audit, flush_observations
  helpers.rs     # more steps of the same workflows
```

Good: modules follow the capability; the public handle exposes its operations.

```text
src/
  cards/
    handle.rs    # Cards::declare, Cards::get
  outbox/
    writer.rs    # Outbox::stage
```

Keep a small private helper beside its caller. Add a module only for a
coherent responsibility.

## Methods: retained state has one owner

Bad: operations on a writer's state live outside the writer.

```rust
fn enqueue(writer: &mut BatchWriter, observation: Observation) { /* ... */ }
fn take_ready_batch(writer: &mut BatchWriter) -> Option<Vec<Observation>> { /* ... */ }
```

Good: the owner holds state that survives across operations, and callers find
the capability as a method.

```rust
impl BatchWriter {
    /// Adds an observation to this writer's pending batch.
    fn enqueue(&mut self, observation: Observation) {
        self.pending.push(observation);
    }

    /// Takes a batch once the configured minimum is reached.
    fn take_ready_batch(&mut self) -> Option<Vec<Observation>> {
        (self.pending.len() >= self.batch_size.get())
            .then(|| std::mem::take(&mut self.pending))
    }
}
```

## Functions: show the main path

Bad: generic names, and pure preparation made async.

```rust
async fn process(&self, input: Input) -> Result<Receipt, Error> {
    let batch = self.do_work(input).await?;
    self.send(batch).await
}
```

Good: name the domain action and keep stages in order.

```rust
async fn flush(&self, input: Input) -> Result<Receipt, Error> {
    let batch = self.prepare(input)?; // pure work stays synchronous
    self.send(batch).await
}
```

A helper earns its place by isolating a distinct rule or stage, not by
forwarding arguments. Leave a short, locally readable branch in place. Each
function or class does one thing; one that does two is split. All code must be
directly testable.

## Tests: prove an outcome a caller cares about

Bad: passes when a response exists, even if the wrong Card persisted.

```python
def test_register(client, card):
    assert client.cards.declare(card) is not None
```

Good: assert the durable result through the public client.

```python
def test_registered_card_can_be_loaded(client, card):
    receipt = client.cards.declare(card)
    loaded = client.cards.get(receipt.card_ref)
    assert loaded.metadata.uid == receipt.card_ref.uid
    assert loaded.metadata.version == receipt.card_ref.version
```

Parameterize repeated pure cases. Share setup only when it removes real
duplication; do not build a harness to prove one branch.

## Documentation: say what the reader cannot infer

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

A Python docstring likewise states the contract: "Read a saved Card from disk
without contacting the Wyrd server", not "Load a card".

## Python and TypeScript: types carry the contract

Docs explain a type's meaning; they cannot replace a missing or broad type.

```python
# Bad
def describe(self, x: Any, y: Any) -> dict[str, Any]:
    """Get a table."""

# Good
def describe_table(self, namespace: str, name: str) -> TableDescription:
    """Return the stored physical schema of one registered table.

    Args:
        namespace (str): Namespace containing the registered table.
        name (str): Name of the table within that namespace.

    Returns:
        TableDescription: The table's stored schema and physical layout.
    """
```

```ts
// Bad
/** List cards. Returns card summaries. */
async list(data: any): Promise<any>

// Good
/**
 * List metadata-only Card summaries visible to this client.
 *
 * @param request - Registry filters and pagination; omitted means the default page.
 * @returns Card summaries and a cursor when another page exists.
 */
async list(request: ListCardsRequest = {}): Promise<ListCardsResponse>
```

- Annotate every public parameter and return. Keep `Args`, `Returns`,
  `Raises`, `@param`, and `@returns` identical to the signature.
- `T | None` only when `None` is accepted or returned, and say what it means.
- `Any`/`unknown` only at a genuinely open boundary; narrow it there.

## Review threshold

Read the changed symbol, its module, callers, and tests before judging. A
blocking finding names the location, the rule or example it breaks, the
concrete cost to understanding or changing the code, and the smallest fix
within the task. Prefer a nearby Wyrd pattern. Personal preference, a
line-count threshold, an equally clear alternative, or an unrelated old
inconsistency is not a finding; record uncertain preferences separately.
