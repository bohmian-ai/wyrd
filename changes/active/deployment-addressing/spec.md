---
id: SPEC-deployment-addressing
revision: 5
status: approved
---

# Deployment onboarding, Prompt authoring, and Agent execution

## Objective and user value

Configure credentials and named gateway deployments once, author a Prompt
without connection details, and select execution on the Agent that uses it.
The same Prompt can execute natively or through a Wyrd deployment, including
one connected to an external gateway. Portable and provider-specific
content share Rust validation, rendering, and request construction.

| Owner | Declares |
|---|---|
| PromptSpec | Provider, model, content, inference settings, output schema |
| AgentSpec | Prompt reference, execution route, tools, bounded run configuration |
| Gateway deployment | Adapter, connection configuration, credential reference, model restrictions, access |
| Provider credential | Tenant-scoped upstream credential identity and protected secret source |
| Executing host | Native provider configuration, Wyrd connection/authentication, runtime resources |

## Revision and approval

**Revision 5 is approved.** The owner explicitly approved the written r5
revision on 2026-10-15 ("yes approve") after requesting one implementation
task. Approval covers the concrete requirements below; it does not claim
implementation readiness or completed verification. The
owner approved the direction in the current conversation: separate Prompt
`provider` and `model`; two content forms without duplicate fields; Agent
execution ownership; the name `execution_route`; named deployments; and
external gateways onboarded through Wyrd rather than an Agent bypass.
Examples and explicit edge/default rules below complete that direction for
review. They are approved target contracts, not claims about shipped behavior.

This revision incorporates `SPEC-prompt-authoring` r1 from
`change/prompt-authoring` at `944965af5` and replaces its conflicting addressed
model and qualified-format proposals. That spec is a draft, not a separately
approved dependency. Its worktree is not edited here. Approval of r5 records
that source draft as superseded authority for this consolidated change; do
not plan or execute its old task independently. Deployment-addressing
TASK-001 is replaced under `$wyrd-plan` against this approved revision; its
old r1 scope is not authority for this revision.

Revision 3 removes deployment model renaming by explicit owner direction.
The Prompt's model identifier is sent upstream unchanged; the selected
deployment must accept it.

Revision 4 records subsequent owner decisions: retain required execution
routes and the generated schema name; use a portable core of text, image,
and document content with broader settings; allow explicit invocation-time
route overrides; remove Workflow entirely; defer local Vertex gateway
support. Audio/video remain native authoring for now. Exact content/settings
shapes below complete those directions for review.

Revision 5 records explicit owner approval of the pre-run override contract.
It adds the requested simple local-development journey and confirms that the
existing deployment command accepts one deployment or a list. Existing
credential upload and upstream header injection are reused unchanged; no
combined credential/deployment command or new secret-input mechanism is added.

Revision 1's deployment/access intent remains. Its implicit deployment named
after Prompt provider and its separation of Prompt authoring into another
change are replaced. Historical WIP patches are reference material, not an
implementation prescription.

## Scope and non-goals

In scope: deployment onboarding/addressing; external OpenAI-compatible
gateway onboarding; invocation grants/discovery; two Prompt forms; format
validation and shared compilation; Agent execution routes/overrides; complete
Workflow product removal; readable dev/tenant/multi-deployment setup journey;
affected SDKs,
HTTP/CLI/MCP contracts, consumers, fixtures, examples, and documentation.

Out of scope:

- New server-hosted Agent submission/status/cancellation APIs, arbitrary code
  execution, or a replacement application runtime.
- Gateway model-alias CRUD or alias-based Prompt identity.
  `SPEC-gateway-aliases` remains separate and must be reconciled before alias
  implementation. Deployment model renaming is not introduced.
- Universal external gateway protocols/authentication, upstream model
  discovery/catalogs, capacity provisioning, or new cloud integrations.
- Embedding, Predict, and raw requests as Prompt authoring formats. Existing
  non-Prompt runtime callers retain those capabilities.
- Audio/video portable content and new local Vertex gateway support. Retain
  existing native/provider capabilities and server Vertex behavior; local
  Vertex gateway calls remain explicitly unsupported in this revision.

## Required behavior

### REQ-001 Named deployments and deterministic addressing

A deployment is tenant gateway configuration, not a new Card kind. Its name
selects exactly one configured adapter/credential binding. A direct inference
request addresses `<deployment>/<model>`: split at the first slash and
preserve the entire model suffix, including further slashes. Reuse validated
identifier types; refuse empty parts, controls, and unsafe path encodings.

New deployment body through the existing administration surface:

```json
{
  "name": "openai-primary",
  "adapter": "openai",
  "auth": {"bearer": {"credential": "openai-key"}},
  "access": "open"
}
```

- `models` is optional. Omitted means Wyrd applies no model allowlist, not a
  guarantee of upstream entitlement or model existence. A present list is
  nonempty and unique and restricts invocation to its validated identifiers.
- `capabilities` is optional. Omitted permits adapter-supported operations. A
  present list is nonempty and only narrows that set. Adapter support does
  not guarantee that every model supports that operation.
- `access` is `open | granted`, default `open`. It never means anonymous
  inference; REQ-002 applies.
- Remove the required single `model`, `routing_weight`, and weighted
  selection. Built-in adapters may have arbitrary valid deployment names.
- Adapter identity, credential-provider identity, and deployment name remain
  separate. Removing name restrictions does not weaken credential matching,
  tenant binding, endpoint validation, or managed-secret sealing.
- Missing deployments, disallowed models, and unavailable operations fail
  before provider IO with existing structured gateway error semantics.
- Fallbacks, pricing, budgets, and governance targets use deployment/model
  addresses. Failover is explicit fallback configuration, not weight or a
  provider-naming convention.
- `/v1/models` lists only explicit `models` lists, filtered by invoke
  visibility. Unrestricted deployments are not probed or exhaustively listed.

A second deployment can use a different credential and restrict access:

