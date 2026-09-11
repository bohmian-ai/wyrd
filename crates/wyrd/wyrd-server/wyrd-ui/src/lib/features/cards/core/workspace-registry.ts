import type { Component } from 'svelte';
import type { CardDetail, CardKind } from './types';

/**
 * The workspace discovery seam. A focused Card workspace registers itself by
 * adding `src/lib/features/cards/workspaces/<name>/index.ts` exporting a
 * `workspace` contract — no route, host or central switch edit. Modules
 * provide presentation, local URL-state parsing and typed fixture projection
 * only; Card identity, routing, auth and IO stay with the host.
 */
export type CardWorkspaceModule = {
  /** The single Card kind this workspace presents. */
  kind: CardKind;
  /** The presentation rendered in the shared shell's work region. */
  component: Component<{ detail: CardDetail; base: string }>;
  /**
   * `page` claims the full area below the shared Card header — the workspace
   * then owns its own rail, versions and local navigation (the Service
   * operational workspace). Absent means the default panel placement inside
   * the shared columns.
   */
  layout?: 'page';
};

/**
 * Build the kind → workspace map from discovered modules.
 *
 * Fails deterministically at module evaluation — build, dev and test alike —
 * when a module omits the contract or two modules claim one kind, naming the
 * offending paths so parallel workspace tasks collide loudly, not silently.
 *
 * # Errors
 * Throws when a module lacks a `workspace` export or duplicates a kind.
 */
export function buildRegistry(
  modules: Record<string, { workspace?: CardWorkspaceModule }>
): ReadonlyMap<CardKind, CardWorkspaceModule> {
  const registry = new Map<CardKind, CardWorkspaceModule>();
  const claims = new Map<CardKind, string>();
  for (const [path, module] of Object.entries(modules)) {
    const workspace = module.workspace;
    if (!workspace?.kind || !workspace.component)
      throw new Error(`Card workspace module ${path} must export { workspace: { kind, component } }`);
    const existing = claims.get(workspace.kind);
    if (existing)
      throw new Error(
        `Duplicate Card workspace for kind ${workspace.kind}: ${existing} and ${path}`
      );
    claims.set(workspace.kind, path);
    registry.set(workspace.kind, workspace);
  }
  return registry;
}

/** The build-time discovered registry; unregistered kinds use the Spec fallback. */
export const workspaces = buildRegistry(
  import.meta.glob<{ workspace: CardWorkspaceModule }>('../workspaces/*/index.ts', { eager: true })
);
