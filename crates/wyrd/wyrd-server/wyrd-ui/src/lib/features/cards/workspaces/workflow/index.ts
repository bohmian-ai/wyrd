import type { CardWorkspaceModule } from '../../core/workspace-registry';
import WorkflowSpec from './WorkflowSpec.svelte';

/** Registers the earned Workflow presentation with the discovery seam. */
export const workspace: CardWorkspaceModule = { kind: 'Workflow', component: WorkflowSpec };
