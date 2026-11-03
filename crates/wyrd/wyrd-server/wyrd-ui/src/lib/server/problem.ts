import { isHttpError } from '@sveltejs/kit';
import examples from '../../../../../../wyrd-spec/schemas/ui_problem_examples.json';

/** Safe BFF problem projection. Error authority remains in wyrd-spec. */
export type WyrdProblem = {
  type: string;
  title: string;
  status: number;
  detail: string;
  code: string;
  details: unknown;
  remediation: string;
};

/** Return a fresh safe example generated from the Rust WyrdError projection. */
export function problem(kind: keyof typeof examples): WyrdProblem {
  return structuredClone(examples[kind]);
}

/**
 * Preserve known canonical metadata, never arbitrary upstream diagnostics.
 * The one detail kept is a throttled password attempt's positive whole
 * `retry_after_seconds`, which the person needs to know when to try again.
 */
export function safeProblem(cause: unknown): WyrdProblem {
  const body: unknown = isHttpError(cause) ? cause.body : cause;
  if (typeof body === 'object' && body !== null && 'code' in body) {
    for (const kind of Object.keys(examples) as (keyof typeof examples)[]) {
      if (examples[kind].code !== body.code) continue;
      const value = problem(kind);
      const wait = kind === 'passwordAttempts' ? retryAfter(body) : undefined;
      if (wait) value.details = { retry_after_seconds: wait };
      return value;
    }
  }
  return problem('internal');
}

/** The positive whole `details.retry_after_seconds` of `body`, if any. */
export function retryAfter(body: unknown): number | undefined {
  const details = typeof body === 'object' && body !== null && 'details' in body ? body.details : null;
  const wait = typeof details === 'object' && details !== null && 'retry_after_seconds' in details ? details.retry_after_seconds : null;
  return Number.isSafeInteger(wait) && (wait as number) > 0 ? (wait as number) : undefined;
}

type ProblemKind = keyof typeof examples;
const kindByStatus: Record<number, ProblemKind> = {
  400: 'validation',
  401: 'unauthenticated',
  403: 'denied',
  404: 'notFound',
  409: 'conflict'
};

/** Map a Wyrd problem response to its safe problem: by code, then by status. */
export async function upstreamProblem(response: Response): Promise<WyrdProblem> {
  const known = safeProblem(await response.json().catch(() => null));
  if (known.code !== problem('internal').code) return known;
  return problem(kindByStatus[response.status] ?? 'upstream');
}