```json
{
  "name": "openai-dedicated",
  "adapter": "openai",
  "auth": {"bearer": {"credential": "dedicated-key"}},
  "models": ["gpt-4o"],
  "capabilities": ["chat_completions"],
  "access": "granted"
}
```

This does not provision upstream capacity. Upstream arrangements and adapter
configuration determine capacity; a second key is not a universal selector.

### REQ-002 Authorization, attribution, and visibility

`gateway:invoke` scopes are typed `all`, `{deployment}`, and
`{deployment, model}`. Deployment scope replaces provider scope.

| Effective subject grant | Open deployment | Granted deployment |
|---|---|---|
| `all` only | Allow | Deny |
| Matching deployment | Allow its permitted models | Allow its permitted models |
| Matching deployment/model | Allow that model | Allow that model |
| No matching grant | Deny | Deny |

Built-in invocation roles retain `all`. Administrative read/write does not
itself grant invocation. Invocation does not require administration read
permission merely to resolve a target.

Authorize the requested target before upstream IO. Authorize each fallback
separately; remove unauthorized fallbacks and retain their denied decisions.
A denied requested target is refused, not rescued by an authorized fallback.
Use the same coverage rules for model visibility and invoke-scoped batch
listing/access. Administrative listings retain their own permission checks.

Authority is the effective subject, including existing accepted bounded
verification-run authority. Delegation retains its permission intersection.
Actor/`executed_by` attribution never widens access; SYSTEM attribution grants
no authority. Every permission evaluation stages its allowed/denied decision
on the existing nonblocking process Scribe outbox. Audit failure never refuses
or delays inference.

### REQ-003 One onboarding and invocation workflow

Reuse existing credential/deployment creation, reads, updates, deletion and
credential revocation. Create credentials before referencing deployments.
Administration remains tenant-authorized/audited; secrets never enter Cards.

Credential file (`<upstream-provider-key>` is a placeholder):

```json
{
  "name": "openai-key",
  "provider": "openai",
  "source": {"managed_secret": {"secret": "<upstream-provider-key>"}}
}
```

```bash
wyrd gateway credential put --file credential.json
wyrd gateway deployment put --file deployment.json
wyrd gateway deployment get openai-primary
wyrd gateway deployment list
```

`wyrd gateway deployment put --file gateway.json` accepts either one
deployment object or a nonempty JSON array of the same deployment objects.
The command and object fields are the same in both cases. One invocation can
therefore configure OpenAI, Anthropic, Gemini, and Jev:

```json
[
  {
    "name": "openai",
    "adapter": "openai",
    "auth": {"bearer": {"credential": "openai-key"}}
  },
  {
    "name": "anthropic",
    "adapter": "anthropic",
    "auth": {
      "api_key_header": {"header": "x-api-key", "credential": "anthropic-key"}
    }
  },
  {
    "name": "gemini",
    "adapter": "gemini",
    "auth": {
      "api_key_header": {"header": "x-goog-api-key", "credential": "gemini-key"}
    }
  },
  {
    "name": "jev",
    "adapter": "jev",
    "auth": {"bearer": {"credential": "jev-key"}}
  }
]
```

The tenant supplies credential files through the existing credential command;
each referenced credential must already exist and be usable. At request time
the gateway resolves that credential and injects the configured upstream
authentication header, as it already does. Neither requests nor SDK clients
need the upstream key. This change does not add credential lists, environment
interpolation in deployment files, or automatic secret registration.

Validate the complete deployment file structurally before submitting writes;
reject an empty array, malformed entry, or duplicate deployment name without
writing an earlier entry. Apply valid entries through the existing authorized
per-deployment PUT lifecycle. Do not invent cross-request atomicity: if a
server-side write fails, stop, return nonzero, and report completed/failed/
unattempted names without secrets. An interrupted run leaves committed writes
intact. Rerunning the file converges through existing PUT semantics without
duplicate deployments; absent names are not deleted and credentials are not
rotated. A single-object file retains its current response shape; a list
returns an ordered per-entry outcome list. These batch failure/output details
are approved contract details, not an additional command/API.

HTTP writes remain:

- `PUT /v1/admin/gateway/provider-credentials/{name}`
- `PUT /v1/admin/gateway/provider-deployments/{name}`

Require body/path name agreement; retain existing read/delete/revoke routes.
The three SDKs project deployment CRUD. Credential mutation remains on its
existing privileged administration surface; SDK reads remain redacted.

Retain `managed_secret`, `environment`, and `external_secret` sources. Managed
secrets use the existing sealing keyring and tenant/name/provider binding.
Operator-controlled sources retain tenant/provider/exact-host assignments and
backend restrictions. Credential provider is not renamed to deployment name.

Built-in adapters retain their existing expected credential-provider identity
(`openai`, `anthropic`, `gemini`, `vertex`, or `jev`), independently of the
deployment name. OpenAI-compatible deployments retain a non-reserved custom
credential-provider identity such as `external`. Resolve that identity from
the referenced credential and preserve its SQL/provider assignment and sealed
secret binding; do not substitute the Prompt's provider or deployment name.
The explicit Prompt-provider/adapter matrix in REQ-010 governs inference
compatibility, separately from this authentication binding.

Callers use their Wyrd identity, never the upstream key. Existing stock-client
authentication remains supported. This example uses the existing Python auth
transport with the approved target address:

```python
import openai
from wyrd import WyrdClient

wyrd = WyrdClient()
client = openai.OpenAI(
    base_url=f"{wyrd.server_url}/v1",
    api_key="wyrd",
    http_client=openai.DefaultHttpxClient(auth=wyrd.http_auth),
)
response = client.chat.completions.create(
    model="openai-primary/gpt-4o",
    messages=[{"role": "user", "content": "Hello"}],
)
```

Use existing `WYRD_SERVER_URL`/saved connection/auth configuration. Static
Wyrd API keys remain accepted only by the existing inference ingress; this
adds no API-key authentication to management, MCP, or Bifrost.

### REQ-004 External gateways are deployments

A supported OpenAI-compatible external gateway uses the same lifecycle:

