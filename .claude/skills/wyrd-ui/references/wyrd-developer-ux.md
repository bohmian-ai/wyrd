# Wyrd Developer UX

## Product posture

Wyrd is a developer tool. Interfaces should be dense, direct, and useful for repeated work. Prioritize clarity, speed, inspection, and recovery over decorative presentation.

Good Wyrd screens help users answer:

- What exists?
- What changed?
- What is healthy, risky, blocked, or stale?
- What can I inspect next?
- What action is safe to take now?

## Navigation and hierarchy

- Keep primary navigation stable.
- Put route context, object identity, and status near the top of the screen.
- Use tabs for sibling views of the same object.
- Use breadcrumbs only when route depth makes them useful.
- Avoid landing-page composition inside app workflows.

## Filters and drilldowns

- Make active filters visible and removable.
- Preserve filter state across refresh when it represents user intent.
- Put high-signal filters first; hide rare filters behind a menu or advanced section.
- Make drilldowns reversible with browser back behavior.

## Empty, loading, and error states

- Empty states should say what is absent and offer the next useful action.
- Loading states should preserve layout dimensions when possible.
- Error states should distinguish user action needed, missing permission, unavailable backend, and unexpected failure.
- Retry controls should be close to the failed content.

## Keyboard and copy workflows

- Support keyboard focus for all controls.
- Add copy actions for IDs, routes, commands, and structured values users need elsewhere.
- Keep focus visible in both light and dark modes.

## Anti-patterns

- Marketing copy inside operational views.
- Decorative cards that do not expose data or actions.
- Ambiguous status colors.
- Hidden controls required for common workflows.
- Text that wraps unpredictably inside fixed controls.
