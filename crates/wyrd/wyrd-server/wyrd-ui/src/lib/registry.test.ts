import { readFileSync } from 'node:fs';
import { expect, test } from 'vitest';
import { registry } from './registry';

type Contract = { status: string; catalog: boolean };
const manifest = JSON.parse(readFileSync('brand/components.json', 'utf8')) as {
  components: Record<string, Contract>;
};

const contracts = Object.entries(manifest.components);
const catalogManifest = contracts
  .filter(([, c]) => c.status === 'built' && c.catalog)
  .map(([name]) => name)
  .sort();

const registered = Object.keys(registry).sort();

test('every registered component has a built catalog contract in components.json', () => {
  for (const name of registered) {
    expect(manifest.components[name]).toMatchObject({ status: 'built', catalog: true });
  }
});

test('every built catalog contract is wired into the registry', () => {
  expect(registered).toEqual(catalogManifest);
});

test('trusted application chrome is deliberately absent from the catalog', () => {
  // REQ-129 / INV-021: an authored view may never place the shell, navigation, tenant
  // identity or the mode root, so none of them may be resolvable by name.
  for (const name of ['Shell', 'Sidebar', 'Topbar', 'ModeProvider', 'ProductPill']) {
    expect(manifest.components[name]?.catalog).toBe(false);
    expect(registered).not.toContain(name);
  }
});

test('every contract declares which component it is, built or specified', () => {
  for (const [name, c] of contracts) {
    expect(['built', 'spec'], `${name} status`).toContain(c.status);
    expect(typeof c.catalog, `${name} catalog`).toBe('boolean');
  }
});
