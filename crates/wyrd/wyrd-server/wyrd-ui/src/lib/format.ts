// Shared presentational formatters.
//
// Wyrd's server owns canonical values (counts, milliseconds, USD, scores 0–1) and
// returns them raw on the wire so every client — CLI, MCP, agents, this UI — can
// render them its own way. Presentation is a UI concern: typed-domain components
// take the raw value and humanize it here, so the rendering stays consistent across
// the workbench without baking display strings into the API contract.

/** ms → "843ms" / "4.21s". Durations from the trace pipeline. */
export function fmtDuration(ms: number): string {
  return ms < 1000 ? `${ms}ms` : `${(ms / 1000).toFixed(2)}s`;
}

/** Integer counts → compact "12.4k" / "1.2M". Tokens, spans, runs. */
export function fmtCount(n: number): string {
  if (n < 1000) return String(n);
  if (n < 1_000_000) return `${(n / 1000).toFixed(1)}k`;
  return `${(n / 1_000_000).toFixed(1)}M`;
}

/** USD → "$0.18" / "$42.10". Spend/cost columns. */
export function fmtCost(usd: number): string {
  return `$${usd.toFixed(2)}`;
}
