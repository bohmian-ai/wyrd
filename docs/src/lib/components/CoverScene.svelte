<script lang="ts">
  import { onMount } from 'svelte';

  // The landing cover, synthwave: a banded sun on the horizon, wireframe
  // ridges, a perspective grid floor, and a wireframe orb above it. Procedural,
  // seeded, painted once per size on a canvas that fills its parent. Palette is
  // the dark Evidence Thread set in both themes. Decorative only.
  let cv: HTMLCanvasElement;

  const C = {
    bg: '#0b0c12',
    deep: '#05060c',
    cobalt: '#4d5ef0',
    peri: '#7a8cff',
    soft: '#1a2040',
    lime: '#c5f23b',
    amber: '#f0a13c',
    cream: '#e6e4da'
  };

  function paint(): void {
    const c2d = cv.getContext('2d');
    if (!c2d) return;
    const ctx: CanvasRenderingContext2D = c2d;
    const r = cv.getBoundingClientRect();
    const d = Math.min(window.devicePixelRatio || 1, 2);
    const W = r.width;
    const H = r.height;
    if (!W || !H) return;
    cv.width = W * d;
    cv.height = H * d;
    ctx.setTransform(d, 0, 0, d, 0, 0);

    let seed = 7;
    const rnd = (): number => (seed = (seed * 16807) % 2147483647) / 2147483647;
    const rgba = (hex: string, a: number): string => {
      const n = parseInt(hex.slice(1), 16);
      return `rgba(${n >> 16},${(n >> 8) & 255},${n & 255},${a})`;
    };
    const TAU = Math.PI * 2;
    const horizon = H * 0.62;
    // Under 900px the scene is a band below the copy, so it spreads the sun and
    // orb apart and drops the left fade that darkens the field behind the type.
    const narrow = matchMedia('(max-width: 900px)').matches;
    const vx = W * 0.62; // vanishing point

    // Sky: deep at the top, cobalt haze, a thin amber band at the horizon.
    const sky = ctx.createLinearGradient(0, 0, 0, horizon);
    sky.addColorStop(0, C.deep);
    sky.addColorStop(0.55, '#0e1130');
    sky.addColorStop(0.92, '#2a2452');
    sky.addColorStop(1, rgba(C.amber, 0.55));
    ctx.fillStyle = sky;
    ctx.fillRect(0, 0, W, horizon);

    // Stars.
    for (let i = 0; i < 220; i++) {
      const x = rnd() * W;
      const y = rnd() * horizon * 0.85;
      const s = rnd();
      ctx.fillStyle = rgba(s > 0.85 ? C.cream : C.peri, 0.3 + rnd() * 0.6);
      ctx.fillRect(x, y, s > 0.9 ? 2 : 1, s > 0.9 ? 2 : 1);
      if (s > 0.97) {
        ctx.strokeStyle = rgba(C.cream, 0.6);
        ctx.lineWidth = 1;
        ctx.beginPath();
        ctx.moveTo(x - 5, y);
        ctx.lineTo(x + 5, y);
        ctx.moveTo(x, y - 5);
        ctx.lineTo(x, y + 5);
        ctx.stroke();
      }
    }

    // The sun: amber to lime, cut by horizontal bands that thicken toward the horizon.
    const sx = W * (narrow ? 0.7 : 0.78);
    const sr = Math.min(W, H) * 0.3;
    const sy = horizon - sr * 0.08;
    ctx.save();
    ctx.beginPath();
    ctx.rect(0, 0, W, horizon);
    ctx.clip();
    ctx.beginPath();
    ctx.arc(sx, sy, sr, 0, TAU);
    ctx.clip();
    const sun = ctx.createLinearGradient(0, sy - sr, 0, sy);
    sun.addColorStop(0, C.amber);
    sun.addColorStop(0.55, C.amber);
    sun.addColorStop(1, C.lime);
    ctx.beginPath();
    ctx.arc(sx, sy, sr, 0, TAU);
    ctx.fillStyle = sun;
    ctx.fill();
    for (let i = 0; i < 9; i++) {
      const t = i / 9;
      const y = sy - sr * 0.62 + sr * 0.62 * t * t;
      ctx.fillStyle = C.bg;
      // Band height scales with the sun, so a small sun keeps its stripes.
      ctx.fillRect(sx - sr, y, sr * 2, ((2 + i * 1.6) * sr) / 210);
    }
    ctx.beginPath();
    ctx.arc(sx, sy, sr, 0, TAU);
    ctx.strokeStyle = rgba(C.cream, 0.35);
    ctx.lineWidth = 1;
    ctx.stroke();
    ctx.restore();

    // Ridges: two wireframe ranges, dark fill, peri edges, lime facet lines.
    const ridge = (y0: number, n: number, amp: number, fill: string, edge: string): void => {
      const pts: Array<[number, number]> = [];
      for (let i = 0; i <= n; i++) {
        const t = i / n;
        pts.push([-20 + (W + 40) * t, y0 - Math.abs(Math.sin(t * 9 + y0)) * amp - rnd() * amp * 0.5]);
      }
      ctx.beginPath();
      ctx.moveTo(-20, horizon);
      pts.forEach((p) => ctx.lineTo(p[0], p[1]));
      ctx.lineTo(W + 20, horizon);
      ctx.closePath();
      ctx.fillStyle = fill;
      ctx.fill();
      ctx.strokeStyle = edge;
      ctx.lineWidth = 1;
      ctx.stroke();
      pts.forEach((p, i) => {
        if (i % 2) return;
        ctx.beginPath();
        ctx.moveTo(p[0], p[1]);
        ctx.lineTo(p[0] + (i % 4 ? 14 : -10), horizon);
        ctx.strokeStyle = rgba(C.lime, 0.25);
        ctx.stroke();
      });
    };
    ridge(horizon - H * 0.01, 26, H * 0.1, '#0a0c1c', rgba(C.peri, 0.55));
    ridge(horizon, 18, H * 0.05, '#07080f', rgba(C.peri, 0.8));

    // Floor: perspective grid converging on the vanishing point.
    ctx.fillStyle = '#07080f';
    ctx.fillRect(0, horizon, W, H - horizon);
    ctx.lineWidth = 1;
    for (let i = 1; i <= 14; i++) {
      const t = i / 14;
      const y = horizon + (H - horizon) * t * t;
      ctx.strokeStyle = rgba(C.cobalt, 0.25 + t * 0.6);
      ctx.beginPath();
      ctx.moveTo(0, y);
      ctx.lineTo(W, y);
      ctx.stroke();
    }
    for (let i = -16; i <= 16; i++) {
      const xb = vx + i * W * 0.11;
      ctx.strokeStyle = rgba(C.cobalt, 0.5);
      ctx.beginPath();
      ctx.moveTo(vx + (xb - vx) * 0.02, horizon);
      ctx.lineTo(xb, H + 10);
      ctx.stroke();
    }
    // Horizon line and the sun's reflection down the grid.
    ctx.fillStyle = C.lime;
    ctx.fillRect(0, horizon - 1, W, 2);
    const refl = ctx.createLinearGradient(0, horizon, 0, H);
    refl.addColorStop(0, rgba(C.lime, 0.16));
    refl.addColorStop(1, rgba(C.lime, 0));
    ctx.fillStyle = refl;
    ctx.beginPath();
    ctx.moveTo(sx - sr * 0.4, horizon);
    ctx.lineTo(sx + sr * 0.4, horizon);
    ctx.lineTo(sx + sr * 0.9, H);
    ctx.lineTo(sx - sr * 0.9, H);
    ctx.closePath();
    ctx.fill();

    // The orb: a wireframe sphere above the grid, amber core, lime equator.
    const ox = W * (narrow ? 0.28 : 0.52);
    const oy = H * 0.38;
    const orr = Math.min(W, H) * 0.1;
    ctx.beginPath();
    ctx.arc(ox, oy, orr, 0, TAU);
    ctx.fillStyle = rgba(C.bg, 0.85);
    ctx.fill();
    ctx.strokeStyle = rgba(C.peri, 0.9);
    ctx.lineWidth = 1.2;
    ctx.stroke();
    ctx.lineWidth = 1;
    for (let i = -3; i <= 3; i++) {
      ctx.beginPath();
      ctx.ellipse(ox, oy + (i / 4) * orr, Math.sqrt(1 - (i / 4) ** 2) * orr, orr * 0.1, 0, 0, TAU);
      ctx.strokeStyle = i === 0 ? C.lime : rgba(C.peri, 0.5);
      ctx.stroke();
    }
    for (let i = 0; i < 6; i++) {
      ctx.beginPath();
      ctx.ellipse(ox, oy, orr * Math.abs(Math.cos((i / 6) * Math.PI)), orr, 0, 0, TAU);
      ctx.strokeStyle = rgba(C.peri, 0.45);
      ctx.stroke();
    }
    ctx.beginPath();
    ctx.arc(ox, oy, orr * 0.18, 0, TAU);
    ctx.fillStyle = C.amber;
    ctx.fill();
    // Its shadow ring on the grid and a tether down to it.
    ctx.strokeStyle = rgba(C.lime, 0.6);
    ctx.setLineDash([3, 5]);
    ctx.beginPath();
    ctx.moveTo(ox, oy + orr);
    ctx.lineTo(ox, horizon + (H - horizon) * 0.5);
    ctx.stroke();
    ctx.beginPath();
    ctx.ellipse(ox, horizon + (H - horizon) * 0.5, orr * 1.4, orr * 0.3, 0, 0, TAU);
    ctx.strokeStyle = rgba(C.amber, 0.7);
    ctx.stroke();
    ctx.setLineDash([]);

    // Pylons along the horizon, amber tips.
    for (let i = 0; i < 9; i++) {
      const x = W * (0.42 + i * 0.07);
      const h = H * (0.02 + rnd() * 0.04);
      ctx.fillStyle = '#07080f';
      ctx.fillRect(x - 2, horizon - h, 4, h);
      ctx.fillStyle = C.amber;
      ctx.fillRect(x - 1, horizon - h - 3, 2, 3);
    }

    // Plate labels.
    ctx.font = `11px ${getComputedStyle(cv).getPropertyValue('--font-mono')}`;
    ctx.letterSpacing = '2px';
    const callout = (x: number, y: number, tx: number, ty: number, label: string): void => {
      // A label that would run off the right edge points left instead, and
      // higher, clear of the labels to its left.
      if (tx > x && tx + 60 + ctx.measureText(label).width > W - 8) {
        tx = x - (tx - x);
        ty -= 40;
      }
      const right = tx > x;
      ctx.strokeStyle = rgba(C.cream, 0.8);
      ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.moveTo(x, y);
      ctx.lineTo(tx, ty);
      ctx.lineTo(tx + (right ? 60 : -60), ty);
      ctx.stroke();
      ctx.beginPath();
      ctx.arc(x, y, 3, 0, TAU);
      ctx.fillStyle = C.bg;
      ctx.fill();
      ctx.stroke();
      ctx.fillStyle = C.cream;
      ctx.textAlign = right ? 'left' : 'right';
      ctx.fillText(label, tx, ty - 6);
    };
    callout(sx, sy - sr, sx + sr * 0.5, sy - sr * 1.25, 'INTENT');
    callout(ox + orr, oy, ox + orr * 1.6, oy - orr * 0.8, 'BEHAVIOR');
    callout(ox + orr * 1.4, horizon + (H - horizon) * 0.5, ox + orr * 2.2, horizon + (H - horizon) * 0.72, 'EVIDENCE');

    // Left fade so the type sits on a dark field; grain on top.
    if (!narrow) {
      const fade = ctx.createLinearGradient(0, 0, W * 0.55, 0);
      fade.addColorStop(0, rgba(C.bg, 0.94));
      fade.addColorStop(0.6, rgba(C.bg, 0.6));
      fade.addColorStop(1, rgba(C.bg, 0));
      ctx.fillStyle = fade;
      ctx.fillRect(0, 0, W * 0.55, H);
    }
    const g = document.createElement('canvas');
    g.width = g.height = 160;
    const gc = g.getContext('2d');
    if (!gc) return;
    const id = gc.createImageData(160, 160);
    for (let i = 0; i < id.data.length; i += 4) {
      const v = 200 + rnd() * 55;
      id.data[i] = id.data[i + 1] = id.data[i + 2] = v;
      id.data[i + 3] = 20;
    }
    gc.putImageData(id, 0, 0);
    const grain = ctx.createPattern(g, 'repeat');
    if (grain) {
      ctx.fillStyle = grain;
      ctx.fillRect(0, 0, W, H);
    }
  }

  onMount(() => {
    const ro = new ResizeObserver(paint);
    ro.observe(cv);
    return () => ro.disconnect();
  });
</script>

<canvas bind:this={cv} aria-hidden="true"></canvas>

<style>
  canvas {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    display: block;
  }
  /* Narrow screens: a landscape band under the copy instead of a tall field. */
  @media (max-width: 900px) {
    canvas {
      top: auto;
      height: 400px;
    }
  }
</style>
