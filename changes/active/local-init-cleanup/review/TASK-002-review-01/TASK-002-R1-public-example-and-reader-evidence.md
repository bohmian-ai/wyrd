---
id: TASK-002-R1
kind: remediation
status: proposed
spec: SPEC-local-init-cleanup
spec_revision: 2
parent_task: TASK-002
remediates: [FIND-TASK-002-1, FIND-TASK-002-2]
depends_on: []
---

# Prove the published policy recipe and preserve reader-exercise evidence

## Authority and immutable context

This is a concrete proposed correction packet, not implementation permission.
[Spec revision 2](../../spec.md) is still draft; formal approval must be
established before this task is handed to `$wyrd-implement`.

- Original task: contract snapshot `e13bbb5fb` of
  [TASK-002](../../tasks/TASK-002-document-verification-scope.md).
- Reviewed base: `5aaa38937d76fef1a690cd931e3001363be69ddc`.
- Reviewed candidate: `729255588540db8a46dba9f5d090bbfc9e59d2bb`.
- [Verdict](verdict.md), [behavior review](task-review-behavior.md), and
  [independent final ledger](findings-validation.md).
- Required outcomes remain REQ-006/007 and AC-005; root/docs AGENTS and
  docs publication rules govern source ownership and proof.

## Diagnosis and intended correction

### FIND-TASK-002-1 — the published code is not the executed example source

`verifier-recipes.md:432–452` adds handwritten Service-binding and Python
fences. The same material reaches machine readers through `llms-full.txt`.
The referenced handbook journey proves selected-file upload, tools, mounted
evidence and citations using the `handbook-desk` fixture and its own client
setup. It does not consume the new public `support-reply.yaml` fragment or
execute the new Python program. The task explicitly records no completed
public workflow run at the documented release.

This violates the docs rules requiring copyable source embedded from a
journey-executed example and a proved public workflow. It is not a finding
that the API is invalid or that model judgment quality is poor. Merely citing
the underlying capability journey cannot detect drift in separately copied
code; moving the snippet into an unexecuted file also leaves the gate open.

**Selected correction:** complete the existing Uploaded files owner once.
Reuse the existing handbook Card/Service bundle, make its actual file
placement and identity clear, and put the minimal public invocation/citation
inspection into one example source that a journey executes. Embed that source
with CodeFromFile. Link existing gateway, tool-grant and declaration owners;
show only the local missing steps needed to run this example. Do not add
another policy tutorial or duplicate the architecture/selection table.

Reuse the public-script execution pattern already present in
`test_docsite_quickstart.py`: prepare the reader directory, choose the normal
client connection, execute the actual embedded program against the real server,
and assert its result. The existing handbook responder and uploaded-Artifact
fixtures already perform the real file-tool round trip; reuse them rather
than creating another test server, provider service or harness.

Run the exact published upload/declaration/invocation sequence with existing
server/Postgres/scripted-provider setup and observe a verdict plus a valid
citation to known selected-file content. Record the command, version/source
identity and observed result. The documented release is 0.4.1; do not silently
label a candidate-source execution as a released-binary run. If an explicit
release-path observation is still unavailable, retain that proof as open and
do not claim the public publication gate passed.

This is bounded example/source/proof work. No paid provider call or real-model
semantic-quality benchmark is necessary: a disclosed existing scripted
provider is sufficient for composition and citation proof. Root-cause closure
is one maintained executable source consumed by both docs and the focused
journey, not repeated guards in every documentation consumer.

### FIND-TASK-002-2 — summaries do not preserve the required reader answers

TASK-002's Reader exercises section records model/revision, question strings,
general restrictions and eight answer summaries. Some entries say “Same” or
“Same composition.” No complete prompts or actual returned answers are linked
from the packet. This does not prove the exercises failed or never happened;
it prevents independent assessment of the explicitly required comprehension
evidence and possible prompt leakage.

**Selected correction:** recover and preserve the original complete supplied
messages and actual eight answers, with each run's model/version, documentation
revision, entry material and retrieved sources. Store them in the packet's
implementation evidence or a clearly linked artifact; summaries may remain
as navigation. Do not reconstruct alleged original answers from summaries.
Rerun only records whose original output cannot be recovered, then assess
actual answers against the existing four-by-two rubric. Repair and rerun only
failed rows if a recorded answer reveals a real documentation gap.

A faithful disclosed source projection of MCP onboarding remains acceptable
for this bounded reader exercise; do not invent a live-MCP requirement. The
existing guide journey separately proves discovery. No extra reader scenarios,
new model evaluation framework, CI gate or general content rewrite is needed.

## Constraints and preserved acceptance

Preserve the six badges, thin hub, simple six-platform comparison, canonical
content owners, third onboarding step, generated projections and existing
publication/draft filtering. Keep benchmarks and duplicated architecture out.
Do not change runtime behavior, schemas, permissions, provider integration or
workflow/enforcement ownership. Preserve existing selected-file isolation,
authorized tools and citation assertions; a scripted model is not evidence of
real-model judgment quality.

The two corrections have separate local owners: shared executable recipe
source/public proof, and reader evidence. They do not justify reopening the
accepted comparison or navigation design. No source-derived finding requires
a new product or architecture decision.

## Acceptance and focused proof

| Finding | Closure observation |
| --- | --- |
| FIND-TASK-002-1 | New copyable binding/program content comes from a shared tested source; a focused real-server public-client journey executes the exact embedded source and published sequence, verifies selected policy content and citation, and records accurate source/release identity. Existing capability proof remains intact. |
| FIND-TASK-002-2 | Eight records preserve complete supplied prompts, actual answers and per-run metadata. A reviewer can assess each existing rubric directly. Missing records are rerun rather than fabricated; only failed rows require content repair. |

For the example change, use the owning Python docs-example/handbook journey
infrastructure and confirm the exact focused selector after adding or extending
the test, before RED. Demonstrate the absent exact-source/execution proof,
connect the shared example, then run that focused journey to GREEN. Record
the exact repository-managed Postgres/setup command and selector with output.
No broad cross-SDK suite is required solely for this Python documentation path.

Reader-record recovery and static documentation provenance require direct
evidence inspection, not prose-pinning tests. For altered discovery/projections,
reuse the existing focused docs check:

```sh
mise exec -- pnpm --dir docs exec vitest run src/lib/derived-nav.test.ts src/lib/content.test.ts src/lib/indexes.test.ts src/routes/shell-links.test.ts src/lib/readme.test.ts
```

Regenerate through `mise run docs:generate`, then run the required
`mise run docs:check` and applicable Python format/lint checks if executable
Python changes. Preserve the gate's generated-output cleanliness requirement
and caller-owned Git authority; no bypass or unauthorized commit. Do not
rerun provider benchmarks, unrelated user journeys or the broad repository
gate. Retain this review for a focused remediation review after implementation.
