/** Safe BFF projections. Domain protocol and error authority remain in wyrd-spec. */
export type WyrdProblem = {
  type: string;
  title: string;
  status: number;
  detail: string;
  code: string;
  details: unknown;
  remediation: string;
};

export type Tenant = { key: string; name: string };
export type SessionMetadata = {
  subject: { id: string; name: string };
  tenants: Tenant[];
  expiresAt: number;
  csrf: string;
};

type HomeStatus = { status: string; tone: 'warn' | 'danger' | 'neutral' | 'ok' | 'running' };
export type HomeView = {
  attention: (HomeStatus & { title: string; detail: string; href: string; age: string })[];
  changes: (HomeStatus & {
    id: string;
    title: string;
    owner: string;
    claims: { satisfied: number; total: number };
    age: string;
    href: string;
  })[];
  cards: (HomeStatus & {
    name: string;
    kind: string;
    version: string;
    space: string;
    seen: string;
    href: string;
  })[];
  summaries: {
    label: string;
    value: number;
    unit: string;
    detail: string;
    href: string;
    signal?: boolean;
  }[];
  recent: { title: string; detail: string; href: string }[];
};
