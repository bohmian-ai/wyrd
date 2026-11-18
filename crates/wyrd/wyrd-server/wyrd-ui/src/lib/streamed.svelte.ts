/**
 * The latest settled value of a section a server load streams: a page's
 * supporting reads (facets, overview, discovery charts, Card names) arrive
 * after its list on client navigations and Live refreshes. A value a newer
 * load superseded never replaces the newer one, and the previous value stays
 * shown until its successor settles, so a refresh never blanks a section.
 * A value the load already awaited (the first HTML) is used as it is.
 */
export class Streamed<T> {
  #settled = $state<T>();
  #latest = 0;

  /** The section's current value, or `undefined` until the first one settles. */
  get value(): T | undefined {
    return this.#settled;
  }

  /** Follow the section of the newest load; call it whenever the load's data changes. */
  follow(next: T | Promise<T> | undefined): void {
    const at = ++this.#latest;
    if (!(next instanceof Promise)) {
      this.#settled = next;
      return;
    }
    next.then(
      (value) => {
        if (at === this.#latest) this.#settled = value;
      },
      // A superseded or abandoned load's failure is not this page's; the next load reports its own.
      () => {}
    );
  }
}
