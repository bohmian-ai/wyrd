<script lang="ts">
  import { onMount } from 'svelte';
  import { base } from '$app/paths';

  // Homepage arcade hero — "The Plane" synthwave scene. Markup mirrors the
  // source-of-truth §7 LOUD register; styling lives in the global
  // `.wyrd-arcade-hero` recipe in src/styles/wyrd.css. The scene painter (grid
  // scroll toward the vanishing point, seeded starfield, pixel sprite) runs in
  // onMount and is gated behind prefers-reduced-motion (freeze to a static
  // frame). Geometry/colors resolve through CSS tokens referenced inline.
  type Stat = { n: string; k: string };
  type Cta = { label: string; href: string; secondary?: boolean };

  let {
    wordmark = 'WYRD',
    ornament = 'docs',
    title,
    tagline,
    taglineBold,
    stats = [],
    ctas = []
  }: {
    wordmark?: string;
    ornament?: string;
    title: string;
    tagline?: string;
    taglineBold?: string;
    stats?: Stat[];
    ctas?: Cta[];
  } = $props();

  let starsEl = $state<SVGSVGElement>();
  let gridEl = $state<SVGSVGElement>();
  let sprEl = $state<HTMLDivElement>();

  function ctaHref(href: string): string {
    return /^https?:\/\//.test(href) ? href : `${base}${href}`;
  }

  onMount(() => {
    const W = 600;
    const VIEW_H = 200; // grid SVG viewBox height
    const MAP_H = 232; // map height > viewBox so the nearest lines clip off-screen
    const P = 2.0; // perspective exponent: lines bunch toward the horizon
    const N = 14; // number of scrolling depth lines
    const cx = 300; // vanishing point x
    const PERIOD = 6000; // ms for one depth line to travel horizon -> viewer

    const reduced = () =>
      window.matchMedia && window.matchMedia('(prefers-reduced-motion: reduce)').matches;

    // Static scene parts: colored plane wedges + radial perspective lines.
    function gridStatic(): string {
      let s = '';
      s += `<polygon points="${cx},0 0,${VIEW_H} ${W / 3},${VIEW_H}" fill="var(--client)" opacity=".20"/>`;
      s += `<polygon points="${cx},0 ${W / 3},${VIEW_H} ${(2 * W) / 3},${VIEW_H}" fill="var(--server)" opacity=".15"/>`;
      s += `<polygon points="${cx},0 ${(2 * W) / 3},${VIEW_H} ${W},${VIEW_H}" fill="var(--control)" opacity=".22"/>`;
      for (let i = -4; i <= 16; i++) {
        const x = i * 50;
        s += `<line x1="${cx}" y1="0" x2="${x}" y2="${VIEW_H}" stroke="var(--grid)" stroke-width="1.3" opacity=".42"/>`;
      }
      return s;
    }

    function heroStars(): string {
      let s = '';
      let seed = 97;
      const rnd = () => (seed = (seed * 16807) % 2147483647) / 2147483647;
      for (let i = 0; i < 46; i++) {
        const x = (rnd() * 600) | 0;
        const y = (rnd() * 118) | 0;
        const sz = rnd() < 0.18 ? 2 : 1;
        s += `<rect x="${x}" y="${y}" width="${sz}" height="${sz}" fill="var(--star)"/>`;
      }
      return s;
    }

    // side-profile armored agent — helmet + lime visor, backpack, arm-cannon
    const SPR = [
      '................',
      '.....VVVVV......',
      '....VVVVVVV.....',
      '....VVVVLLV.....',
      '...DVVVVVVV.....',
      '...DVVVVVVV.....',
      '...DVVVVVVVVLL..',
      '...DVVVVVVVVLL..',
      '....VVVVVVV.....',
      '....VVVVVV......',
      '....VVVVVV......',
      '....VVVVVV......',
      '....DDVVVV......',
      '....DDVVVV......',
      '...DD..VVV......',
      '..DD...VVVV.....'
    ];
    function spriteSVG(map: string[]): string {
      const C: Record<string, string> = {
        V: 'var(--rune-strong)',
        D: 'var(--spr-d)',
        L: 'var(--lime)'
      };
      let r = '';
      map.forEach((row, y) => {
        row.split('').forEach((ch, x) => {
          if (C[ch]) r += `<rect x="${x}" y="${y}" width="1.04" height="1.04" fill="${C[ch]}"/>`;
        });
      });
      return `<svg class="spr" viewBox="0 0 16 16" shape-rendering="crispEdges">${r}</svg>`;
    }

    if (starsEl) starsEl.innerHTML = heroStars();
    if (sprEl) sprEl.innerHTML = spriteSVG(SPR);

    let raf = 0;
    if (gridEl) {
      let lineSvg = '';
      for (let j = 0; j < N; j++) {
        lineSvg += '<line class="gln" x1="0" x2="' + W + '" stroke="var(--grid)" stroke-width="1.3"/>';
      }
      gridEl.innerHTML = gridStatic() + lineSvg;
      const lines = gridEl.querySelectorAll<SVGLineElement>('.gln');

      const place = (phase: number) => {
        for (let j = 0; j < N; j++) {
          const u = (j / N + phase) % 1; // 0 = horizon, 1 = nearest
          const y = MAP_H * Math.pow(u, P);
          const ln = lines[j];
          ln.setAttribute('y1', String(y));
          ln.setAttribute('y2', String(y));
          // fade in from the horizon so the wrap point is invisible
          ln.setAttribute('opacity', (Math.min(u / 0.12, 1) * 0.5).toFixed(3));
        }
      };

      if (reduced()) {
        place(0.0);
      } else {
        let start: number | null = null;
        const frame = (ts: number) => {
          if (start === null) start = ts;
          const phase = ((((ts - start) / PERIOD) % 1) + 1) % 1;
          place(phase);
          raf = requestAnimationFrame(frame);
        };
        raf = requestAnimationFrame(frame);
      }
    }

    return () => {
      if (raf) cancelAnimationFrame(raf);
    };
  });