```json
{
  "name": "external-key",
  "provider": "external",
  "source": {"managed_secret": {"secret": "<external-gateway-key>"}}
}
```

```json
{
  "name": "support-gateway",
  "adapter": {
    "openai_compatible": {"base_url": "https://gateway.example.com/v1"}
  },
  "auth": {"bearer": {"credential": "external-key"}},
  "access": "open"
}
```

Agent selects `support-gateway` through `wyrd_gateway`, exactly as it selects
a direct-provider deployment. URL/adapter/credentials remain on the
deployment. Wyrd authorization, governance, audit, capture, and attribution
still apply. External credential provider identifies its authentication
destination; it need not identify the manufacturer of each served model.

Retain existing bearer, allowed API-key-header, and no-auth configurations.
This does not add arbitrary authentication or tenant-configurable native
Anthropic/Gemini endpoint overrides. Resolve/screen/pin destinations before
attaching secrets; retain redirect and reserved-header protections. Never
forward Wyrd identity credentials upstream.

Send the Prompt's model identifier upstream unchanged. For a direct gateway
request, remove only the deployment prefix from the addressed model. The
selected deployment must accept the remaining model identifier. No deployment
model-renaming field, alias resolution, or substitution is introduced.
Authorize, price, govern, and record the resolved deployment/model target.
Existing explicit fallback behavior remains governed by REQ-010 and records
each actual target; it does not rewrite the stored Prompt or its hash.

### REQ-005 Prompt provider and model occur once

Both Prompt forms require top-level `provider` and `model`. Canonical built-in
providers are `openai`, `anthropic`, `google`, `vertex`, and `jev`. Custom
providers use validated registered names. Provider identifies the inference
family/configured native provider, not a deployment, credential, or format.
Model is the provider model identifier, not a Wyrd deployment address or
alias. Preserve slash-containing identifiers; never infer deployment from a
Prompt model prefix.

Prompt has no route, deployment, URL, credential, or alias binding. Different
Agent versions may reference the same exact Prompt with different routes.
Portable means common authoring vocabulary, not universal model capabilities.
Provider/model and route/deployment identities are literal configuration, not
template variables. Rendering cannot select another inference target.

### REQ-006 Portable authoring

Portable form has shared fields and no native `body` or `format`:

```yaml
apiVersion: wyrd/v1
kind: Prompt
metadata:
  name: support-answer
  version: 1.0.0
  space: default
spec:
  provider: openai
  model: gpt-4o
  system: Answer the customer's question clearly.
  messages:
    - role: user
      content: "{{question}}"
  settings:
    temperature: 0.2
    max_output_tokens: 256
  response_format:
    type: object
    properties:
      answer: {type: string}
    required: [answer]
    additionalProperties: false
```

`system` is optional text. `messages` is an ordered list of turns with
`role: user | assistant`. `content` is either a text string or an ordered
list of typed content blocks. A string is the convenience form of one text
block, not another provider-specific authoring path. Preserve the authored
representation on registration/GET. Require nonempty text or media content
in at least one system/message. Refuse portable system-role messages: system
text has one location. Runtime tool turns belong to the Agent loop.

The portable content core is text, image, and document. Audio/video stay in
native authoring for now. Block fields are:

| `type` | Content fields |
|---|---|
| `text` | `text: string` |
| `image` | `source: MediaSource` or one isolated `${media:name}` binding |
| `document` | `source: MediaSource` or one isolated `${media:name}` binding |

Reuse existing shared `MediaSource` semantics: `{type: url, url, mime_type?}`,
`{type: base64, mime_type, data}`, or `{type: file, uri, mime_type?}`. Bindings
reuse the existing typed MediaRef input and validate its kind against the
block. Do not add provider-specific image/document fields to portable blocks
or invent a second upload/secret/media-fetch subsystem.

Multimodal Prompt spec-body example:

```yaml
provider: openai
model: gpt-4o
messages:
  - role: user
    content:
      - type: text
        text: "Compare this photograph with the attached document. {{question}}"
      - type: image
        source: "${media:photo}"
      - type: document
        source: "${media:document}"
settings:
  max_output_tokens: 1024
```

Compile blocks in order through existing media/provider mappings: OpenAI
content parts, Anthropic content blocks, and Google/Vertex parts. Base64
requires MIME type and valid bounded data. File IDs/URIs remain scoped to
their provider; changing provider never converts or reuploads them. Preserve
existing restrictions, including Google's accepted file/GS URI forms and
required MIME types, and OpenAI document-source restrictions. Refuse
unsupported kind/source/role combinations before dispatch where detectable;
do not fetch, flatten, OCR, drop blocks, or substitute URLs to fake support.
Provider/model-specific refusals remain structured errors. Registration and
loading perform no media IO; existing authorized runtime media resolution
retains its tenant, size, SSRF, and secret boundaries.

`settings` is optional and typed, not arbitrary provider-native pass-through.
The initial portable vocabulary covers eight settings, not only the lowest
common denominator. Each provider supports the entries below; `unsupported`
means an explicit field-specific refusal, never a dropped setting:

| Setting | OpenAI Chat | Anthropic Messages | Google/Vertex GenerateContent |
|---|---|---|---|
| `temperature` | `temperature` | `temperature` | generation config `temperature` |
| `top_p` | `top_p` | `top_p` | generation config `topP` |
| `max_output_tokens` | `max_completion_tokens` | `max_tokens` | generation config `maxOutputTokens` |
| `top_k` | unsupported | `top_k` | generation config `topK` |
| `stop_sequences` | `stop` array | `stop_sequences` | generation config `stopSequences` |
| `seed` | `seed` | unsupported | generation config `seed` |
| `presence_penalty` | `presence_penalty` | unsupported | generation config `presencePenalty` |
| `frequency_penalty` | `frequency_penalty` | unsupported | generation config `frequencyPenalty` |

