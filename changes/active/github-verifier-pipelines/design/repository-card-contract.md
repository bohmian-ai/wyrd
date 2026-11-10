# Repository Card and PR review contract

Status: exploring. The user approved points 3–6 of the six-part conversation
contract: automatic declaration/registration/binding activation; one selected
Repository version and update/failure semantics; PR Trigger/lifecycle; and frozen
policy/source identities through the existing queue/engine. This approval does
not approve every later research detail or unresolved contract below.

## Agreed

- Repository is a native Card.
- Repository envelope, initial name and provider identity are inferred; explicit
  metadata/spec values are overrides, not required authoring. Keep `runs_on`.
- `wyrd declare` is the declaration path; extend it to accept `wyrd.toml` as
  the root and automatically resolve dependencies and repository bindings.
- Local Verifier definitions and exact registered references are both supported.
- Existing verification queue/engine are reused. UI ships in another branch.
- Waiting must eventually support timers, external actions and human approval.

## Approval scope

The approved six-part proposal was in the conversation, not the numbering of
this supplemental document. Its points 3–6 cover:

- One `wyrd declare wyrd.toml` operation resolves/registers the graph, completes
  required artifacts, derives bindings and selects the Repository version.
- One selected Repository Card version per external repository; historical
  versions remain readable. Unchanged declaration reuses versions; a separately
  published Verifier never silently changes policy. Failed declaration preserves
  prior selection; delayed older completion cannot overwrite a newer selection.
- A closed pull_request Trigger via `runs_on`; omitted runs_on remains explicit.
  Repository-wide PR bindings: opened/reopened/ready and head/base changes run;
  draft does not start; draft conversion/close stops unfinished reviews;
  metadata-only edits do not run. New source supersedes unfinished older review.
- Admission freezes Repository/Verifier/Trigger and PR pins. Each binding submits
  through the existing queue/engine. Retries keep those identities. Policy
  updates affect future reviews and do not automatically rerun all open PRs.

The user subsequently requested research on burst handling and successive commits;
record detailed operational proposals separately rather than treating this
approval as proof that implementation/capacity already exists.

## Approved declaration details

The user approved [the six-rule declaration contract](approved-declaration.md).
It settles inference/override precedence, selected-graph loading, ordinary Card
versioning, readiness/selection and racing-declaration conflicts, rollback,
complete binding lists including empty removal, and existing checks/receipts.
Use that record where earlier proposal wording below leaves these open. Exact
wire schemas remain specification work, not new product-policy questions.

## Detailed contract and remaining proposals

### 1. Authoring and identity

`wyrd.toml` retains its existing defaults and kind overrides. The user supplies
only the binding intent (correct spelling is `verifier`):

```toml
[repository.verified_by]
verifier = [
  "./review/code-review.yaml",
  { kind = "Verifier", name = "security-review", version = "1.2.0" },
]
runs_on = { kind = "pull_request" }
```

The user confirmed retaining `runs_on`. The verifier array and shared Trigger
are TOML authoring shorthand: lower them to one ordinary `verified_by` entry per
Verifier, each with that Trigger. The canonical Card spec still uses the existing
binding list with one `verifier` per entry. Do not change the wire field into an
array or introduce a second binding execution model. The pull_request Trigger
kind remains new capability requiring its own approved contract.

No explicit `[repository]` envelope, `[repository.metadata]` or
`[repository.spec]` table is required. Infer `apiVersion: wyrd/v1`,
`kind: Repository`, the initial metadata name, provider, canonical host and stable
provider repository ID. The user may override inferred metadata/spec fields;
validate overrides through the same typed contracts and current connection
proof. An override is not permission to change the Card kind or claim access.
The full envelope is the lowered standard Card, not required TOML boilerplate.

Recommended resolution flow:

1. Discover the containing Git repository and its `origin` remote as local hints.
   This Git-origin discovery is new work, not an existing CLI capability found
   by the research. Do not guess a different remote when origin is absent;
   explain the missing input and allow explicit repository identity override.
2. Pass the parsed host/owner/name hint to Wyrd. The server matches the authorized
   tenant App connection and performs provider lookup to obtain stable ID and
   canonical repository name. Clients do not receive App installation secrets.
   Provider lookup/identity validation is a reusable independent-library concern;
   tenant lookup and credential authority remain Wyrd's.
3. Infer initial Card name from the provider-confirmed repository name. If that
   name is invalid or collides with another repository's Card family, require an
   explicit metadata.name override rather than inventing suffixes. Subsequent
   declarations reuse the established Card-name association for this stable
   repository ID; a provider rename does not create a new Card family.
