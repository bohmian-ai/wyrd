---
id: TASK-003-R1
title: Close narrow-shell and canonical-error gaps
kind: remediation
status: review
spec: SPEC-wyrd-ui-foundation
spec_revision: 6
requirements: [REQ-094, REQ-098, REQ-101, REQ-066, REQ-081, AC-010]
depends_on: [TASK-003]
parent_task: TASK-003
remediates: [FIND-TASK-003-1, FIND-TASK-003-2]
---

# Outcome and authority

Remediate the review of candidate bbc56733ae3cb075322f08876ba687ff4dae3ee0.
Both findings are validated against the current Shell and hand-authored problem
list. Preserve spec revision 6 and all authorization semantics.

Use TASK-003's locked visual authority, product/mobile.svg M-01 and product
render ledger's mobile navigation contract. Canonical error authority is
crates/wyrd-spec/src/error.rs and architecture/references/languages/errors.md.

# Scope and decisions

The narrow Menu disclosure contains five destinations, active area, tenant
identity and Close. Page actions remain in the page. Preserve desktop shell.
Generate the BFF's five problem examples from actual Rust WyrdError values via
the existing schema-generation lane; import that projection in the server BFF.
Never maintain code/title/status/remediation values independently. Retain safe
normalization that discards arbitrary upstream details.

# Ordered scenarios and checks

1. Shell.test.ts fails for missing Menu, contained tenant/current-area and Close;
   implement disclosure and keyboard close with focus return, retain five links.
2. problem.test.ts fails for canonical metadata mismatch; generate and consume
   Rust-derived problem examples, verify every supported value and unknown/error
   diagnostic scrubbing. Existing codegen:check detects stale generation.
3. Retain HTTP journey and all existing auth tests. Capture implementation and
   reference side by side at 1440×1024 and 390×844 in both themes, including the
   open Menu state; inspect all comparisons.

Exact commands (from repository root):

```sh
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/components/app/Shell.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/server/problem.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/server/auth/session.test.ts src/lib/server/routing/tenant.test.ts src/lib/server/routing/journey.test.ts
mise exec -- cargo run --locked -p wyrd-spec --example gen_schemas --features server
mise run codegen:check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
mise run check:tokens
mise run fmt:check
mise exec -- cargo clippy --locked -p wyrd-spec --example gen_schemas --all-features -- -D warnings
git diff --check
```

Use wyrd-implement and wyrd-ui. Record actual RED/GREEN and verification results.
Stop for a spec revision only if required behavior/ownership changes; no OIDC,
transport, credential, domain API or unrelated architecture changes are needed.


# Implementation and verification evidence

Readiness: READY. Both findings reproduce on the reviewed candidate, trace to
approved revision 6, and require no specification revision. Implementation is
in the worktree for cumulative TASK-003 rereview; this is not review approval.

- FIND-TASK-003-1: RED was the missing accessible Menu control. GREEN covers
  the contained five links, current area, tenant identity, Close, Escape, and
  focus return. Desktop retains the same navigation and page actions stay
  outside the disclosure.
- FIND-TASK-003-2: RED reproduced the unauthenticated remediation mismatch.
  The existing Rust schema generator now exports safe problem examples using
  actual WyrdError projection. The server imports that generated JSON. The
  focused test compares all five examples to their Rust derive metadata and
  checks that upstream diagnostics and unknown codes remain scrubbed.

Passed verification:

- Focused Shell, problem, session, tenant-routing and HTTP-journey files:
  17 tests across five files. Shell/problem rechecked after formatting.
- Full UI suite: 95 tests across 20 files.
- Svelte check: zero errors, zero warnings; production build passed.
- Canonical generator and full `mise run codegen:check` passed.
- `mise run fmt:check`, focused generator Clippy with all features and warnings
  denied, `mise run check:tokens`, and `git diff --check` passed.

One intermediate Svelte check encountered development-generated environment
ambient types while the development server/build were active. After stopping
that server and running sync/check sequentially, the check passed without any
source changes or suppression.

# Visual evidence

Live Chrome captures use 1440×1024 and 390×844, light and dark. Browser checks
confirmed closed/open visibility, five contained destinations, current area and
tenant identity, page-action exclusion, Close/Escape focus return, and no page
horizontal overflow. All six reference/implementation comparisons were visually
inspected. Reference M-01 depicts Cards while this task's implemented page is
Home; the comparison assesses the shared navigation contract, not Cards content.
Desktop uses H-02 Home. Existing page content and component styling are retained.

- [Desktop light](../../evidence/TASK-003-R1-1440-light-desktop-comparison.jpg)
- [Desktop dark](../../evidence/TASK-003-R1-1440-dark-desktop-comparison.jpg)
- [Mobile closed light](../../evidence/TASK-003-R1-390-light-closed-comparison.jpg)
- [Mobile open light](../../evidence/TASK-003-R1-390-light-open-comparison.jpg)
- [Mobile closed dark](../../evidence/TASK-003-R1-390-dark-closed-comparison.jpg)
- [Mobile open dark](../../evidence/TASK-003-R1-390-dark-open-comparison.jpg)

Original reference and implementation PNG captures are adjacent to the linked
comparisons under the same TASK-003-R1 prefix. Production OIDC and live transport
remain the parent task's explicitly deferred non-goals.
