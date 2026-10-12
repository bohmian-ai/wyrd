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
};

type HomeStatus = { status: string; tone: 'warn' | 'danger' | 'neutral' | 'ok' | 'running' };
/** Home content. A `null` section has no live source yet and renders as unavailable, never as empty. */
export type HomeView = {
  attention: (HomeStatus & { title: string; detail: string; href: string; age: string })[] | null;
  changes: (HomeStatus & {
    id: string;
    title: string;
    owner: string;
    claims: { satisfied: number; total: number };
    age: string;
    href: string;
  })[] | null;
  cards: (HomeStatus & {
    name: string;
    kind: string;
    version: string;
    space: string;
    registered: string;
    href: string;
  })[];
  summaries: {
    label: string;
    /** `null` when the count has no live source yet. */
    value: number | null;
    /** The count covers only the first page the server returned; more exist. */
    more?: boolean;
    unit: string;
    detail: string;
    href: string;
    signal?: boolean;
  }[];
  recent: { title: string; detail: string; href: string }[] | null;
};