4. Combine defaults, inferred values and explicit overrides, then lower the
   normal Repository Card and resolve its Verifier graph. An existing-name or
   provider-ID disagreement is a typed refusal, not silent retargeting.
5. Fully explicit, authorized identity supports declaration outside a Git checkout;
   `--check` stays offline and reports deferred provider identity/authority checks
   rather than pretending to prove them.

Explicit metadata values override inferred metadata; explicit repository identity
is the target to verify, while Git remote information is only a default. The
server checks the explicit host/ID and connection instead of silently requiring
it to equal origin. This permits deliberate overrides without credentials or
SSRF bypass. Existing stored association is retained unless a separate future
rename operation is designed; changing metadata.name must not duplicate the same
provider repository under two Card families.

A normal YAML Repository Card is another authoring representation of the same
canonical Card and should use the same registration/activation lifecycle. Local
Verifier YAML owns normal Card metadata/spec; exact registered CardRefs remain
exact. Do not introduce another Verifier body format, a remote `latest` selector,
or path/ref discriminator. Single/multiple local definitions and registered
references converge through the same automatic declaration.

Use ordinary metadata version behavior. Omitted/scoped local authored versions
are resolved by existing registration; unchanged resolved content deduplicates,
changed content follows existing bump rules, and exact conflicting pins fail.
The server returns exact CardRefs/UIDs; no manual dependency-UID substitution.

Separate stable external identity (tenant + provider host + provider repository
ID), Card identity (tenant + Repository + authored name + Card version), and PR
source identity (PR number + immutable base/head pins). Repository rename at the
provider does not create another resource. Recommend one Repository Card name
line for an external repository within a tenant; forbid retargeting that name
to another host/ID. Separate tenants still have separate authorization/evidence.

### 2. Root and automatic binding ownership

Lower the inferred and overridden TOML repository definition to one normal
Repository Card submission. Its
`verified_by` references connect Verifiers and their transitive dependencies,
so it is the real graph root. Extend existing typed reference traversal,
composition validation and binding sites for this new owner; do not invent a
synthetic Service or a second non-Card declaration root/receipt.

Register through the existing composite Card declaration, version resolution,
artifact upload/completion and Card receipt machinery. Repository versions own
their derived bindings just as existing Card owners do. The server derives and
persists bindings from the resolved spec; users never issue a second bind command.
A referenced-only repository still declares the Repository Card root.

`runs_on: {kind: pull_request}` is a proposed new closed Trigger activation.
It can be inline or a registered Trigger reference under existing conventions.
Absent `runs_on` remains explicit-only; never silently activate an untriggered
binding. Do not introduce a generic webhook event vocabulary or move source
activation into Operators. Repository/Verifier kind and PR compatibility are
checked before activating the selected configuration.

### 3. Active review policy and updating it

Historical Card versions remain readable and can retain normal Active lifecycle
status. That status means their content is available, not that every version is
automatically selected for PRs.

Maintain one selected Repository Card UID per external repository in the tenant.
A successful authorized Repository declaration selects that version automatically
as part of the declaration lifecycle, after required Cards/artifacts are ready.
Old versions/bindings remain historical and are excluded from new PR selection.
A newer registered Verifier version alone never changes a Repository binding.

Changing a local review Card and running `wyrd declare wyrd.toml` can produce a
new Verifier version and a new Repository version because resolved dependency
identities enter the Repository spec hash. Repeating unchanged effective content
reuses versions and selection. Removing a binding in the new Repository version
removes it from future PR selection. The user approved an explicitly declared
empty `verified_by` list as intentional removal of all automatic review bindings,
matching Card optional binding conventions; show that outcome in the receipt.

Selection must be protected against delayed completion/reconciliation of an older
declaration overwriting a newer selected policy. The persisted declaration intent
and activation order must supply that fence. Do not select whichever historical
version happens to finish its blob write last. Explicitly declaring an old exact
Repository version is an intentional rollback and must be distinguishable from
recovery of an old declaration attempt. Conflicting concurrent selection and intentional rollback behavior are now
approved in the declaration record. Exact persisted coordination remains to specify.

No cross-storage transaction is promised. Failure leaves prior selected policy
in effect; any successfully registered dependency Cards remain valid resources.
A lost response/retry must recover or reapply the accepted selection without
creating duplicate logical activation. Completion must report Repository UID,
version and derived binding IDs in the normal receipt/status surfaces.

