---
id: TASK-002
kind: implementation
status: superseded
spec: SPEC-developer-docsite-rebuild
spec_revision: 2
requirements: [REQ-002, REQ-006, REQ-008, REQ-009, REQ-010, REQ-011, REQ-014, REQ-016, INV-001, INV-002, INV-003, INV-004, INV-005, AC-001, AC-002, AC-003, AC-004, AC-005, AC-006]
depends_on: [TASK-001]
parent_task:
remediates: []
---

## Supersession — 2026-10-20

Remaining documentation work is owned by
[SPEC-docsite-incremental-reset revision 2](../../docsite-incremental-reset/spec.md)
and its four tasks. Preserve this task's historical implementation evidence;
this status does not assert completion or revoke the retained brand, platform,
navigation, or accessibility commitments.

## Outcome and Value

A new reader can understand Wyrd, set up a supported local environment, and
follow one complete declaration → observation → verification path to a visible
verdict and evidence. The landing then has a real first-success destination.

The work runs in two stages. **Stage A** writes the three Get started pages as
drafts against Wyrd's contracts while the underlying behavior is still being
finished. **Stage B** runs every documented command, replaces the unverified
output markers with captured output, and publishes. Readers see nothing until
Stage B passes.

## Owners, Scope, Consumers, and Prohibited Changes

Own the Get started introduction, setup, and quickstart, their example sources,
and their landing and shell links. Also own the four Understand explanations
those pages link to: Cards, versions, and relationships; How verification
works; Runs, observations, and retained evidence; and Identity and
authorization. They publish with the Get started journey. Derive every step from
`architecture/wyrd-design.md`, and current
Wyrd contracts (`wyrd-spec` schemas, SDK declarations, CLI). Do not implement
product behavior, simulate a verdict, invent output or timing, restore old
articles, or alter the approved visual system.
Use `$human-tech-docs` for every reader-facing page, including its Diátaxis,
humanizer, and voice passes.

## Approach

### Stage A — draft against the contract

Write one page at a time and get the user's review before starting the next.

1. **Introduction** (explanation): the Declare → Observe → Verify loop and the
   boundary between Wyrd and the reader's own runtime.
2. **Setup** (how-to): from a fresh checkout to a running server and an
   authenticated client, with a visible health check.
3. **Quickstart** (tutorial): one service declared with a Verifier binding,
   behavior recorded, and the verdict and its evidence read back, followed by
   failure guidance, cleanup, and the next task.

Every page carries `draft: true`, so it renders under `mise run docs:dev` and
`mise run docs:mocks` but stays out of production builds, search, and
`llms.txt`.

Quickstart code lives once under `examples/` and is embedded with
`<CodeFromFile>`, so Stage B runs exactly what the page shows. Rust, Python,
and TypeScript appear together where the same workflow exists; a language
without the capability is named, not given invented parity.

Commands and code may be written ahead of running them. Expected output,
verdicts, IDs, and timings may not. Each place that needs captured output gets
this exact marker, so Stage B can find every one:

```svelte
<Aside type="caution" title="Unverified">Output is captured on the first verified run.</Aside>
```

### Stage B — verify and publish

Once the documented path is runnable:

1. In a fresh repository-managed environment (`mise run dev:backend` where the
   server is required), run every documented command in page order and record
   the transcript, verdict, evidence, and elapsed time.
2. Replace each `Unverified` marker with the captured output and correct any
   step that differed from the run.
3. Exercise one material caller failure, such as an under-privileged token,
   and its documented recovery.
4. Set `draft: false`, connect the landing, search, sidebar, and next-step links
   to the published path, and run `mise run docs:check`.

## Acceptance Criteria

### Stage A

- The three pages exist as drafts, each with one Diátaxis job, and the user
  has reviewed each one.
- The introduction explains the verification loop and the boundary with the
  reader's runtime in Wyrd vocabulary, with no component inventories.
- Setup and quickstart give complete inputs, prerequisites with versions,
  failure guidance, and cleanup. Every Card kind, field, route, command, and
  status they name exists in the current contract or design authority.
- No page states output, a verdict, an ID, or a duration that was not
  captured; every such point carries the `Unverified` marker.
- Quickstart code is embedded from `examples/` through `<CodeFromFile>`.
- `mise run docs:check` passes and the published site is unchanged.

### Stage B

- Setup reaches a healthy server and an authenticated client with a visible
  check and no implicit credentials or unpublished artifact.
- The quickstart reaches a real verdict tied to the declared version and the
  observed behavior; code and expected output match a fresh run.
- No `Unverified` marker remains. The page reports a time only if it was
  measured, and it links to the next supported task.
- The landing → setup → quickstart → next-task route works in the built site
  and in search.

## Expected Write Set and Consumer Closure

Stage A: new draft pages under `docs/src/content/docs/get-started/` and
`docs/src/content/docs/understand/`, and their example sources under
`examples/`. Stage B: the same files plus landing or
shared navigation metadata needed to expose the published journey.

## Verification and Evidence

Stage A: `mise run docs:check`, a `docs:dev` render of each draft, and a
source check of every named contract item against `wyrd-spec`, the SDKs, the
CLI, and `architecture/wyrd-design.md`.

Stage B: the fresh-environment transcript and timing, supported and denied
result checks, `mise run docs:check`, and a built-site link and search pass.
Existing product journey tests show that behavior is available; they do not
substitute for running the documented path.

The task stays open until Stage B passes.

## Material Stop Conditions

- Stage A: if the contract or design authority cannot describe a complete
  declaration → observation → verdict loop, stop and report the gap. Do not
  fill it with invented behavior.
- Stage B: if a documented step cannot be run as written, fix the page to match
  the real behavior. If no supported path completes the loop, return
  `SPEC_REVISION_REQUIRED`; do not publish a partial quickstart or create
  server or SDK functionality under this docs task.

## Local setup prerequisite update

The local-server-bootstrap TASK-004 update rewrites the existing draft setup
page around `wyrd server dev --tenant <slug>` and its saved tenant credential
and endpoint. The Quickstart page does not exist yet in this worktree; when
its tutorial is authored, link its prerequisites to `/get-started/setup/` and
use fresh clients with the saved Wyrd configuration, without requiring API-key
or endpoint exports. Keep the verdict workflow and its example implementation
within this task rather than adding an empty Quickstart page for bootstrap.
