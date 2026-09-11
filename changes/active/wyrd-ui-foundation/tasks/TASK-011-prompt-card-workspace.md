---
id: TASK-011
title: Prompt Card workspace
kind: implementation
status: proposed
spec: SPEC-wyrd-ui-foundation
spec_revision: 6
requirements: [REQ-015, REQ-016, REQ-017, REQ-080, REQ-081, REQ-110, REQ-127, REQ-128, REQ-132, INV-001, INV-002, INV-008, INV-009, INV-014, INV-015, INV-016, INV-021, INV-022, AC-003, AC-008, AC-010, AC-011]
depends_on: [TASK-006]
parent_task:
remediates: []
---

# Outcome and value

Make the authored Prompt definition the dominant work region, with readable
messages and media, complete variable/model settings, safe raw inspection, and
direct links to its consumers.

# Owner and write set

- Own only `src/lib/features/cards/workspaces/prompt/**`, its fixture projection,
  focused tests, and registration module.
- Separate system instructions from ordered roles and render text, image, audio,
  file, and tool-call parts with accessible type labels.
- Expose variables/media variables, required/default/use-site state, provider,
  model, relevant settings, response schema, raw definition, and consuming
  Agent/Service Card references.

# Locked decisions and non-goals

- Unsupported provider parts remain discoverable through raw definition; secret
  values never render.
- No playground, prompt execution, editor, provider call, copied consumer data,
  or provider-specific page fork.

# Locked visual implementation authority

- Implement `C-05-light` and `C-05-dark` from
  [`cards.svg`](../../../../crates/wyrd/wyrd-server/wyrd-ui/brand/renders/product/cards.svg)
  using the exact dominant prompt-definition region, ordered roles and content
  treatments, variables/media variables, provider/model/settings, response
  schema, raw-definition disclosure, consumer links, and absent/excluded states
  in the [C-05 ledger entry](../../../../crates/wyrd/wyrd-server/wyrd-ui/brand/renders/product/README.md#c-05--prompt-card).
- `M-09` in
  [`mobile.svg`](../../../../crates/wyrd/wyrd-server/wyrd-ui/brand/renders/product/mobile.svg)
  is also binding for the shared Prompt inspection content at narrow width:
  roles, content types, variables, provider/model, raw definition, and direct
  Prompt link remain reachable without page-level horizontal overflow.
- Do not replace the authored-content hierarchy with metadata cards or a raw
  JSON page. Completion evidence must compare `C-05` in both themes and prove
  the same content remains usable at 390 × 844.

# Ordered test scenarios

1. System and ordered role content are unambiguous for every supported part.
2. Variables and media variables expose type, required/default, and use sites.
3. Provider/model/settings and response schema disclose safely; raw definition
   is closed by default and keyboard operable.
4. Agent and Service consumers preserve exact Card identity and direct links.
5. Long content/media metadata remains usable at narrow width in both themes.

# Red-Green-Refactor

Drive authored-content readability first. Reuse disclosure, badge, and code
presentation primitives without creating a generic document renderer.

# Exact verification

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/cards/workspaces/prompt/PromptWorkspace.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
mise run check:tokens
```

# Evidence and stop conditions

Record content-part, variable, disclosure, and consumer-link cases against C-05.
Stop before adding execution or provider behavior absent from the Card contract.

# Execution skills

Use `$wyrd-implement` and `wyrd-ui`.
