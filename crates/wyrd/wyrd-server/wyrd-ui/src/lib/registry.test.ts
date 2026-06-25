import { readFileSync } from 'node:fs';
import { expect, test } from 'vitest';
import { registry } from './registry';

const manifest = JSON.parse(readFileSync('brand/components.json', 'utf8')) as {
  components: Record<string, { status: string }>;
};

const builtInManifest = Object.entries(manifest.components)
  .filter(([, c]) => c.status === 'built')
  .map(([name]) => name)
  .sort();

const registered = Object.keys(registry).sort();

test('every registered component has a built contract in components.json', () => {
  for (const name of registered) {
    expect(manifest.components[name]?.status).toBe('built');
  }
});

test('every built contract is wired into the registry', () => {
  expect(registered).toEqual(builtInManifest);
});
