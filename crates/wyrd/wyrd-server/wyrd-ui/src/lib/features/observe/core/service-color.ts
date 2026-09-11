/**
 * Deterministic service → colour assignment for trace visuals.
 *
 * Colour is a secondary channel: every use sits beside the service's name in
 * text, so the mapping only needs to be stable within one view and
 * distinguishable, not semantic. Tokens come from the brand theme so both
 * modes stay consistent. Assignment is by first appearance, which keeps the
 * root service on the identity blue and avoids hash collisions.
 */
const palette = [
  'var(--brand-strong)',
  'var(--server-bar)',
  'var(--lime)',
  'var(--control-bar)'
];

/** Maps each distinct service, in order of first appearance, to a palette colour. */
export function serviceColorMap(services: string[]): Map<string, string> {
  const map = new Map<string, string>();
  for (const service of services)
    if (!map.has(service)) map.set(service, palette[map.size % palette.length]);
  return map;
}
