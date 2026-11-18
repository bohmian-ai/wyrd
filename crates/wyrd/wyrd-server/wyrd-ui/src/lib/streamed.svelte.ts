import { wyrdProblem, type Problem } from 'wyrd-ai-ui/loaders';

/**
 * The latest settled value of a section a server load streams: a page's
 * supporting reads (facets, overview, discovery charts, Card names) arrive
 * after its list on client navigations and Live refreshes. A value a newer
 * load superseded never replaces the newer one, and the previous value stays
 * shown until its successor settles, so a refresh never blanks a section.
 * When the newest load fails, its failure replaces the value, so a stale
 * section is never shown as current. A value the load already awaited (the
 * first HTML) is used as it is.
 */
export class Streamed<T> {
  #settled = $state<{ value: T } | { problem: Problem }>();
  #latest = 0;

  /** The section's current value, or `undefined` until one settles or after the newest load failed. */
  get value(): T | undefined {
    return this.#settled && 'value' in this.#settled ? this.#settled.value : undefined;
  }

  /** Why the newest load failed, or `undefined`. */
  get problem(): Problem | undefined {
    return this.#settled && 'problem' in this.#settled ? this.#settled.problem : undefined;
  }

  /** Follow the section of the newest load; call it whenever the load's data changes. */
  follow(next: T | Promise<T> | undefined): void {
    const at = ++this.#latest;
    if (!(next instanceof Promise)) {
      this.#settled = next === undefined ? undefined : { value: next };
      return;
    }
    next.then(
      (value) => {
        if (at === this.#latest) this.#settled = { value };
      },
      // A superseded load's failure is not this page's; the newest one's is shown.
      (error: unknown) => {
        if (at === this.#latest) this.#settled = { problem: wyrdProblem('upstream', error instanceof Error ? error.message : 'The section could not be read.') };
      }
    );
  }
}
