# Repository-root declaration proposal

Status: superseded proposal; comparison evidence only.

> **Reconsidered after binding-owner research.** The non-Card repository proposal
> below is retained only as comparison evidence. The current recommendation is a
> native Repository Card: existing verification bindings require a Card owner and
> Card subject, and it supplies a real declaration graph root. The native
> kind is now agreed; its final schema remains proposed. See the main design notes.

## User requirement

`wyrd declare wyrd.toml` is the single user operation. It resolves local and
registered Verifiers and creates/updates repository bindings automatically.
Separate user-facing declaration and binding commands were rejected.

## Recommended authoring shape

Extend the existing strict `wyrd.toml` schema with one optional repository section:

```toml
[defaults.labels]
team = "platform"

[repository]
host = "https://api.github.com"
repository_id = "12345678"

[[repository.verified_by]]
verifier = "./review/code-review.yaml"

[[repository.verified_by]]
verifier = { kind = "Verifier", name = "security-review", version = "1.2.0" }
```

This is proposed syntax, not a currently accepted config. The repository must
already be connected to this tenant through the deployment's App. Stable host
and repository ID identify it; owner/name is a mutable display label. Tenant
and credentials come from authentication and connection state, not TOML.

The `verifier` slot uses the existing authoring forms: local path or exact CardRef.
Do not introduce a second Verifier body format, version range, `latest`, or
additional path/ref discriminator. Local YAML owns normal Card metadata/spec;
existing declaration rules resolve omitted/scoped authored versions. TOML never
assigns Card versions. A local file's transitive references keep their existing
path/inline/registered semantics. Referenced top-level roots must be Verifiers.

This section is a repository PR-review attachment, consumed by the GitHub
adapter's explicit submission path. It does not alter the meaning of omitted
`runs_on` on existing Service/Agent bindings or add a generic Trigger event
vocabulary. Detailed eligible PR actions remain a separate event-lifecycle
contract. The sample proposes repository-wide selection without branch filters.

## How it fits the current root contract

A declaration-operation root and a Card-graph root are different responsibilities.
The TOML file identifies the repository configuration being declared. It need not
be a registered Card or a node in a composite Card request.

The offline loader builds a repository declaration plan with its identity and
ordered Verifier entries. Each local Verifier entry expands through the existing
Card loader and graph validation. Existing registered references become exact
lookups, not empty Card submissions. Normalize repeated local paths and reject
repeated effective Verifier identities. Validate every local graph before any
remote declaration begins. Files are resolved relative to the TOML; transitive
paths stay relative to their owning Card files and use the existing containment
owner. Load explicit roots rather than recursively declaring every YAML in the
repository. Existing YAML/directory invocation semantics stay as they are.

The client runs the existing registration saga for each distinct local root,
retaining each exact returned root identity. Shared dependencies reuse existing
server content/version deduplication; do not add a second version resolver.
Standalone Verifier roots are supported by existing composition validation.
Multiple independent Verifiers are not crammed into one ambiguous Card graph.

Consequently, retain `POST /v1/cards`, its flat submissions and server-derived
Card root. No fake Service, new Repository Card kind, generic graph-node union,
or relaxation of existing Card composition is needed. Registered-only TOML
requires no Card write at all.

This is one client operation driving multiple necessary network phases, just as
current declaration already sequences registration, uploads and completion. It
is not a second CLI workflow. Shared client code owns this sequencing; the CLI
only selects the entry kind and renders the result.

## Server-owned binding registration

Before publishing local Cards, preflight the connection and caller's repository
write/access authority and read the current configuration revision. Preflight
limits avoidable partial publication but grants no durable permission. Recheck
all relevant authority at activation.

After all selected Cards are registered and ready, the same declare operation
submits the complete resolved repository selection to a typed repository
configuration operation. No filesystem paths or TOML cross this boundary.
The server verifies the tenant connection, repository access, caller authority,
exact active Verifier identities and PR compatibility. The server then stores
an immutable configuration revision, derives its bindings, and atomically makes
that revision active. It accepts no subset of an invalid selection.

The active revision represents the complete managed selection in this TOML:
removed entries are removed from future PR selection; reordered entries alone
should not create a new revision. An empty selection is refused in this first
proposal rather than silently disabling review. Explicit disable/offboarding
uses its own connection lifecycle; changing a defaults-only TOML is not disable.

Use the revision observed at operation start to reject a stale competing update.
Identical effective selection returns the current revision without replacing it.
The resolved semantic selection, not source whitespace or file path, determines
identity. A binding revision is server-derived configuration history; a Verifier
version is normal Card identity. They are not the same version counter.

