/**
 * Internal native plumbing shared by the public entry points: the catalog
 * error class, the projection of native results into values or errors, the
 * strict JSON snapshot, and the native handle behind each authoring value.
 * Nothing here is a package export; `index.ts` re-exports the public errors.
 */

import type { NativeAuthored, NativeLifecycleResult } from "../index.cjs";
import type { WyrdErrorCode } from "./error-codes.js";

/**
 * Every failure the SDK raises: one stable catalog code plus its HTTP status,
 * title, detail, remediation, and structured details, read as properties
 * rather than parsed from the message.
 */
export class WyrdError extends Error {
  readonly code: WyrdErrorCode;
  readonly status: number;
  readonly title: string;
  readonly detail: string;
  readonly remediation: string | undefined;
  /** JSON-safe structured diagnostics supplied by the originating Wyrd error. */
  readonly details: unknown;

  /**
   * Build one catalog error; the SDK raises these, and a test double may too.
   *
   * @param code - Stable catalog code, such as `WYRD_SPEC_400_VALIDATION`.
   * @param status - HTTP-equivalent status.
   * @param title - Stable catalog title.
   * @param detail - Scrubbed human-readable detail, also the `message`.
   * @param remediation - Operator-facing remediation, when the catalog has one.
   * @param details - JSON-safe structured diagnostics, when present.
   */
  constructor(
    code: WyrdErrorCode,
    status: number,
    title: string,
    detail: string,
    remediation?: string,
    details?: unknown,
  ) {
    super(detail);
    this.name = "WyrdError";
    this.code = code;
    this.status = status;
    this.title = title;
    this.detail = detail;
    this.remediation = remediation;
    this.details = details;
  }
}

/** A query stream that ended before its validated terminal frame. */
export class IncompleteQueryStreamError extends WyrdError {
  /**
   * Build the `WYRD_VALA_502_QUERY_STREAM_INCOMPLETE` error.
   *
   * @param status - HTTP-equivalent status.
   * @param title - Stable catalog title.
   * @param detail - Scrubbed human-readable detail.
   * @param remediation - Operator-facing remediation, when present.
   * @param details - JSON-safe structured diagnostics, when present.
   */
  constructor(status: number, title: string, detail: string, remediation?: string, details?: unknown) {
    super("WYRD_VALA_502_QUERY_STREAM_INCOMPLETE", status, title, detail, remediation, details);
    this.name = "IncompleteQueryStreamError";
  }
}

export interface NativeErrorMetadata {
  errorCode?: string | null;
  errorStatus?: number | null;
  errorTitle?: string | null;
  errorDetail?: string | null;
  errorRemediation?: string | null;
  errorDetailsJson?: string | null;
}

export function projectedError(metadata: NativeErrorMetadata): WyrdError | undefined {
  if (metadata.errorCode === null || metadata.errorCode === undefined) {
    return undefined;
  }
  const detail = metadata.errorDetail ?? "Bifrost query failed";
  const status = metadata.errorStatus ?? 500;
  const title = metadata.errorTitle ?? "Bifrost query failed";
  const remediation = metadata.errorRemediation ?? undefined;
  const details = metadata.errorDetailsJson == null ? undefined : (JSON.parse(metadata.errorDetailsJson) as unknown);
  if (metadata.errorCode === "WYRD_VALA_502_QUERY_STREAM_INCOMPLETE") {
    return new IncompleteQueryStreamError(status, title, detail, remediation, details);
  }
  return new WyrdError(
    // The native projection only emits codes from the same derive-backed catalog.
    metadata.errorCode as WyrdErrorCode,
    status,
    title,
    detail,
    remediation,
    details,
  );
}

/** The value of one native lifecycle result, throwing its catalog error. */
export function lifecycleValue<T>(result: NativeLifecycleResult): T {
  const error = projectedError(result);
  if (error !== undefined) {
    throw error;
  }
  if (result.valueJson === null || result.valueJson === undefined) {
    throw new WyrdError(
      "WYRD_VALA_502_QUERY_STREAM_PROTOCOL",
      502,
      "Query lifecycle protocol failed",
      "native lifecycle control returned neither a value nor structured error",
    );
  }
  return JSON.parse(result.valueJson) as T;
}

/** The `WYRD_SPEC_400_VALIDATION` refusal of `field` at the JSON `path`. */
function invalidJson(field: string, path: string, reason: string): WyrdError {
  return new WyrdError(
    "WYRD_SPEC_400_VALIDATION",
    400,
    "Validation failed",
    `${field} is invalid: ${path} ${reason}`,
    "Check the submitted Wyrd request fields against the published schema and retry.",
    { field, path, reason },
  );
}

