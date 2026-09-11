import type { CardWorkspaceModule } from '../../core/workspace-registry';
import VerifierSpec from './VerifierSpec.svelte';

/** Registers the earned Verifier presentation with the discovery seam. */
export const workspace: CardWorkspaceModule = { kind: 'Verifier', component: VerifierSpec };
