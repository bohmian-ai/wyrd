import '@testing-library/jest-dom/vitest';

// jsdom does no layout; `bind:clientWidth` needs an observer, and components fall back to their default width.
globalThis.ResizeObserver ??= class {
  observe() {}
  unobserve() {}
  disconnect() {}
};
