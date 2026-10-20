/**
 * The read contract shared by loaders, executors, and components.
 *
 * Every domain type comes from the generated `wyrd-ai` wire types; these
 * imports are type-only, so no browser entry point loads the Node SDK.
 */
import type {
  BoundedQueryResult,
  CardRef,
  CardSummary,
  ListCardsRequest,
  ListCardsResponse,
  QueryParam,
  RegisteredCard,
  TableDescription,
  VerificationExecutionState
} from 'wyrd-ai';

import problems from './problems.json';

export type { CardRef, CardSummary, QueryColumn, QueryParam, RegisteredCard } from 'wyrd-ai';

/**
 * The stable Wyrd error a refused or failed read reports. Upstream reads keep
 * their own projection; local failures carry the full canonical problem.
 */
export interface Problem {
  readonly code: string;
  readonly title: string;
  readonly status?: number;
  readonly detail?: string;
  readonly type?: string;
  readonly details?: unknown;
  readonly remediation?: string;
}

/**
 * A canonical problem, generated from the Rust error catalog by
 * `scripts/catalog.ts`, with this failure's safe context as `detail`.
 */
export function wyrdProblem(kind: keyof typeof problems, detail: string): Problem {
  return { ...structuredClone(problems[kind]), detail };
}

/** The closed set of existing domain reads a loader may plan. */
export type WireRequest =
  | { readonly op: 'cards.get_by_ref'; readonly card_ref: CardRef }
  | ({ readonly op: 'cards.list' } & ListCardsRequest)
  | {
      readonly op: 'bifrost.query';
      readonly sql: string;
      readonly params?: readonly QueryParam[];
      readonly deadline_ms?: number;
      readonly max_rows?: number;
      readonly max_bytes?: number;
    }
  | { readonly op: 'bifrost.list_tables' }
  | { readonly op: 'bifrost.describe_table'; readonly namespace: string; readonly name: string }
  | { readonly op: 'verification.get_execution'; readonly execution_id: string };

/** A complete bounded query result: positional rows, returned only after a valid terminal. */
export type QueryResult = BoundedQueryResult;

/** One authorized table as `bifrost.list_tables` lists it. */
export interface TableListing {
  readonly tables: readonly { readonly namespace: string; readonly name: string; readonly status: string }[];
}

/** The response each read produces, keyed by its operation. */
export interface WireResponses {
  'cards.get_by_ref': RegisteredCard;
  'cards.list': ListCardsResponse;
  'bifrost.query': QueryResult;
  'bifrost.list_tables': TableListing;
  'bifrost.describe_table': TableDescription;
  'verification.get_execution': VerificationExecutionState;
}

/** One read's outcome: its response, or the structured refusal or failure. */
export type WireResult<R extends WireRequest = WireRequest> =
  | { readonly ok: true; readonly value: WireResponses[R['op']] }
  | { readonly ok: false; readonly problem: Problem };

/**
 * Runs reads for one viewer; the only code that holds a credential or transport.
 *
 * Results are positional with `requests`. Aborting `signal` cancels active
 * work where the transport supports it.
 */
export interface Executor {
  run<const R extends readonly WireRequest[]>(
    requests: R,
    signal?: AbortSignal
  ): Promise<{ -readonly [K in keyof R]: WireResult<R[K]> }>;
}

/** What a data component renders, from props alone. */
export type Loaded<T> =
  | { readonly state: 'loading' }
  | { readonly state: 'populated'; readonly data: T }
  | { readonly state: 'empty'; readonly data: T; readonly reason: string }
  | { readonly state: 'denied'; readonly problem: Problem }
  | { readonly state: 'error'; readonly problem: Problem };

/** A component's typed request to its host; never a handler, tool name, or URL. */
export interface ActionIntent {
  readonly name: string;
  readonly input: unknown;
}

/** The host-reported state of one action, keyed by action name in component props. */
export type ActionState =
  | { readonly state: 'pending' }
  | { readonly state: 'success'; readonly message?: string }
  | { readonly state: 'denied'; readonly problem: Problem }
  | { readonly state: 'error'; readonly problem: Problem }
  | { readonly state: 'unavailable'; readonly reason: string };

/** Map a refused or failed read onto its component state. */
export function failed(problem: Problem): Extract<Loaded<never>, { state: 'denied' | 'error' }> {
  const status = problem.status ?? Number(/_(\d{3})_/.exec(problem.code)?.[1]);
  return status === 401 || status === 403 ? { state: 'denied', problem } : { state: 'error', problem };
}

/** Action props every interactive component takes from its host. */
export interface ActionProps {
  /** The host-reported state of each action, by action name. */
  readonly actions?: Readonly<Record<string, ActionState>>;
  /** Receives each typed intent; absent, the component's actions are unavailable. */
  readonly onaction?: (intent: ActionIntent) => void;
}
