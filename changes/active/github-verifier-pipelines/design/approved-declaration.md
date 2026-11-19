# Approved Repository declaration

Status: approved design decisions; exact wire/schema and implementation remain to specify.

The user approved the six declaration rules presented in conversation.

## 1. Infer and verify identity

`wyrd declare wyrd.toml` uses the containing checkout's origin remote as its
repository identity hint. Wyrd verifies the target against the tenant's connected
GitHub installation and obtains the stable provider repository ID and name.

Missing or ambiguous identity is a clear refusal. Explicit metadata/spec
overrides take precedence and are still verified; explicit repository identity
supports declaration outside a Git checkout. Provider lookup and secret-bearing
requests remain server-owned through the generic provider library.

Infer the initial Repository Card name from the repository name. After first
registration, a provider rename preserves the existing Repository Card family.
The stable external repository identity and Card family must not silently drift.
Exact inference/override fields and diagnostic shapes remain specification work.

## 2. Declare the selected graph

The minimal authoring contract remains:

```toml
[repository.verified_by]
verifier = ["./review/code-review.yaml"]
runs_on = { kind = "pull_request" }
```

Infer the native Repository Card as root and resolve the selected Verifiers and
their transitive dependencies. Do not recursively declare unrelated repository
YAML as a consequence of using the TOML root. Local definitions and exact
registered references follow existing authoring/resolution conventions. Complete
local validation before remote Card registration begins.

TOML binding shorthand expands into ordinary per-Verifier bindings. Existing
YAML/directory declaration behavior is not replaced by another registration
system; a normal YAML Repository declaration uses the same server lifecycle.

## 3. Normal Card versioning

Reuse existing registration/version resolution:

- Unchanged resolved content reuses the existing eligible Card version.
- Changed content follows existing automatic versioning unless explicitly pinned.
- Changed content at an existing exact version is rejected.
- A changed Verifier reference changes the resolved Repository spec and produces
  a new Repository version through those same rules.

Do not add separate repository versioning or implicit remote latest resolution.
An independently registered Verifier version never updates a Repository binding
without another authorized Repository declaration.

## 4. Readiness, selection, failure and concurrency

A successful Repository declaration means its required Cards/artifacts are ready
and that Repository version is selected for future reviews. Historical versions
remain readable; ordinary Active Card lifecycle is not current policy selection.

- Failure preserves the previous selected Repository version.
- Successfully registered dependencies remain valid registered resources; do not
  delete them as compensating rollback.
- Already admitted reviews retain their exact accepted policy/source versions.
- Two different declarations racing from the same selection cannot silently
  overwrite each other: one activates and the other receives a conflict.
  Identical effective declarations can reuse the same selected result.
- Retry/recovery of an existing declaration attempt cannot later undo a newer
  selected policy. Preserve declaration identity through retries and readiness
  completion; delayed old blob completion is not a new activation request.
- An intentional new declaration of an older exact Repository version supports
  rollback, subject to normal content and authority validation.

There is no atomic transaction across all registry/object-storage phases. Atomic
selected-policy change and attempt replay are required observable behavior; exact
request fields, persisted coordination and owner implementation remain to specify.

## 5. Complete desired binding list

The declaration's binding list is the complete desired review configuration.
Adding/removing entries changes future PR selection. An explicitly empty array
intentionally removes automatic review bindings:

```toml
[repository.verified_by]
verifier = []
```

This preserves repository connection/access, historical Cards and review results.
It is a policy update, not provider-access revocation. Already admitted work keeps
its accepted policy unless independently cancelled or access is revoked.

## 6. Offline checks and normal receipts

`--check` remains offline: validate local structure/paths/graph and report checks
that require server identity/reference/authority verification as deferred.
Never claim that an offline check proved remote access or reference readiness.

Normal execution returns the exact Repository reference and resolved declaration
outcomes through the existing Card receipt model. Derived binding IDs are exposed
through the Repository Card's server-managed verification status. Normal YAML
Repository declarations use the same registration/activation lifecycle.

## End-to-end declaration

Load → infer/verify repository → resolve graph → register/complete → select
Repository version → return receipt.

No second user-facing bind command, parallel registry, unrelated YAML scan or
separate Repository receipt/root identity is introduced. Exact new schemas and
readiness/selection handoff still need specification before implementation.
