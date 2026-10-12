import type { Pt } from './types';

/** SVG path data for a polyline. */
export function pathD(pts: Pt[]): string {
  return 'M' + pts.map((p) => p.join(' ')).join(' L');
}

/** The point a fraction `f` (0..1) of the way along a polyline, by length. */
export function pointAt(pts: Pt[], f: number): Pt {
  const lens = pts.slice(1).map((p, i) => Math.hypot(p[0] - pts[i][0], p[1] - pts[i][1]));
  let d = Math.max(0, Math.min(1, f)) * lens.reduce((a, b) => a + b, 0);
  for (let i = 0; i < lens.length; i++) {
    if (d <= lens[i] || i === lens.length - 1) {
      const t = lens[i] === 0 ? 0 : Math.min(1, d / lens[i]);
      return [pts[i][0] + (pts[i + 1][0] - pts[i][0]) * t, pts[i][1] + (pts[i + 1][1] - pts[i][1]) * t];
    }
    d -= lens[i];
  }
  return pts[pts.length - 1];
}

/** Strips authored HTML for aria labels. */
export function plain(html: string): string {
  return html.replace(/<[^>]+>/g, '');
}

/** True when the reader asked the OS for less motion; false during SSR. */
export function reducedMotion(): boolean {
  return typeof matchMedia !== 'undefined' && matchMedia('(prefers-reduced-motion: reduce)').matches;
}