Validate finite/range-safe protocol values: temperature within the destination
protocol's range, `top_p` in [0, 1], positive integer token/top-k limits,
signed integer seeds within the destination representation, finite penalties
within destination ranges, and nonempty bounded stop strings/list. Seed is
best-effort upstream reproducibility, not a determinism guarantee. Anthropic
requires explicit `max_output_tokens`; invent no token budget. Native-only
settings such as provider reasoning controls use the native form. Unknown,
unsupported, or colliding settings fail explicitly. Model-specific upstream
refusals remain structured errors; maintain no local model catalog.

`response_format` is one JSON Schema object usable by Agents and judges.
Shared compilation maps these fields:

| Field | OpenAI Chat | Anthropic Messages | Google/Vertex GenerateContent |
|---|---|---|---|
| `system` | leading system message | `system` | `systemInstruction` |
| user/assistant turns | messages | messages | contents, user/model roles |
| `response_format` | JSON-schema `response_format` envelope | `output_config.format` | generation config JSON MIME type and response schema |

Use the existing typed wire owners for actual casing/envelopes. OpenAI schema
name is deterministic `response`, with `strict: true`; no duplicate authored
name/schema or Card identity is required, so inline/standalone Prompts compile
identically. Refuse unsupported schema features rather than weakening the
requested schema. Portable OpenAI uses Chat Completions; Responses uses native
authoring in this revision. These approved rules replace
the earlier prompt-authoring mapping questions.

### REQ-007 Native authoring and provider/format matrix

Native form has the same identity plus short `format` and native `body`:

```yaml
apiVersion: wyrd/v1
kind: Prompt
metadata:
  name: support-answer-native
  version: 1.0.0
  space: default
spec:
  provider: openai
  model: gpt-4o
  format: chat_completion
  body:
    messages:
      - role: user
        content: "{{question}}"
    temperature: 0.2
    response_format:
      type: json_schema
      json_schema:
        name: response
        strict: true
        schema:
          type: object
          properties:
            answer: {type: string}
          required: [answer]
          additionalProperties: false
```

Responses spec-body example:

```yaml
provider: openai
model: gpt-4o
format: responses
body:
  input: "{{question}}"
  max_output_tokens: 256
```

| Provider | Native `format` | Portable default |
|---|---|---|
| `openai` | `chat_completion`, `responses` | `chat_completion` |
| `anthropic` | `messages` | `messages` |
| `google` | `generate_content` | `generate_content` |
| `vertex` | `generate_content` | `generate_content` |
| `jev` | `system_one` | none; native only |
| registered custom provider | admitted generation formats its implementation declares | only if it explicitly supports a listed portable mapping |

Validate the pair. Google/Vertex share a body format but retain different
destination/auth behavior. Gateway adapter `gemini` maps explicitly to Prompt
provider `google`; it is not inferred from deployment name. Custom provider
names require runtime registration; validate capabilities at use when they
cannot be resolved during registration. This adds no runtime plugin protocol.

`format` is required in native form and absent in portable form. Existing
internal request discriminators may keep qualified names. One shared typed
mapping projects the public pair into those owners. Qualified names such as
`open_ai_chat_completion` are not accepted as alternate authored spellings.

Native body is the protocol's content/options excluding model and
transport-owned auth/routing fields. Refuse `body.model` even when equal to
`spec.model`. Insert the model into Chat/Responses/Messages at dispatch, or
into the GenerateContent URL. Authored body is intentionally not a complete
upstream HTTP request.

Refuse portable `system`, `messages`, `settings`, and `response_format`
beside native body. Native structured output is declared only in its protocol
field. Preserve valid native options, including supported extension fields;
never silently discard unknown options through typed decoding. Refuse invalid
or unsupported native content wherever detectable. Raw request APIs do not
bypass Prompt validation.

### REQ-008 Authored storage, derivation, shared compilation

Register/GET/hydrate/local save/load preserve authored JSON fields and values
in either form. YAML formatting/comments are not durable. Hash authored spec
JSON through existing canonical hashing; do not store compiled requests or
add derived fields. Both forms work standalone and inline at `Agent.prompt`
using existing untagged Ref/path/native-inline conventions.

Derive variables/media variables from existing placeholder grammar, and
response metadata/schema from portable `response_format` or the native output
field. Remove authored/stored `variables`, `media_variables`, `response_type`,
the `request` wrapper, and duplicate models. Keep derived introspection on
hydrated objects in all three SDKs.

Construct the provider request at dispatch after rendering and destination
validation. One shared Rust implementation owns provider/format validation,
portable mappings, rendering, output-schema derivation, and model insertion.
File/SDK/HTTP/MCP authoring must not duplicate provider assembly tables. New
or materially relocated PyO3 belongs in the Python SDK.

Registration/loading validates shape, identifiers, known pairs, placeholders,
settings, schemas, and native content without discovery, secret resolution,
credential exchange, or inference. Destination availability and runtime-only
custom capabilities are validated at use. Parsing/planning stays synchronous;
foundations stay IO/PyO3-free.

### REQ-009 AgentSpec execution configuration

AgentSpec, AgentCard, runtime and SDK projections gain `execution_route`.
It is an explicit required field in the revised authored/durable contract.
Omission fails instead of silently selecting native. Exactly two variants:

```yaml
execution_route:
  kind: native
```

```yaml
execution_route:
  kind: wyrd_gateway
  deployment: openai-primary
```

The required-field rule is explicitly confirmed by the owner. Native
accepts no deployment/URL/credential. Gateway requires one deployment name,
not an adapter name or full model address. Reject Agent `llm_route` and
`ext_gateway`; provide no direct external bypass.

Complete Agent referencing the portable Prompt:

```yaml
apiVersion: wyrd/v1
kind: Agent
metadata:
  name: support-agent
  version: 1.0.0
  space: default
spec:
  prompt:
    kind: Prompt
    name: support-answer
    version: 1.0.0
    space: default
  execution_route:
    kind: wyrd_gateway
    deployment: openai-primary
  tool_names: []
  run_config:
    max_iterations: 5
    timeout_ms: 30000
```

