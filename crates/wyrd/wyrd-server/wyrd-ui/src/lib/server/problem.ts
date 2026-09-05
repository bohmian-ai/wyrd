import type { WyrdProblem } from '$lib/views';
import { isHttpError } from '@sveltejs/kit';
import examples from '../../../../../../wyrd-spec/schemas/ui_problem_examples.json';

/** Return a fresh safe example generated from the Rust WyrdError projection. */
export function problem(kind: keyof typeof examples): WyrdProblem {
  return structuredClone(examples[kind]);
}

/** Preserve known canonical metadata, never arbitrary upstream diagnostics. */
export function safeProblem(cause: unknown): WyrdProblem {
  const body: unknown = isHttpError(cause) ? cause.body : cause;
  if (typeof body === 'object' && body !== null && 'code' in body) {
    for (const kind of Object.keys(examples) as (keyof typeof examples)[]) {
      if (examples[kind].code === body.code) return problem(kind);
    }
  }
  return problem('internal');
}
