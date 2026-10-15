import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    // Each integration file boots an in-process Wyrd server with its own
    // Postgres pools, so concurrency is bounded by Postgres connections, not
    // CPU. Matches the journey cap in .config/nextest.toml; raise them together.
    maxWorkers: 6,
  },
});