For native execution, author another Agent with
`execution_route: {kind: native}`. For an external gateway, select
`execution_route: {kind: wyrd_gateway, deployment: support-gateway}`. Its
Prompt is unchanged. Changing the persisted Agent route changes its spec
under normal Card versioning. An explicit invocation-time override changes
only that invocation, not the stored Card, hash, version, or another run.

Invocation APIs accept optional `execution_route` with the same two-variant
shape as AgentSpec. Omission uses the required Agent route; presence replaces
the entire route for that invocation. Do not merge partial fields or choose
an implicit environment override. Proposed Python invocation example:

```python
result = agent.run(
    "Help with this order",
    execution_route={"kind": "wyrd_gateway", "deployment": "openai-backup"},
)
```

All first-class SDKs project the same option through their existing Agent
execution surfaces. Resolve one immutable effective route at invocation
start, independent of concurrent invocations. Validate and authorize its
actual target through the ordinary path, with no privilege inherited from
the default deployment. Record the declared and effective routes and the
actual attempt targets in execution/capture evidence. No mid-loop global
mutation is introduced. A server execution context still refuses a native
override. Background/judge execution uses its declared route unless its
existing invocation contract explicitly supplies the override; never recover
one implicitly from unrelated observation content.

All Agent entry points, including local SDK execution, existing observed
invocation, judge execution and Service composition, honor the effective
Agent route throughout its bounded tool loop. Reuse existing Agent callbacks,
tools, sessions, journal, structured output, verification, timeout and
cancellation. Sub-agents use their own declared routes under existing caller
boundaries; references grant neither credentials nor permission.

Reuse/move existing routing capability to an Agent/shared-runtime ownership
boundary before removing Workflow under REQ-012. Do not require Workflow to
execute an Agent or invent another loop.

### REQ-010 Execution resolution and failures

Native uses the executing host's provider configuration named by Prompt
provider. Missing configuration fails before provider IO. Existing
server-controlled execution refuses native Agents before inference: never
expose server process credentials through this new route. No general server
Agent job API is added.

Gateway combines Agent deployment with Prompt model, resolves it in the
caller's tenant, validates compatibility, authorizes, and invokes the existing
gateway. Local execution uses authenticated public ingress; server consumers
reuse its service decision path. Derive neither deployment nor adapter from
Prompt provider or reserved prefix. Clients use shared Wyrd transport/auth;
resolution must not require administrative read permission.

Initial requested-target compatibility:

| Prompt provider | Compatible requested deployment adapter |
|---|---|
| `openai` | `openai`, `openai_compatible` |
| `anthropic` | `anthropic` |
| `google` | `gemini` |
| `vertex` | `vertex` |
| `jev` | `jev` |
| custom | compatible adapter for its declared admitted generation format |

An external compatible deployment accepts OpenAI-shaped Prompts regardless of
its separate credential-provider label. It does not promise arbitrary native
Anthropic/Gemini external support. Never silently replace Prompt provider to
fit a deployment.

Preserve supported direct gateway protocol translations. Explicit tenant
fallback policy may select a different adapter only with existing supported
conversion, authorization and actual target capture. Refuse unsupported
native options/tools/output requirements rather than silently dropping them.
Jev's existing no-general-tools/no-streaming constraints remain.

Addresses must survive shipped supported ingress: Anthropic, Gemini and Jev
Agent calls must not strip deployment and reconstruct provider names.
Body-model protocols carry deployment/model in the public model field until
resolution. GenerateContent carries it in the protocol model resource;
validate the deployment segment and preserve the complete model suffix before
constructing upstream URLs. Choose Google/Vertex by adapter, not prefix.
Preserve existing server-side Vertex support and native Vertex execution.
Do not add local public Vertex gateway ingress in this change. A local
`wyrd_gateway` route targeting Vertex fails explicitly before provider IO,
including when chosen by runtime override; it must not silently use Gemini
or native execution. This is an explicit deferred capability, not a claim of
local/server parity for Vertex.

Timeout/cancellation bounds every iteration/call. Configuration, credential,
authorization and dispatch failures do not switch execution routes. Before-
model callbacks/replacement requests cannot escape the bound route, credential
or authorized target; model changes must not bypass target checks. Native tool
declarations cannot silently widen Agent executable tools: reject conflicts
with Agent/host tool policy. Preserve supported runtime tool injection.

### REQ-011 Public errors and consumer completion

HTTP, CLI, MCP where exposed, schemas, and Rust/Python/TypeScript SDKs project
the same typed contracts. Helpers may be idiomatic, not alternate wire shapes.
Use the derive-backed public error catalog and field-specific remediation.

Distinguish unsupported/duplicate/mixed Prompt fields, invalid provider/model,
invalid provider/format/settings/schema, missing/invalid execution route,
unavailable target/capability, incompatible target, permission denial, and
unavailable/revoked credential. Reuse matching existing errors; add catalog
entries where necessary. No language-specific validation/error semantics.
Refuse before storage/IO when information is available; never disclose secret
material or secret locations. Unknown fields are errors, not ignored input.

Update every affected Prompt/Agent consumer, especially judges, inline
Prompts, SDK builders/stubs/exports, loaders, fixtures, support-desk,
quickstart, gateway setup and stock-client docs. Regenerate contracts from
owners and verify served OpenAPI. Remove duplicate-variable reconciliation
errors only after their declaring fields and reachable callers are gone.

### REQ-012 Remove Workflow as a product capability

Remove Workflow entirely in this change, not just its route fields: the
registrable Workflow Card kind/spec, generic Workflow orchestration runtime,
server Workflow-run submission/status/cancellation, client handles,
SDK/CLI/UI surfaces, permissions, configuration, reference slots, generated
contracts, examples, and capability-specific build/test wiring. Remove
`OperatorAction::Workflow`/`workflow_ref` rather than adding a replacement
dispatch kind. Remove Workflow from Service component support, Card-scope
reachability, registry kind catalogs, principal/observation validation, and
other enumerations. The native registrable catalog becomes 14 kinds; no
replacement Card kind is introduced. Retired input is rejected through the
existing unknown/unsupported surface semantics, with no compatibility alias,
decoder, or forwarding route.

