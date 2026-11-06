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

/** Stored bytes → "812 B" / "4.2 KiB" / "1.3 MiB". */
export function fmtBytes(bytes: number): string {
  const units = ['B', 'KiB', 'MiB', 'GiB', 'TiB'];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return unit === 0 ? `${bytes} B` : `${value.toFixed(1)} ${units[unit]}`;
}

/** A string YAML reads back as itself without quotes. */
const PLAIN_YAML = /^[A-Za-z_/][\w./@+-]*(?: [\w./@+-]+)*$/;
const YAML_WORDS = /^(?:true|false|yes|no|on|off|null|y|n)$/i;

/** One YAML scalar: plain when unambiguous, else a JSON string (valid YAML double-quoted). */
function yamlScalar(value: unknown): string {
  if (value === null || value === undefined) return 'null';
  if (typeof value !== 'string') return JSON.stringify(value);
  return PLAIN_YAML.test(value) && !YAML_WORDS.test(value) ? value : JSON.stringify(value);
}

/**
 * JSON-shaped data → block YAML, keys in their returned order. Multi-line
 * strings use a literal block; every other value round-trips as YAML 1.2.
 */
export function fmtYaml(value: unknown, indent = ''): string {
  const child = `${indent}  `;
  const nested = (item: unknown) =>
    item !== null && typeof item === 'object' && Object.keys(item).length > 0;
  const inline = (item: unknown): string => {
    if (Array.isArray(item) && item.length === 0) return '[]';
    if (item !== null && typeof item === 'object' && Object.keys(item).length === 0) return '{}';
    if (typeof item === 'string' && item.includes('\n') && !/^\s/.test(item))
      return `|${item.endsWith('\n') ? '' : '-'}\n${item.replace(/\n$/, '').split('\n').map((line) => `${child}${line}`.trimEnd()).join('\n')}`;
    return yamlScalar(item);
  };
  if (Array.isArray(value)) {
    if (value.length === 0) return `${indent}[]`;
    return value
      .map((item) => (nested(item) ? `${indent}- ${fmtYaml(item, child).slice(child.length)}` : `${indent}- ${inline(item)}`))
      .join('\n');
  }
  if (value !== null && typeof value === 'object') {
    const entries = Object.entries(value).filter(([, item]) => item !== undefined);
    if (entries.length === 0) return `${indent}{}`;
    return entries
      .map(([key, item]) => {
        const label = `${indent}${yamlScalar(key)}:`;
        return nested(item) ? `${label}\n${fmtYaml(item, child)}` : `${label} ${inline(item)}`;
      })
      .join('\n');
  }
  return `${indent}${inline(value)}`;
}
