// Hand-authored sidebar information architecture. The order here is the single
// source of truth for both the sidebar groups AND prev/next pagination (which
// derives from the flattened item order). Paths are base-relative (no `/wyrd`
// prefix) — the chrome prepends `base` from `$app/paths` at render time.

export type NavItem = { label: string; path: string };
export type NavGroup = { label: string; kind?: 'client' | 'server' | 'control' | 'rune'; items: NavItem[] };

// Top-level standalone links render as single-item groups without a header.
export const nav: NavGroup[] = [
  {
    label: 'Overview',
    kind: 'rune',
    items: [{ label: 'Overview', path: '/' }]
  },
  {
    label: 'Start here',
    kind: 'client',
    items: [
      { label: 'What is Wyrd', path: '/start-here/what-is-wyrd/' },
      { label: 'How it connects', path: '/start-here/how-it-connects/' },
      { label: 'Install & quickstart', path: '/start-here/quickstart/' }
    ]
  },
  {
    label: 'Cards',
    kind: 'rune',
    items: [
      { label: 'Cards overview', path: '/cards/' },
      { label: 'DataCard', path: '/guides/data-card-local-workflow/' },
      { label: 'ModelCard', path: '/cards/model/' },
      { label: 'PromptCard', path: '/cards/prompt/' }
    ]
  },
  {
    label: 'Server & auth',
    kind: 'server',
    items: [
      { label: 'Run the server', path: '/server/' },
      { label: 'Authentication', path: '/server/auth/' },
      { label: 'Register cards', path: '/server/register-cards/' }
    ]
  },
  {
    label: 'Agents & workflows',
    kind: 'client',
    items: [
      { label: 'Skald overview', path: '/skald/' },
      { label: 'Build an agent', path: '/guides/build-an-agent/' },
      { label: 'Build a workflow', path: '/guides/build-a-workflow/' }
    ]
  },
  {
    label: 'Evaluation',
    kind: 'server',
    items: [{ label: 'Evaluate agents', path: '/evaluation/' }]
  },
  {
    label: 'Reference',
    kind: 'control',
    items: [
      { label: 'Data card spec', path: '/cards/data/' },
      { label: 'Schema reference', path: '/api/schemas/' },
      { label: 'Error reference', path: '/api/errors/' }
    ]
  },
  {
    label: 'For agents',
    kind: 'control',
    items: [
      { label: 'For agents', path: '/agents/' },
      { label: 'Error remediation', path: '/agents/error-remediation/' }
    ]
  },
  {
    label: 'Roadmap',
    kind: 'rune',
    items: [{ label: 'Roadmap', path: '/roadmap/' }]
  }
];

// Flattened item order — the spine for prev/next pagination.
export const navOrder: NavItem[] = nav.flatMap((g) => g.items);

function normalize(path: string): string {
  // Compare without a trailing slash; `/` stays `/`.
  return path === '/' ? '/' : path.replace(/\/$/, '');
}

export function siblings(pathname: string): { prev?: NavItem; next?: NavItem } {
  // pathname arrives base-prefixed (e.g. `/wyrd/cards/`). Strip a leading base
  // segment by matching against the navOrder tails.
  const target = normalize(stripBase(pathname));
  const i = navOrder.findIndex((it) => normalize(it.path) === target);
  if (i === -1) return {};
  return { prev: navOrder[i - 1], next: navOrder[i + 1] };
}

// The router gives us `page.url.pathname` which includes the configured base.
// nav paths are base-free, so trim a leading `/wyrd` (the configured base) if
// present. Kept as a plain string op so nav.ts stays import-light/testable.
export function stripBase(pathname: string, base = '/wyrd'): string {
  if (base && pathname.startsWith(base)) {
    const rest = pathname.slice(base.length);
    return rest === '' ? '/' : rest;
  }
  return pathname;
}
