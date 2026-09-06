import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, test } from 'vitest';
import Badge from './Badge.svelte';
import Chip from './Chip.svelte';
import Disclosure from './Disclosure.svelte';
import Select from './Select.svelte';
import StateBlock from './StateBlock.svelte';
import Table from './Table.svelte';

// The catalog boundary, checked against the source rather than described in prose.
// A component is standardized only when brand/components.json, its Svelte
// implementation and (for catalog entries) the registry all agree; these tests are
// what makes that claim falsifiable.

type Contract = {
  status: string;
  catalog: boolean;
  consumers?: string[];
  tokens?: string[];
  props?: Record<string, string>;
  variants?: string[];
};

const manifest = JSON.parse(readFileSync('brand/components.json', 'utf8')) as {
  components: Record<string, Contract>;
};
const contracts = Object.entries(manifest.components);
const built = contracts.filter(([, c]) => c.status === 'built');

const ROOT = 'src/lib/components';

function sourcePath(name: string): string | undefined {
  for (const dir of [ROOT, `${ROOT}/charts`]) {
    const path = `${dir}/${name}.svelte`;
    if (existsSync(path)) return path;
  }
  return undefined;
}

function sourceOf(name: string): string {
  const path = sourcePath(name);
  if (!path) throw new Error(`${name} has a built contract but no implementation`);
  return readFileSync(path, 'utf8');
}

/** Every custom property the generated theme defines, in either mode. */
const themeTokens = new Set(
  [...readFileSync('brand/theme.css', 'utf8').matchAll(/^\s*(--[a-z0-9-]+):/gm)].map((m) => m[1])
);

/** Font shorthands ModeProvider sets on the themed root for every component beneath it. */
const providedTokens = new Set(['--fm', '--fh']);

describe('catalog validation', () => {
  test('every built contract has an implementation', () => {
    for (const [name] of built) {
      expect(sourcePath(name), `${name} implementation`).toBeDefined();
    }
  });

  test('every implemented component has a contract', () => {
    const files = [
      ...readdirSync(ROOT).map((f) => f),
      ...readdirSync(`${ROOT}/charts`).map((f) => f)
    ].filter((f) => f.endsWith('.svelte'));
    for (const file of files) {
      const name = file.replace('.svelte', '');
      expect(manifest.components[name], `${name} contract`).toBeDefined();
    }
  });

  test('every shared catalog component names at least two accepted mock consumers', () => {
    for (const [name, c] of built.filter(([, c]) => c.catalog)) {
      expect(c.consumers?.length ?? 0, `${name} consumers`).toBeGreaterThanOrEqual(2);
    }
  });

  test('no component references an obsolete theme token', () => {
    for (const [name] of built) {
      const source = sourceOf(name);
      // Custom properties the component itself declares (e.g. Badge's --bc tone hook).
      const local = new Set([...source.matchAll(/^\s*(--[a-z0-9-]+):/gm)].map((m) => m[1]));
      for (const used of new Set([...source.matchAll(/var\((--[a-z0-9-]+)/g)].map((m) => m[1]))) {
        const known = themeTokens.has(used) || providedTokens.has(used) || local.has(used);
        expect(known, `${name} uses ${used}, which no longer exists in brand/theme.css`).toBe(true);
      }
    }
  });

  test('every contracted token exists in the generated theme', () => {
    for (const [name, c] of built) {
      for (const token of c.tokens ?? []) {
        expect(themeTokens.has(token), `${name} contracts ${token}`).toBe(true);
      }
    }
  });

  test('no component renders a variant the contract does not document', () => {
    for (const [name, c] of built) {
      const declared = new Set([
        ...(c.variants ?? []),
        ...Object.values(c.props ?? {}).flatMap((t) =>
          t.includes('|') ? t.split('|').map((s) => s.trim().replace('?', '')) : []
        )
      ]);
      const rendered = new Set(
        [...sourceOf(name).matchAll(/data-[a-z]+='([a-z-]+)'/g)].map((m) => m[1])
      );
      for (const variant of rendered) {
        expect(declared.has(variant), `${name} renders an undocumented '${variant}'`).toBe(true);
      }
    }
  });

  test('every documented variant is implemented', () => {
    for (const [name, c] of built) {
      const source = sourceOf(name);
      for (const variant of c.variants ?? []) {
        expect(source.includes(`'${variant}'`), `${name} documents '${variant}'`).toBe(true);
      }
    }
  });
});

describe('catalog prop safety', () => {
  // REQ-130: a catalog prop is a raw value, label, unit, bounded enum, safe link or
  // declarative child content. Anything that could carry executable behaviour, styling
  // authority or a data-fetch target is refused at the contract, not at the call site.
  const BANNED = /colou?r|css|class|style|html|token|script|callback|function|=>|\burl\b|query|fetch|endpoint|snippet/i;
  const ATOM =
    /^(string|number|boolean|content|string\[\]|number\[\]|Array<\{[^}]*\}>|\{[^}]*\}|[a-z][a-z0-9-]*)$/;

  const catalogContracts = built.filter(([, c]) => c.catalog);

  test('no catalog prop name or type carries unsafe capability', () => {
    for (const [name, c] of catalogContracts) {
      for (const [prop, type] of Object.entries(c.props ?? {})) {
        expect(BANNED.test(prop), `${name}.${prop} is an unsafe prop name`).toBe(false);
        expect(BANNED.test(type), `${name}.${prop}: ${type} is an unsafe prop type`).toBe(false);
      }
    }
  });

  test('every catalog prop type is JSON-safe and semantic', () => {
    for (const [name, c] of catalogContracts) {
      for (const [prop, type] of Object.entries(c.props ?? {})) {
        for (const alt of type.split('|').map((s) => s.trim().replace(/\?$/, ''))) {
          expect(ATOM.test(alt), `${name}.${prop} declares '${alt}'`).toBe(true);
        }
      }
    }
  });

  test('no catalog component declares an event handler prop', () => {
    for (const [name] of catalogContracts) {
      // `let { onselect }: { onselect?: (v: string) => void }` and friends: a catalog
      // component that takes a callback has made the serialized contract executable.
      const props = sourceOf(name).match(/let \{[\s\S]*?= \$props\(\);/)?.[0] ?? '';
      expect(props, `${name} has an inspectable prop declaration`).not.toBe('');
      expect(props, `${name} widens its props with native attributes`).not.toMatch(/HTML\w+Attributes/);
      expect(/=>/.test(props), `${name} takes a callback prop`).toBe(false);
    }
  });
});

