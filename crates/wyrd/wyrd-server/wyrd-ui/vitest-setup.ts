import '@testing-library/jest-dom/vitest';

// jsdom has no ResizeObserver; Svelte's bind:clientWidth (used by the width-aware
// charts) requires one. A no-op observer keeps components mountable — chart tests
// assert structure, not measured pixel widths.
if (typeof globalThis.ResizeObserver === 'undefined') {
  globalThis.ResizeObserver = class {
    observe(): void {}
    unobserve(): void {}
    disconnect(): void {}
  };
}
