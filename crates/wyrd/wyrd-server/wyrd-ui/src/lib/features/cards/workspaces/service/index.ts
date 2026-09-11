import type { CardWorkspaceModule } from '../../core/workspace-registry';
import ServiceWorkspace from './ServiceWorkspace.svelte';

/**
 * Registers the earned Service operational workspace with the discovery
 * seam. `layout: 'page'` claims the full area below the shared Card header —
 * the workspace owns its Overview/Composition/Definition views, local
 * navigation, rail and versions (TASK-007).
 */
export const workspace: CardWorkspaceModule = {
  kind: 'Service',
  component: ServiceWorkspace,
  layout: 'page'
};