### 4. App connection is prerequisite authority

The operator configures the App and its secrets. An authorized tenant user
connects the installation and selects permitted repositories using provider proof.
Installing an App alone does not create a binding or authorize Wyrd execution.
Repository declaration verifies a current tenant connection for its host/ID and
binding/Verifier authority; it never accepts App secrets in a Card or TOML.
The binding retains its accountable owner; a GitHub event supplies no Wyrd user
credential and cannot grant SYSTEM additional authority.

Installation/credential rotation is connection state, not a Repository Card
version. Loss of provider access blocks new source IO/admission and cancels
in-flight work at existing cancellation boundaries while retaining evidence.
The [approved App flow](approved-runtime-and-github.md#operator-app-configuration-and-tenant-installation)
owns installation cardinality and onboarding. Connection uses one active
installation per tenant and one tenant per installation.

### 5. PR lifecycle and queued execution

Recommended first contract: repository-wide PR bindings, no branch filters yet.
Eligible actions: opened, reopened, synchronize and ready_for_review for open,
non-draft PRs. A changed base branch/commit causes a new pinned review; title/body
edits alone do not. Draft PRs do not start new review, conversion to draft stops
unfinished automatic reviews, and closure stops unfinished work. Event ordering
must be reconciled against current provider state, not assumed from arrival.

Verify and durably accept bounded webhooks, resolve tenant/repository connection,
then select the currently selected Repository Card and its PR-triggered bindings.
Freeze selected Repository/Verifier/Trigger identities and exact PR base/head at
logical review admission. Submit one ordinary queued execution per binding.
Retry of a delivery or logical admission must not create another execution.

New source revisions supersede unfinished older reviews. Old review results and
accepted evidence remain historical; an old worker cannot publish as the new
review. Binding updates affect future accepted reviews, not retries or already
running reviews. Recommend no automatic rerun of every open PR solely because
policy changed; explicit rerequest uses the policy selected for that new logical
review. Retry of the same rerequest retains its original selection.

The queued verification subject remains the exact Repository CardRef. PR number,
base/head commits and extraction pins are typed accepted evidence, not substitute
Card versions. Extraction uses the independent library through Wyrd adapters;
review steps use ordinary Verifier semantics and retained shared evidence.

GitHub Checks and PR review comments with suggested edits are approved in
[the runtime/GitHub record](approved-runtime-and-github.md). Keep one Check per
logical binding/review. Computation
and Check delivery have independent retry lifecycles; provider write failure must
not rerun the Verifier or change its verdict. Waiting keeps the Check nonterminal.
Exact Check identity and conclusion mapping still need a compact contract.

### 6. Remaining full-change contracts

Subsequent user approvals are authoritative design decisions in
[code-review support](approved-code-review-support.md) and
[runtime/GitHub integration](approved-runtime-and-github.md). Both GitHub delivery
surfaces, direct ineligibility, wait resource release and signal/approval/rejection
policies are settled; do not reopen them from older draft wording.

The [current design index](notes.md#open-questions) lists the remaining
schema, onboarding, durable-transition and scope-audit work. Exact public and
persisted contracts still require specification; private mechanics remain with
implementation. There is no fixed code-review runtime target.

## Authority updates required

Adding Repository expands the current 12-kind catalog, extends binding owners,
reference traversal and graph composition. Adding pull_request extends Trigger's
closed activation catalog and revises the current statement that external pushes
are not activation. The workspace config authority must admit the repository
root. These are explicit owner-document updates before spec approval, not silent
exceptions or new implementation authority. Standard Card envelopes, registration,
CardRef identity, server-owned bindings and existing queue/engine remain the model.

## Evidence

See the main notes' declaration trace. Additional inspected owners:
`wyrd-spec/src/graph/root.rs::pick_root`, `graph/composition.rs::binding_sites`,
`refs/mod.rs::ReferenceSlotVisitor`, `card/verifier/mod.rs::VerificationBinding`,
`card/trigger.rs::TriggerActivation`,
`wyrd-server/src/components/cards/service.rs::persist_node/BindingProjector`,
`wyrd-sql/src/queries/verification.rs::NewBinding/project_bindings`,
`wyrd-client/src/operator_connections.rs::OperatorConnections`.
Native non-Card connections are a valid Wyrd pattern for credentials/access, but
Repository as a versioned verification subject aligns with the Card-binding
contracts and avoids extending execution subject identity to an unrelated kind.

No production changes or runtime verification performed. The TOML example is
syntax-checked only; schema support is proposed.
