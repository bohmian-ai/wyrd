import { expect, test } from 'vitest';
import { buildRegistry, workspaces, type CardWorkspaceModule } from './workspace-registry';

// TASK-006 scenario 3 — the discovery seam selects one local module per kind
// and rejects duplicate or malformed registrations deterministically.

const component = (() => {}) as unknown as CardWorkspaceModule['component'];

test('discovery selects one module per kind', () => {
  const registry = buildRegistry({
    'workspaces/workflow/index.ts': { workspace: { kind: 'Workflow', component } },
    'workspaces/verifier/index.ts': { workspace: { kind: 'Verifier', component } }
  });
  expect([...registry.keys()].sort()).toEqual(['Verifier', 'Workflow']);
});

test('duplicate kind registration fails naming both module paths', () => {
  expect(() =>
    buildRegistry({
      'workspaces/workflow/index.ts': { workspace: { kind: 'Workflow', component } },
      'workspaces/workflow2/index.ts': { workspace: { kind: 'Workflow', component } }
    })
  ).toThrowError(
    /Duplicate Card workspace for kind Workflow: workspaces\/workflow\/index\.ts and workspaces\/workflow2\/index\.ts/
  );
});

test('a module without the workspace contract fails deterministically', () => {
  expect(() => buildRegistry({ 'workspaces/broken/index.ts': {} })).toThrowError(
    /workspaces\/broken\/index\.ts must export \{ workspace: \{ kind, component \} \}/
  );
});

test('the built registry holds exactly the earned presentations', () => {
  expect([...workspaces.keys()].sort()).toEqual(['Service', 'Verifier', 'Workflow']);
  // Only the Service workspace claims the full page below the shared header.
  expect(workspaces.get('Service')?.layout).toBe('page');
  expect(workspaces.get('Workflow')?.layout).toBeUndefined();
});
