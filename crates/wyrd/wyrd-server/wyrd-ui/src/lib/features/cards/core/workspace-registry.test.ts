import { expect, test } from 'vitest';
import { buildRegistry, workspaces, type CardWorkspaceModule } from './workspace-registry';

// TASK-006 scenario 3 — the discovery seam selects one local module per kind
// and rejects duplicate or malformed registrations deterministically.

const component = (() => {}) as unknown as CardWorkspaceModule['component'];

test('discovery selects one module per kind', () => {
  const registry = buildRegistry({
    'workspaces/service/index.ts': { workspace: { kind: 'Service', component } },
    'workspaces/verifier/index.ts': { workspace: { kind: 'Verifier', component } }
  });
  expect([...registry.keys()].sort()).toEqual(['Service', 'Verifier']);
});

test('duplicate kind registration fails naming both module paths', () => {
  expect(() =>
    buildRegistry({
      'workspaces/verifier/index.ts': { workspace: { kind: 'Verifier', component } },
      'workspaces/verifier2/index.ts': { workspace: { kind: 'Verifier', component } }
    })
  ).toThrowError(
    /Duplicate Card workspace for kind Verifier: workspaces\/verifier\/index\.ts and workspaces\/verifier2\/index\.ts/
  );
});

test('a module without the workspace contract fails deterministically', () => {
  expect(() => buildRegistry({ 'workspaces/broken/index.ts': {} })).toThrowError(
    /workspaces\/broken\/index\.ts must export \{ workspace: \{ kind, component \} \}/
  );
});

test('the built registry holds exactly the earned presentations', () => {
  expect([...workspaces.keys()].sort()).toEqual(['Service', 'Verifier']);
  // Only the Service workspace claims the full page below the shared header.
  expect(workspaces.get('Service')?.layout).toBe('page');
  expect(workspaces.get('Verifier')?.layout).toBeUndefined();
});