Before removing owners, preserve reusable Prompt compilation, provider
dispatch, gateway adapters, Agent loop/callback/session behavior, and judge
execution at their existing surviving owners or the narrowest appropriate
Agent/shared-runtime boundary. Delete obsolete generic DAG/job orchestration,
not those reusable capabilities. Users compose Agents in their own application
code; do not substitute another Wyrd orchestration API or silently turn a
multi-step Workflow into one Agent.

Retain Eval's typed evaluation task DAG, conditions, assertions, pass gates,
and Agent-based judges: these are verification execution, not the removed
Workflow product. An Eval observation-schema hint or Agent-payload JSONPath
field does not reference a Workflow Card merely because its existing name
contains `workflow`. Preserve those independent contracts and their extraction
behavior; removing the Workflow product does not authorize unrelated field
renames or deleting verification capabilities.

Rewrite examples that demonstrate Agent inference/verification through direct
Agent calls. Remove examples whose only purpose is the retired orchestration
capability. Retain the behavioral tests for surviving Agent, gateway, and
verification properties at their owners; do not delete a failing surviving
behavior test to make removal pass. Remove checks only where their protected
Workflow property is no longer reachable, and document that justification.
Update architecture and repository kind inventories cohesively; ordinary
references to users' workflows or unrelated evaluation DAGs are not a name-ban
target. No deployed database/data deletion is authorized by this spec edit.

### REQ-013 Simple dev-to-gateway user journey

Ship a short, executable walkthrough and a real user-journey test with the
clarity of `wyrd-example-ts`: show the commands a user runs and assert the
result at each stage. The reference repo is a UX example, not authority for
old deployment/Prompt shapes. Keep this journey in Wyrd's existing example
and installed-CLI/local-development test ownership; no external repo edit or
new application framework is required.

Document prerequisites honestly: an installed CLI/package, supported local
PostgreSQL with its normal configuration, and the user's provider credential
files. Starting Wyrd with one command does not imply automatic database
provisioning. With those prerequisites, terminal one runs only:

```bash
wyrd server dev --tenant acme
```

Reuse its existing installer, migration, readiness, tenant setup, sealing-key,
saved-client configuration and graceful-stop workflow. It creates the initial
tenant on first use and makes fresh CLI/SDK processes ready to authenticate.
Do not require a separate server install, manual migration, platform API call,
tenant creation command, or copying the tenant key into the example program.
The public dev workflow retains its normal release acquisition; the test uses
the existing local installed-artifact fixture rather than depending on a
published release or downloading arbitrary code.

In another terminal, the tenant uploads the ordinary credential files through
the existing command, then submits all four deployments in one operation:

```bash
wyrd gateway credential put --file openai-credential.json
wyrd gateway credential put --file anthropic-credential.json
wyrd gateway credential put --file gemini-credential.json
wyrd gateway credential put --file jev-credential.json
wyrd gateway deployment put --file gateway.json
wyrd gateway deployment list
```

Each credential file uses the existing object shape illustrated in REQ-003,
with its corresponding name/provider/key. `gateway.json` is the four-entry
array shown there. Keep actual provider keys out of committed example files
and captured output. This is existing credential onboarding followed by one
multi-deployment command, not a new credential/config system.

The test starts from a fresh isolated config directory and database, invokes
the real CLI/server processes, and waits for actual readiness with bounded
polling. It must not replace the startup/tenant/bootstrap steps with direct
database inserts or test-client setup helpers. Reuse the repository's managed
Postgres and installed-CLI harness for dependencies/lifecycle, including
cleanup on timeout/failure. Logs retained on failure must be scrubbed.

After setup, use a fresh TypeScript SDK/client process with the saved Wyrd
connection and credential, without passing the bootstrap admin key through
test-only client constructors. Perform one supported inference operation
against each of the four named deployments. Use controlled provider fixtures
and the existing test-only upstream endpoint configuration, not paid/live
keys and not production SSRF-policy exceptions. Include Jev's supported
request shape instead of assuming every adapter supports a general chat/tool
operation. Assert response, resolved deployment, unchanged model identifier,
and receipt of the expected upstream credential header with no Wyrd identity
credential forwarded. Query retained gateway evidence through the normal
client read path rather than relying only on a mock's request log.

Prove a single-object deployment file still works; list setup creates all four;
reapplying updates/converges without duplicates; malformed list/duplicate
names fail before writes; missing/revoked credentials and insufficient grants
are rejected with truthful partial-progress reporting where applicable.
Stop/restart dev with the same tenant and state to prove tenant/key reuse and
continued upstream credential decryption, not key reissuance. The walkthrough
uses the same command/config contract as the test and remains short enough to
follow without reading harness internals. This TypeScript-led setup journey
complements, rather than replaces, the three-SDK contract journeys above.

## Architecture and implementation boundaries

This map identifies existing capabilities and required changes, not private
helper signatures, module layouts, or an implementation task sequence.

