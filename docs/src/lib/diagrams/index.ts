import { bifrostParts, bifrostPods, bifrostWriteRead } from './bifrost';
import type { FigureSpec } from './types';
import { evidenceJoins, runLifecycle, verificationPaths } from './verification';

/** Every diagram a docs page can name, e.g. `<WyrdDiagram name="bifrost-parts" />`. */
export const DIAGRAMS: Record<string, FigureSpec> = {
  'bifrost-parts': bifrostParts,
  'bifrost-pods': bifrostPods,
  'bifrost-write-read': bifrostWriteRead,
  'verification-paths': verificationPaths,
  'run-lifecycle': runLifecycle,
  'evidence-joins': evidenceJoins
};

/** Looks up a diagram by name and type, failing the build on a typo. */
export function diagram<T extends FigureSpec['type']>(name: string, type: T): Extract<FigureSpec, { type: T }> {
  const spec = DIAGRAMS[name];
  if (!spec || spec.type !== type) throw new Error(`No ${type} diagram named "${name}"`);
  return spec as Extract<FigureSpec, { type: T }>;
}
