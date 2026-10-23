import '@testing-library/jest-dom/vitest';

// jsdom does no layout; `bind:clientWidth` needs an observer, and components fall back to their default width.
// `ResizeObserver.notify()` reports every observed element, as a browser does after a layout change.
globalThis.ResizeObserver ??= class {
  static live = new Set<{ callback: ResizeObserverCallback; targets: Set<Element> }>();
  static notify() {
    for (const each of this.live) each.callback([...each.targets].map((target) => ({ target }) as ResizeObserverEntry), each as never);
  }
  targets = new Set<Element>();
  constructor(readonly callback: ResizeObserverCallback) {
    (this.constructor as { live: Set<unknown> }).live.add(this);
  }
  observe(target: Element) {
    this.targets.add(target);
  }
  unobserve(target: Element) {
    this.targets.delete(target);
  }
  disconnect() {
    this.targets.clear();
  }
};