| Existing owner | Capability and extension |
|---|---|
| `crates/wyrd-spec/src/gateway` and auth scopes | Pure deployment/credential contracts; new addressing/access without weakening credential trust |
| `crates/wyrd/wyrd-gateway` | Snapshots, credential resolution, dispatch/fallback/pricing/governance; deterministic named targets |
| `crates/wyrd/wyrd-server/src/components/gateway` | Management/inference, subject authorization, visibility, audit/capture; all ingress paths |
| `crates/wyrd/wyrd-sql` | Tenant gateway persistence; replace model/weight storage, retain credential identity/FK integrity, persist access |
| `crates/skald/skald-spec` | Prompt values, native wire bodies, render/PromptDraft compilation; authored forms and shared pure compilation |
| `crates/skald/skald-prompt` | Prompt builders/output helpers; consume shared compilation instead of duplicate assembly |
| `crates/wyrd-spec/src/card/prompt` and `card/agent.rs` | Card codec, validation, hashing, references, new Prompt/Agent contracts |
| `crates/skald/skald-agent` | Existing dependency-owning Agent and bounded loop; retain/honor execution route |
| `crates/skald/skald-runtime` and current routing adapters | Provider registry/dispatch; preserve reusable execution before deleting Workflow owners |
| `crates/shared/wyrd-client` | Existing Cards loading/hydration, auth/transport, observed invocation; compose Agent and gateway owners |
| CLI server dev/gateway owners and existing installed-CLI local-dev journeys | Reuse real migrate/start/setup/saved-auth lifecycle; accept single/list deployment files through the existing command and test the user path |
| Vala judge consumers | Existing constrained Agent judgment; new Prompt forms/route under accepted subject authority |
| Workflow owners across contracts, runtime, server, shared client and surfaces | Remove the product end to end; preserve surviving Agent/gateway/judge capabilities and evaluation task semantics |
| SDKs, CLI/MCP, generators, UI readers | Contract projection, authored Python typing, regenerated outputs; no duplicate runtime |

Current branch has single-model weighted deployments, no Agent route field,
duplicated top-level/native-body model, and separate PromptDraft/builder
provider assembly. `Agent::with_provider_registry` already routes the complete
existing loop through a provider override; Workflow, judges and observed
invocation consume that seam. Extend/reuse these complete workflows rather
than introduce replacement orchestration around isolated HTTP calls.

Preserve the Card envelope/reference model. Persist authored Prompt form and
Agent route; gateway configuration stays outside registry Cards. This is an
unshipped breaking contract change without compatibility decoders. Existing
defining migrations may be edited in place per r1; this does not authorize
altering deployed data or production databases during specification work.

Reconcile authority before or with implementation of the approved contracts:

| Authority passage | Required replacement |
|---|---|
| `architecture/wyrd-design.md`, Prompt/reference examples | Two stored authored forms, provider/model once, short formats, dispatch compilation |
| Same, Agent | Explicit execution_route, separate from host secrets/config |
| Same, Workflow catalog/spec/job/routing/reference sections | Remove Workflow entirely; Agent owns execution; preserve surviving verification task semantics |
| Same, gateway/identity | Named targets, open/granted scopes, external deployments, subject/actor and current nonblocking outbox |
| `architecture/wyrd-doctrine.mdx`, route summary | Agent declares route; host supplies runtime dependencies; external gateways use deployments |
| Doctrine, AGENTS.md and reference kind inventories | Remove Workflow kind and product capability; reconcile 14 native kinds and surviving ownership |

These are explicit approved replacements of architecture prose, not permission
for implementation, generated artifacts, and documentation to diverge.

## Invariants

- INV-001 Preserve tenant isolation through credentials, lookup, caches,
  registration, authorization, capture and listing. Verified identity supplies
  tenancy; retain TenantConn/RLS ownership.
- INV-002 Protect credentials through existing keyring/binding owners. Never
  put secrets on Cards, expose/log them, or forward Wyrd identity upstream.
  Loading/registration resolves no execution secret.
- INV-003 No compatibility routes, authored aliases/legacy decoders,
  provider-scope shim, weighted selection or old single-model adapter.
- INV-004 Prompt identity/content remains stable across route changes. Agent
  persisted route changes follow versioning; invocation overrides preserve the
  Card/hash and record the effective route. Fallbacks preserve authored hashes
  and actual-target evidence.
- INV-005 Durable server decisions remain Rust-owned. Skald reusable execution
  is independent of Vala/server IO; SDKs share wyrd-client behavior.
- INV-006 Permissions block; audit/capture use the existing process outbox,
  never a new synchronous acceptance dependency, writer, WAL or queue.
- INV-007 No hidden native fallback, credential escalation, cross-tenant
  resolution, unsupported-option loss, or implicit callback/environment route
  override. Explicit invocation overrides are revalidated and authorized.

## Acceptance criteria and evidence

- AC-001 Rust/Python/TypeScript real gateway journeys prove onboarding,
  credential protection, unrestricted/allowlisted models, capability defaults,
  two independently addressed same-adapter deployments, no weighted selection,
  and resolved-deployment evidence. Controlled local upstream fixtures avoid
  paid calls/credentials.
- AC-002 Journeys plus coverage-table tests prove all/open/granted and exact
  model grants; requested-target denial; denied fallback exclusion; subject
  versus actor/SYSTEM; tenant-separated model/batch visibility. Rejected calls
  make no upstream request.
- AC-003 External deployment journeys prove forwarding the model identifier
  unchanged, resolved-target authorization/pricing/evidence, revocation, redaction,
  destination screening, and absence of forwarded Wyrd credentials.
- AC-004 Three-SDK register/GET/hydrate journeys cover both Prompt forms,
  standalone/inline authored JSON/hash preservation, and derived introspection
  without stored duplicate fields.
- AC-005 Three-SDK Agent journeys execute the same Prompt natively, through a
  named Wyrd deployment, and through an external deployment. Cover supported
  tool iterations, structured output, timeout/cancellation, incompatibility,
  missing route/provider/deployment, and permission denial. Use local provider
  fixtures for native execution; real Wyrd server/Postgres for gateway paths.
  Cover default versus explicit invocation route, authorized/denied override,
  recorded effective target, concurrent-run isolation, unchanged stored
  Agent/hash, and refusal of native overrides in server execution contexts.
- AC-006 Compiler tests cover every built-in generation pair/default,
  settings/schema mapping, and native-option preservation. HTTP/SDK negative
  journeys cover removed/mixed fields, body.model, qualified authored formats,
  invalid settings/schema, and rejected Agent llm_route/ext_gateway. Cover
  slash-containing models and GenerateContent resource safety. Exercise text,
  image/document blocks and typed media bindings across supported providers;
  preserve order/content and reject unsupported MIME/source/role combinations
  without provider IO or silent conversion. Exercise all eight portable
  settings, including explicit provider-specific unsupported entries.
