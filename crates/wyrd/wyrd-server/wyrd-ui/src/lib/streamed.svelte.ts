import { wyrdProblem, type Problem } from 'wyrd-ai-ui/loaders';

/** How many addresses a page remembers settled sections for. */
const REMEMBERED = 10;

/**
 * The latest settled value of a section a server load streams: a page's
 * supporting reads (facets, overview, discovery charts, Card names) arrive
 * after its list on client navigations and Live refreshes. A value a newer
 * load superseded never replaces the newer one, and the previous value stays
 * shown until its successor settles, so a refresh never blanks a section.
 * When the newest load fails, its failure replaces the value, so a stale
 * section is never shown as current. A value the load already awaited (the
 * first HTML) is used as it is.
 *
 * A remounted page (Back, or returning to the same address) first shows the
 * value its address last settled, from `remembered`, so the page keeps its
 * height when SvelteKit restores the scroll position before the streamed
 * load settles. The address carries the tenant, so a value never crosses
 * tenants; at most `REMEMBERED` addresses are kept.
 */
export class Streamed<T> {
  #settled = $state<{ value: T } | { problem: Problem }>();
  #latest = 0;
  readonly #remembered: Map<string, T>;

  /** `remembered` lives in the page's module script, so it outlives each mount. */
  constructor(remembered: Map<string, T> = new Map()) {
    this.#remembered = remembered;
  }

  /** The section's current value, or `undefined` until one settles or after the newest load failed. */
  get value(): T | undefined {
    return this.#settled && 'value' in this.#settled ? this.#settled.value : undefined;
  }

  /** Why the newest load failed, or `undefined`. */
  get problem(): Problem | undefined {
    return this.#settled && 'problem' in this.#settled ? this.#settled.problem : undefined;
  }

  /** Follow the section of the newest load at `address`; call it whenever the load's data changes. */
  follow(next: T | Promise<T> | undefined, address: string): void {
    const at = ++this.#latest;
    if (!(next instanceof Promise)) {
      this.#settled = next === undefined ? undefined : { value: next };
      if (next !== undefined) this.#remember(address, next);
      return;
    }
    // Only a fresh mount shows the remembered value; a mounted page keeps its own until the newest load settles.
    const remembered = at === 1 ? this.#remembered.get(address) : undefined;
    if (remembered !== undefined) this.#settled = { value: remembered };
    next.then(
      (value) => {
        if (at !== this.#latest) return;
        this.#settled = { value };
        this.#remember(address, value);
      },
      // A superseded load's failure is not this page's; the newest one's is shown.
      (error: unknown) => {
        if (at === this.#latest) this.#settled = { problem: wyrdProblem('upstream', error instanceof Error ? error.message : 'The section could not be read.') };
      }
    );
  }

  /** Keep `value` as `address`'s newest, dropping the least recently settled past `REMEMBERED`. */
  #remember(address: string, value: T): void {
    this.#remembered.delete(address);
    this.#remembered.set(address, value);
    for (const oldest of this.#remembered.keys()) {
      if (this.#remembered.size <= REMEMBERED) break;
      this.#remembered.delete(oldest);
    }
  }
}
