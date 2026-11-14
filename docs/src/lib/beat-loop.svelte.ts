/**
 * The clock behind the landing page's illustrative figures. A figure is a
 * series of beats with the given durations; the last beat is its finished
 * state, which the server renders, so the figure's meaning never depends on
 * JavaScript. The pace is illustrative, not measured performance.
 *
 * Once attached, the loop plays unless the reader prefers reduced motion, and
 * it stops while the figure is offscreen or the tab is hidden.
 */
export class BeatLoop {
  beat = $state(0);
  playing = $state(false);
  visible = $state(true);
  private readonly durations: readonly number[];

  constructor(durations: readonly number[]) {
    this.durations = durations;
    this.beat = durations.length - 1;
  }

  /** Whether the beat is in `spec`, a beat such as `3` or an inclusive range such as `1-4`. */
  at = (spec: string): boolean => {
    const [from, to = from] = spec.split('-').map(Number);
    return this.beat >= from && this.beat <= to;
  };

  get running(): boolean {
    return this.playing && this.visible;
  }

  /** Pause or resume; from the finished state, play again from the first beat. */
  toggle = (): void => {
    if (!this.playing && this.beat === this.durations.length - 1) this.beat = 0;
    this.playing = !this.playing;
  };

  /** Play from the first beat. */
  replay = (): void => {
    this.beat = 0;
    this.playing = true;
  };

  /** Advance one beat after the current beat's duration; returns the cancel for an `$effect`. */
  tick(): (() => void) | undefined {
    if (!this.running) return undefined;
    const timer = setTimeout(() => (this.beat = (this.beat + 1) % this.durations.length), this.durations[this.beat]);
    return () => clearTimeout(timer);
  }

  /** Start playing unless motion is reduced, and track `node`'s visibility; returns the detach for an `$effect`. */
  attach(node: Element): () => void {
    if (!matchMedia('(prefers-reduced-motion: reduce)').matches) {
      this.beat = 0;
      this.playing = true;
    }
    let onscreen = true;
    const update = () => (this.visible = onscreen && !document.hidden);
    const observer = new IntersectionObserver(([entry]) => {
      onscreen = entry.isIntersecting;
      update();
    });
    observer.observe(node);
    document.addEventListener('visibilitychange', update);
    return () => {
      observer.disconnect();
      document.removeEventListener('visibilitychange', update);
    };
  }
}
