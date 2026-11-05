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

/** Numbers in a chart or stat: grouped, at most three fraction digits. */
const NUMBER = new Intl.NumberFormat('en-US', { maximumFractionDigits: 3 });

/** A plotted or tabulated number → "12,400.5". */
export function fmtNumber(value: number): string {
  return NUMBER.format(value);
}

/**
 * Parse an Arrow JSON temporal value to epoch milliseconds. A timestamp without
 * an offset is a zone-less Arrow timestamp and reads as UTC, never local time.
 */
export function parseInstant(value: unknown): number {
  if (typeof value === 'number') return value;
  const text = String(value);
  const zoned = /([zZ]|[+-]\d\d:?\d\d)$/.test(text) || !text.includes('T');
  return Date.parse(zoned ? text : `${text}Z`);
}

/** An instant for a caption or tooltip → "2026-10-20 14:05 UTC". */
export function fmtInstant(ms: number): string {
  return `${new Date(ms).toISOString().slice(0, 16).replace('T', ' ')} UTC`;
}

/** An exact Card reference in canonical spelling → "Model/ranker@1.4.2", plus "#uid" when one is supplied. */
export function fmtCardRef(ref: { kind: string; name: string; version: string; uid?: string | null }): string {
  return `${ref.kind}/${ref.name}@${ref.version}${ref.uid ? `#${ref.uid}` : ''}`;
}