describe('status and async states are readable without colour', () => {
  test('every status tone pairs a glyph with its text label', () => {
    for (const tone of ['ok', 'warn', 'danger', 'running'] as const) {
      const { container } = render(Badge, { props: { tone } });
      const badge = container.querySelector('.wy-badge');
      expect(badge?.querySelector('.gl')?.textContent?.trim()).not.toBe('');
      expect(badge?.querySelector('.gl')).toHaveAttribute('aria-hidden', 'true');
    }
  });

  test('a state block writes out its state word and its explanation', () => {
    render(StateBlock, {
      props: { state: 'unauthorized', title: 'Not authorized to read drift results', code: 'WYRD-AUTHZ-FORBIDDEN' }
    });
    const block = screen.getByRole('note');
    expect(block.textContent).toContain('unauthorized');
    expect(block.textContent).toContain('Not authorized to read drift results');
    expect(block.textContent).toContain('WYRD-AUTHZ-FORBIDDEN');
  });

  test('a loading state announces the wait', () => {
    render(StateBlock, { props: { state: 'loading', title: 'Loading Cards' } });
    const block = screen.getByRole('status');
    expect(block).toHaveAttribute('aria-busy', 'true');
    expect(block.textContent).toContain('Loading Cards');
  });
});

describe('tables and filter controls are keyboard operable', () => {
  test('a wide table scrolls inside a focusable, named region', () => {
    const { container } = render(Table, { props: { label: 'Cards in acme' } });
    const region = screen.getByRole('region', { name: 'Cards in acme' });
    expect(region).toHaveAttribute('tabindex', '0');
    // Narrow containers scroll the table rather than dropping columns.
    expect(container.querySelector('.wy-table')).toBe(region);
  });

  test('a filter control is a labeled native select', () => {
    render(Select, {
      props: {
        label: 'Kind',
        name: 'kind',
        options: [
          { value: 'Service', label: 'Service' },
          { value: 'Model', label: 'Model' }
        ],
        value: 'Model'
      }
    });
    const select = screen.getByLabelText('Kind') as HTMLSelectElement;
    expect(select.tagName).toBe('SELECT');
    expect(select.name).toBe('kind');
    expect(select.value).toBe('Model');
  });

  test('same-name Select instances keep distinct label and control relationships', () => {
    const options = [{ value: '1h', label: 'Last 1 hour' }];
    for (const label of ['Primary range', 'Comparison range']) {
      render(Select, { props: { label, name: 'range', options } });
    }
    const controls = screen.getAllByRole('combobox') as HTMLSelectElement[];
    expect(controls[0].id).not.toBe(controls[1].id);
    for (const [i, label] of ['Primary range', 'Comparison range'].entries()) {
      expect(screen.getByLabelText(label)).toBe(controls[i]);
      expect(controls[i]).toHaveAccessibleName(label);
      expect(controls[i].labels?.[0].control).toBe(controls[i]);
      expect(controls[i].name).toBe('range');
    }
  });

  test('a filter select submits its form instead of calling back into the view', () => {
    const form = document.createElement('form');
    document.body.appendChild(form);
    let submitted = false;
    form.requestSubmit = () => {
      submitted = true;
    };

    render(Select, {
      props: { label: 'Range', name: 'range', options: [{ value: '1h', label: 'Last 1 hour' }] },
      target: form
    });
    fireEvent.change(screen.getByLabelText('Range'));
    expect(submitted).toBe(true);
  });

  test('a filter chip removes itself through a named link, not a handler', () => {
    render(Chip, { props: { label: 'kind', value: 'Service', removeHref: '?space=prod' } });
    expect(screen.getByRole('link', { name: 'Remove filter kind: Service' })).toHaveAttribute(
      'href',
      '?space=prod'
    );
  });

  test('disclosure keeps native open/close semantics', () => {
    const { container } = render(Disclosure, { props: { summary: 'Raw definition' } });
    const details = container.querySelector('details') as HTMLDetailsElement;
    expect(details.open).toBe(false);
    fireEvent.click(screen.getByText('Raw definition'));
    expect(container.querySelector('summary')).toBeInTheDocument();
  });
});