</script>

{#snippet fuji(w: number, h: number, inline = false)}
  <svg
    class="fuji"
    viewBox="0 0 100 100"
    aria-hidden="true"
    style:width={`${w}px`}
    style:height={`${h}px`}
    style:display={inline ? 'inline-block' : null}
    style:vertical-align={inline ? '-2px' : null}
    style:margin-right={inline ? '5px' : null}
  >
    <path class="p" d="M42 92 L30 92 L6 8 L22 8 Z" />
    <path class="p" d="M58 92 L70 92 L94 8 L78 8 Z" />
    <path class="c" d="M44 8 h12 v84 h-12 z" />
  </svg>
{/snippet}

<header class="wyrd-arcade-hero">
  <div class="l-bar"></div>
  <div class="l-pad">
    <div class="l-scene" aria-hidden="true">
      <div class="l-sky"></div>
      <svg
        bind:this={starsEl}
        class="l-stars"
        viewBox="0 0 600 118"
        preserveAspectRatio="xMidYMid slice"
      ></svg>
      <div class="l-sun">
        {@render fuji(92, 92)}
      </div>
      <div class="l-horizon"></div>
      <svg bind:this={gridEl} class="l-grid" viewBox="0 0 600 200" preserveAspectRatio="none"></svg>
      <div class="l-walker">
        <div class="spr-stack"><div class="spr-a" bind:this={sprEl}></div></div>
      </div>
    </div>
    <div class="l-content">
      <span class="l-orn">{@render fuji(13, 13, true)}{ornament}</span>
      <span class="l-wm">{@render fuji(16, 16)}{wordmark}<i></i></span>
      <div><h1 class="l-hero">{title}</h1></div>
      {#if tagline || taglineBold}
        <p class="l-tag">
          {tagline}
          {#if taglineBold}<b>{taglineBold}</b>{/if}
        </p>
      {/if}
      {#if stats.length > 0}
        <div class="l-stats">
          {#each stats as s (s.k)}
            <div class="l-stat">
              <div class="n">{s.n}</div>
              <div class="k">{s.k}</div>
            </div>
          {/each}
        </div>
      {/if}
      {#if ctas.length > 0}
        <div class="l-ctas">
          {#each ctas as c (c.label)}
            <a class={c.secondary ? 'l-cta l-cta2' : 'l-cta'} href={ctaHref(c.href)}>{c.label}</a>
          {/each}
        </div>
      {/if}
    </div>
  </div>
</header>
