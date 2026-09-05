import type { WyrdProblem } from '$lib/views';
import { isHttpError } from '@sveltejs/kit';

// Mock examples of the existing wyrd-spec/src/error.rs catalog, never upstream text.
const examples = {
  unauthenticated: [
    'WYRD_AUTH_401_UNAUTHENTICATED',
    401,
    'Not authenticated',
    'Sign in to continue.'
  ],
  expired: [
    'WYRD_AUTH_401_TOKEN_EXPIRED',
    401,
    'Token expired',
    'Re-authenticate to obtain a fresh token.'
  ],
  denied: [
    'WYRD_PERMISSION_403_DENIED_RBAC',
    403,
    'Permission denied (RBAC)',
    'Request the required role from a workspace admin.'
  ],
  internal: [
    'WYRD_SPEC_500_INTERNAL',
    500,
    'Internal error',
    'Retry later or inspect server logs using the request ID.'
  ],
  upstream: [
    'WYRD_SPEC_502_UPSTREAM_FAILURE',
    502,
    'Upstream dependency failed',
    'Check the upstream dependency health and retry policy.'
  ]
} as const;

export function problem(kind: keyof typeof examples): WyrdProblem {
  const [code, status, title, remediation] = examples[kind];
  return {
    type: `https://wyrd.dev/problems/${code}`,
    title,
    status,
    code,
    detail: title,
    details: {},
    remediation
  };
}

/** Project only known, safe catalog metadata, never arbitrary upstream diagnostic fields. */
export function safeProblem(cause: unknown): WyrdProblem {
  const body: unknown = isHttpError(cause) ? cause.body : cause;
  if (typeof body === 'object' && body !== null && 'code' in body) {
    for (const kind of Object.keys(examples) as (keyof typeof examples)[]) {
      if (examples[kind][0] === body.code) return problem(kind);
    }
  }
  return problem('internal');
}
