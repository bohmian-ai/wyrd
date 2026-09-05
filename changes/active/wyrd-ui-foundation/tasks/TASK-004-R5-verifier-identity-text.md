---
id: TASK-004-R5
title: Remove the fabricated Verifier Card destination
kind: remediation
status: review
spec: SPEC-wyrd-ui-foundation
spec_revision: 6
requirements: [REQ-060, REQ-063, AC-001]
depends_on: [TASK-004]
parent_task: TASK-004
remediates: [FIND-TASK-004-5]
---

# Remove the fabricated Verifier Card destination

Closes validated MODERATE finding `FIND-TASK-004-5` under the approved spec.

## Authority and boundaries

- [Approved spec revision 6](../spec.md) and [original TASK-004](TASK-004-change-request-workspace.md).
- User-supplied validated review of `027fa942a16626fef2c3a0788bca7c1cf05ee7e8..4f32eefe22e7d55d71bf1465ffaa6b9eec234317`;
  [preserved review handoff](../evidence/TASK-004-remediation-handoff.md).
- [AGENTS.md](../../../../AGENTS.md), [Wyrd design](../../../../architecture/wyrd-design.md),
  [doctrine](../../../../architecture/wyrd-doctrine.mdx), and
  [spec-driven development](../../../../architecture/references/languages/spec-driven-development.md).
- UI paths below are relative to `crates/wyrd/wyrd-server/wyrd-ui`.
  Temporary projections stay behind the existing server-only WyrdClient.
  Preserve tenant binding, CSRF/origin validation, authorization, immutable
  revisions and retry semantics. No durable protocol or new dependency.


## Outcome and ownership

Verifier identity and version remain visible as plain text. No check links to
`/cards/card_verifier_01`. Own `src/lib/features/changes/Verification.svelte`
and its existing HTTP journey assertion only, removing a now-unused prop/import
if this cut makes one unnecessary. Leave Evidence/results, run confirmation,
verdicts and actual source/provider navigation unchanged.

## Implementation constraints

Replace the fabricated anchor with text in its existing metadata position.
Do not add a Cards route, Verifier Card kind, placeholder destination,
disabled link, marketplace, or generalized link resolver. The approved 16-kind
Card authority remains unchanged. This is a presentation correction, not a
schema or codegen change.

## Ordered Red-Green-Refactor scenario

Extend the real Verification HTTP journey to require the expected verifier
identity/version text and assert that rendered markup contains no fabricated
Verifier Card link. Observe RED on the existing anchor, replace it with text,
and rerun to GREEN. Retain existing result/Evidence detail and confirmed-run
assertions; verify the cut in both themes without a layout redesign.

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/changes/ChangesJourney.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/changes/verification-state.test.ts
```

## Verification and handoff

Run the focused commands above, then:

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
mise run check:tokens
git diff --check
```

Use the existing formatter/toolchain and inspect tracked and untracked diffs.
No Rust/schema source is expected to change; if scope legitimately enters that
boundary, add its canonical format/lint/codegen checks under AGENTS.md. The
previous unrelated workspace Clippy failure is evidence limitation, not authority
to weaken a gate. Do not require the broad `gate` lane for these bounded UI edits.

Record each scenario's observed RED, GREEN, any refactor, exact command results,
and evidence links in this task. Do not mark the parent approved. After all
remediations integrate, review the original base through the cumulative candidate,
including R1 and all new tasks. Refresh visual evidence if another task changes
what it depicts; overlapping files do not create a behavioral dependency.

Use `wyrd-task-readiness` before implementation; execute with `wyrd-implement`
and `wyrd-ui`. Stop with `SPEC_REVISION_REQUIRED` only if fixing the finding
would change approved behavior, durable contracts, tenancy, or architecture.


## Execution evidence — 2026-09-05

Readiness: READY — no blocking readiness findings. Approved revision 6,
validated finding coverage, cohesive owners, real dependencies, ordered TDD,
exact existing test targets and browser evidence are sufficient; no material
decision is hidden. Executed with wyrd-implement and wyrd-ui.

RED: the HTTP-rendered Verification page contained /cards/card_verifier_01.
Replaced that anchor with verifier identity/version text and removed its unused
base prop from the component and caller. GREEN: the journey verifies the text
and absence of the fabricated link; run and result/Evidence checks remain green.
No route, Card kind, registry or linking abstraction was added.

Verification: exact ChangesJourney command passes 10 tests; exact review-actions
passes 5; exact verification-state passes 1. Full UI suite: 111 tests / 23 files
passed. Svelte check: zero errors / warnings. Production build, check:tokens and
whitespace checks pass. Original Rust codegen/format evidence remains applicable;
no Rust or generated source changed. The previously documented unrelated workspace
Clippy warning remains a limitation, not a remediated TASK-004 defect.