- AC-007 Agent-based judge journeys cover both forms, parsed structured output
  and dependent assertions, accepted subject authority and SYSTEM attribution.
  Prove no server-native credential escape and no Workflow dependency for
  standalone Agent execution.
- AC-008 Native/public ingress journeys prove named Anthropic/Google/Jev
  targeting where supported, adapter-based selection, local/server parity,
  and explicit tool/streaming/format refusals. Retain supported direct-client
  conversion and existing native/server Vertex coverage. Local Vertex gateway
  calls, including runtime overrides, fail before IO with the documented
  unsupported behavior; new local Vertex ingress is not an acceptance goal.
- AC-009 Architecture/docs/examples/SDK exports/stubs/generated contracts,
  CLI/MCP catalogs and served OpenAPI agree. Every affected inline, judge and
  surviving consumer uses the revised contract. Unit tests or codegen alone
  do not close user-facing obligations.
- AC-010 Real SDK/HTTP/CLI contract evidence proves Workflow registration and
  execution are no longer exposed; generated catalogs/OpenAPI, reference slots,
  Service/Operator choices, permission inventories and UI contain no retired
  capability. Agent and judge journeys still pass independently. Independent
  Eval task DAGs/observed-data assertions retain their behavior and fields. Prove
  removal of unreachable Workflow-only wiring without weakening a live gate.
- AC-011 Installed-CLI/TypeScript-led user journey proves a single
  `wyrd server dev --tenant acme` invocation starts dev and creates the first
  tenant; fresh clients authenticate from saved configuration; ordinary
  credential uploads plus one deployment-file command configure OpenAI,
  Anthropic, Gemini and Jev; requests succeed through all four and retained
  evidence identifies each deployment and upstream authentication. Controlled
  fixtures require no live provider credentials. Walkthrough and test use the
  same visible steps.
- AC-012 Deployment-file journeys prove single/list decoding, complete-file
  validation, duplicate/empty rejection without writes, rerun convergence,
  truthful partial failure, no unrequested deletion/credential rotation, and
  unchanged per-item authorization. Dev restart preserves tenant credentials
  and decrypts existing provider keys through the existing sealing owner.

Use scoped `mise` lanes for implementation iteration. The final integrated
change removes a Card kind, runtime, cross-surface contracts and build/test
wiring, so `mise run gate` is the required final aggregate under AGENTS.md.
Its current checks, Rust families/journeys, gateway, Python and TypeScript
shards compose the relevant coverage; do not repeat its component lanes as
additional final gates. The standalone `test:gateway:journey` remains useful
for iteration; its hidden `test:gateway:gate` subset omits SDK journeys, which
the aggregate's SDK shards own. Preserve served OpenAPI coverage through its
owning integration suite. Planning must add the new Prompt/Agent/removal
journeys to the appropriate owners rather than assume existing selectors
already prove them. Add no permanent check for a property protected by a
typed contract or ordinary behavioral test.

Extend `mise run test:local-dev:journey` and the owning gateway journey lane
for REQ-013 rather than duplicating installed-server orchestration. The current
gate already includes that local-dev lane; keep the new scenario selected
there and by the relevant first-class SDK verification tasks. Named tests and
their exact focused commands belong in planning after approval.

These are evidence classes, not invented named tests. Tasks must supply exact
focused commands for tests they name and use repository environment wrappers.

## Open material decisions and approval boundary

None. The owner approved written revision 5 on 2026-10-15, including the
portable content/settings representation and support matrix, required Agent
route and explicit pre-run override, batch deployment input/output/failure
behavior, complete Workflow-removal scope, and dev/tenant/four-provider
journey. Model renaming and local Vertex gateway extension remain excluded.

Later semantic changes require a revised draft and renewed approval. Normal
private implementation choices remain implementation-owned. Approval does not
waive architecture reconciliation, test evidence, or independent review.

## Revision history and authority

| Revision | Status/history | Material change |
|---|---|---|
| 1 | Approved direction 2026-10-14 from SPEC-example-journey-fixes r6 REQ-009 | Named deployments, unrestricted models, open/granted invoke access; separate Prompt/alias work |
| 2 | Draft 2026-10-15 | Consolidated onboarding, Prompt authoring, Agent execution; provider/model once, short formats, external deployments, execution_route |
| 3 | Draft 2026-10-15; model renaming removed by owner direction | Send the Prompt model unchanged; remove deployment model mapping, associated persistence and evidence requirements |
| 4 | Draft 2026-10-15; revised by owner direction | Portable text/image/document core and eight settings; explicit runtime route overrides; required route/schema name retained; full Workflow removal; local Vertex gateway work deferred |
| 5 | Approved 2026-10-15 by explicit owner confirmation | Existing deployment put accepts one object or a list; unchanged credential upload/injection; real dev-start/tenant/four-provider setup journey |

Governing sources:

- [Repository standards](../../../AGENTS.md),
  [agent rules](../../../architecture/agent-rules.md),
  [protocol authority](../../../architecture/wyrd-design.md),
  [public doctrine](../../../architecture/wyrd-doctrine.mdx).
- [Reference router](../../../architecture/references/README.md),
  [architecture constraints](../../../architecture/references/doctrine/architecture-constraints.md),
  [ownership patterns](../../../architecture/references/architecture/patterns.md),
  [spec lifecycle](../../../architecture/references/languages/spec-driven-development.md).
- Source packet: `changes/active/prompt-authoring/spec.md` r1 at `944965af5`;
  related alias packet at `46eeff36b`. Provenance does not grant these packets
  authority over this revision.
- Existing startup owner: `crates/wyrd/wyrd-cli/src/server/dev.rs`; setup guide:
  [Set up Wyrd](../../../docs/src/content/docs/get-started/setup.md).
  UX reference: `bohmian-ai/wyrd-example-ts`, README and `test/ask.test.ts`
  (inspected locally); its old addressed Prompt examples are not adopted.

This revision edits only this specification. Architecture/code/schema updates
and replacement tasks follow approval under their owning workflows.