The existing SQL projector cannot be called unchanged: `NewBinding` has a Card
subject, `project_bindings` requires owner Card UID, and the stored owner has a
Card foreign key. Extend repository binding/subject ownership under the existing
verification domain; never fabricate a Card UID. The exact migration and typed
repository operation must be specified with the broader repository-target work.

Return a repository declaration receipt identifying repository, active revision,
bindings and exact Verifiers, with outcomes for any local Card declarations.
Existing Card declarations retain their current Card receipt shape. CLI
in-process and SDK interfaces need an explicit typed result for the TOML entry;
do not pretend `DeclarationReceipt.root: CardRef` can hold a repository.
Exact wire field spellings beyond the authoring sketch remain to specify.

## Failure and runtime behavior

- Local parse/reference/graph failure: no remote write.
- Missing repository access: preflight refusal, then authoritative recheck at
  activation so revocation during upload is still effective.
- Card registration/upload/readiness failure: binding revision is not activated.
  Previously active configuration remains effective.
- Some Cards succeed before another fails: retain the registered Cards, report
  the partial declaration outcome and allow retry. Do not delete valid Cards or
  claim atomicity across registration, object storage and binding activation.
- Process stops after Card completion but before activation: prior binding stays
  active. Repeating `wyrd declare wyrd.toml` reuses normal registered content and
  attempts activation. No background callback may later activate stale intent.
- Activation response lost: operation replay/effective-selection identity must
  recover the accepted revision without producing another one. Exact request
  idempotency must be specified; current Card saga's UUID-per-attempt key does
  not itself provide cross-invocation repository declaration replay.
- Binding update races: stale different selection is refused, not silently
  written over a newer revision. An intentional retry starts from current state.
- A PR review snapshots the active revision and exact PR commits at logical
  acceptance. Queue retry/recovery uses that selection, not the newest binding.
- A pushed file alone does not activate policy; an authorized `wyrd declare`
  does. CI can call that same operation. No implicit server fetch/import from
  untrusted PR-head config is introduced.

## Source-backed rationale and affected owners

Research was read-only; no tests were run.

- `wyrd-loader/src/lib.rs::load`, `parse.rs::parse_path_with_sandbox`,
  `resolve.rs::resolve_tree`: existing YAML/config/path resolution pipeline.
- `wyrd-spec/src/graph/root.rs::pick_root`: one root, with a sole-Service-root
  preference among independent candidates; `composition.rs::validate_composition`
  validates bound peers. Earlier shorthand that it always demands one graph
  candidate was incomplete. Keep these existing Card rules.
- `wyrd-spec/src/reference.rs::Ref`: local Path and registered CardRef authoring.
- `wyrd-client/src/cards/handle.rs::CardSelector::exact` and reads: reuse exact
  registered lookup; avoid latest-selector semantics.
- `wyrd-client/src/cards/saga/mod.rs::declare`: prepare, submit, upload, complete,
  ensure Active. Reuse this complete lifecycle for each local root.
- `wyrd-server/src/components/cards/service.rs::persist_node/BindingProjector`:
  existing versioned registration and automatic Service/Agent binding projection.
- `wyrd-sql/src/queries/verification.rs::NewBinding/project_bindings`: current
  bindings require Card owner and subject; repository ownership is new work.
- `wyrd-spec/src/registry/submission.rs::DeclarationReceipt`: root is CardRef;
  existing Card result must not be silently reinterpreted.

Work reaches the config schema, shared offline declaration loader/plan, shared
client sequencing, CLI dispatch/check/output, typed repository contracts and
server/SQL binding owners, SDK/MCP projections and their user journeys. The
independent bohmian-code-review library needs no changes for this declaration
feature. Its review/evidence changes are a separate boundary.

Focused future proof: one mixed local/reference TOML declaration produces exact
ready Verifiers and an active binding; repeated unchanged declaration is stable;
changed local content selects a new Card/config version; invalid dependency,
partial upload, revoked access, lost activation response and competing update
leave predictable state. `--check` remains offline and cannot claim remote
reference existence or repository authority. Existing YAML/directory declaration
journeys must retain their behavior.

## Authority and remaining decisions

The user has selected one `wyrd declare` operation with automatic resolution from
a TOML root. This proposal has not been approved in full. The owning workspace
config authority currently describes only defaults, so accepting `[repository]`
requires an authority/schema update before spec approval. Existing Card-root
rules and the rule that the server never reads `wyrd.toml` remain intact under
this design. Repository activation is a new server-owned contract.

Material decisions still needing agreement: this proposed schema and complete-
selection semantics; repository-wide versus branch-scoped configuration; exact
binding authority/PR compatibility; typed result/API shapes; selected PR actions.
Do not ask the implementor to select these as private implementation details.