/** Whether `key` is a canonical element index below `length`, which JSON keeps. */
function isArrayIndex(key: string, length: number): boolean {
  return String(Number(key)) === key && Number.isInteger(Number(key)) && Number(key) >= 0 && Number(key) < length;
}

/** One position inside a JSON snapshot: object keys and list indices from the root. */
export type SnapshotPath = readonly (string | number)[];

/** A plain JSON snapshot, with `null` at each held value's position. */
export type JsonSnapshot<H> = {
  readonly value: unknown;
  readonly holes: readonly { readonly path: SnapshotPath; readonly hole: H }[];
};

/**
 * Snapshot `value` as plain JSON, refusing what `JSON.stringify` would
 * silently drop or coerce, and holding every object `hold` recognizes in place.
 *
 * Only plain objects, arrays, strings, booleans, `null`, and finite safe
 * numbers pass. Traversal reads every property and element exactly once, so a
 * getter cannot return one value to validation and another to serialization.
 * An object `hold` maps to a value is not traversed: it stands as `null` and
 * is returned with its path.
 *
 * @param field - The argument named in a refusal.
 * @param value - The value to snapshot.
 * @param hold - Recognizes an object to hold in place rather than traverse.
 * @returns The snapshot and every held value with its path.
 * @throws a `WYRD_SPEC_400_VALIDATION` {@link WyrdError} naming the offending
 * path.
 */
export function jsonSnapshot<H = never>(
  field: string,
  value: unknown,
  hold: (node: object) => H | undefined = () => undefined,
): JsonSnapshot<H> {
  const ancestors = new Set<object>();
  const holes: { readonly path: SnapshotPath; readonly hole: H }[] = [];
  const check = (node: unknown, path: string, segments: SnapshotPath): unknown => {
    if (node === null || typeof node === "string" || typeof node === "boolean") {
      return node;
    }
    if (typeof node === "number") {
      if (!Number.isFinite(node)) {
        throw invalidJson(field, path, "is not a finite number");
      }
      if (Number.isInteger(node) && !Number.isSafeInteger(node)) {
        throw invalidJson(field, path, "is an integer beyond the safe range");
      }
      return node;
    }
    if (typeof node !== "object") {
      throw invalidJson(field, path, `is an unsupported ${typeof node} value`);
    }
    const held = hold(node);
    if (held !== undefined) {
      holes.push({ path: segments, hole: held });
      return null;
    }
    if (ancestors.has(node)) {
      throw invalidJson(field, path, "is a cycle");
    }
    ancestors.add(node);
    let snapshot: unknown;
    if (Array.isArray(node)) {
      for (const key of Reflect.ownKeys(node)) {
        if (key !== "length" && !(typeof key === "string" && isArrayIndex(key, node.length))) {
          throw invalidJson(field, path, "has a non-element array property");
        }
      }
      const items: unknown[] = [];
      for (let index = 0; index < node.length; index += 1) {
        items.push(check(node[index], `${path}[${index}]`, [...segments, index]));
      }
      snapshot = items;
    } else {
      const prototype = Object.getPrototypeOf(node) as unknown;
      if (prototype !== Object.prototype && prototype !== null) {
        throw invalidJson(field, path, "is not a plain object");
      }
      const keys = Reflect.ownKeys(node);
      for (const key of keys) {
        if (typeof key === "symbol") {
          throw invalidJson(field, path, "has a symbol key");
        }
        if (!Object.prototype.propertyIsEnumerable.call(node, key)) {
          throw invalidJson(field, path, `has a non-enumerable property ${key}`);
        }
      }
      snapshot = Object.fromEntries(
        (keys as string[]).map((key) => [
          key,
          check((node as Record<string, unknown>)[key], `${path}.${key}`, [...segments, key]),
        ]),
      );
    }
    ancestors.delete(node);
    return snapshot;
  };
  return { value: check(value, "$", []), holes };
}

/** The native handle behind each `wyrd-ai/authoring` value. */
const authored = new WeakMap<object, NativeAuthored>();

/** Record `native` as the handle behind the authoring value `owner`. */
export function holdAuthored(owner: object, native: NativeAuthored): void {
  authored.set(owner, native);
}

/** The native handle behind `value`, when it is an authoring value. */
export function authoredNative(value: object): NativeAuthored | undefined {
  return authored.get(value);
}

/**
 * The native handle behind the authoring value `owner`.
 *
 * @param owner - A `wyrd-ai/authoring` value.
 * @returns Its handle.
 * @throws `TypeError` when `owner` is not an authoring value.
 */
export function authoredOf(owner: object): NativeAuthored {
  const native = authored.get(owner);
  if (native === undefined) {
    throw new TypeError("not a wyrd-ai/authoring value");
  }
  return native;
}
