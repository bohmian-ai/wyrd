<script lang="ts">
  // Retro perspective grid + pixel stars (homage to brand/landing-vision.html).
  // Deterministic — no Math.random — so prerender is stable.
  function planeGrid(): string {
    const W = 600,
      H = 200,
      cx = 300;
    let s = '';
    for (let i = -5; i <= 17; i++) {
      const x = i * 50;
      s += `<line x1="${cx}" y1="0" x2="${x}" y2="${H}" stroke="var(--grid)" stroke-width="1.1" opacity="0.5"/>`;
    }
    [2.1, 7.8, 16.9, 29.2, 44.7, 63.2, 84.7, 109.2, 136.6, 166.9, 200].forEach((y) => {
      s += `<line x1="0" y1="${y}" x2="${W}" y2="${y}" stroke="var(--grid)" stroke-width="1.1" opacity="0.5"/>`;
    });
    return s;
  }
  function planeStars(): string {
    let s = '',
      seed = 131;
    const rnd = () => (seed = (seed * 16807) % 2147483647) / 2147483647;
    for (let i = 0; i < 58; i++) {
      const x = (rnd() * 600) | 0,
        y = (rnd() * 180) | 0,
        sz = rnd() < 0.16 ? 2 : 1;
      s += `<rect x="${x}" y="${y}" width="${sz}" height="${sz}" fill="var(--star)" opacity="${(0.3 + rnd() * 0.6).toFixed(2)}"/>`;
    }
    return s;
  }
  const grid = planeGrid();
  const stars = planeStars();
</script>

<div class="plane" aria-hidden="true">
  <svg class="stars" viewBox="0 0 600 180" preserveAspectRatio="xMidYMid slice">{@html stars}</svg>
  <svg class="grid" viewBox="0 0 600 200" preserveAspectRatio="none">{@html grid}</svg>
</div>
