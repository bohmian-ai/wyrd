<script lang="ts">
  import { base } from '$app/paths';
  import Fuji from '$lib/mocks/Fuji.svelte';

  const dirs = [
    {
      id: 'a',
      name: 'Arcade Cabinet',
      tag: 'Max personality',
      desc: 'Dark always-on marquee, perspective-plane hero, Press Start 2P, layer-stack visual. Loudest brand expression.',
      kind: 'rune'
    },
    {
      id: 'b',
      name: 'Workbench / Quiet',
      tag: 'Docs-first',
      desc: 'Clean theme-aware chrome, editorial hero with a live registry panel, restrained arcade DNA. Reads like a serious reference manual.',
      kind: 'client'
    },
    {
      id: 'c',
      name: 'Terminal / Console',
      tag: 'Hacker / agent-first',
      desc: 'Terminal-window chrome, VT323 boot-screen hero, command/output centerpiece, monospace-forward. CLI/API flavored.',
      kind: 'server'
    },
    {
      id: 'd',
      name: 'Quest Mode',
      tag: 'Full 8-bit / NES',
      desc: 'HUD status bar (P1 · WORLD · SCORE · COINS), VT323 on-screen body, NES dialogue boxes, PRESS START title hero, stage-select grid, locked Fathom world. Hardest into the game feel.',
      kind: 'control'
    }
  ];
</script>

<div class="mk">
  <div class="gallery">
    <div style="display:flex;align-items:center;gap:11px;margin-bottom:6px">
      <Fuji size={28} />
      <span style="font-family:var(--font-arcade);font-size:13px;letter-spacing:1.5px">MITARI DOCS</span>
    </div>
    <h1>Design directions</h1>
    <p class="sub">
      Three on-brand skins for the docs skeleton — all use the locked tokens
      (palette.json), both light &amp; dark, hard-shadow / zero-blur. Toggle theme
      inside any mock. Pick a direction; the full archetype set gets built out in it.
    </p>

    <div class="grid3">
      {#each dirs as d}
        <div class="card">
          <div class={`kbar ${d.kind}`}></div>
          <div class="ct"><span class={`d ${d.kind}`}></span>{d.name}</div>
          <div class="meta" style="margin-top:0;margin-bottom:10px">{d.tag}</div>
          <p style="flex:1">{d.desc}</p>
          <div style="display:flex;gap:8px;margin-top:14px">
            <a class="btn btn-lime btn-sm" href={`${base}/mocks/${d.id}/landing/`}>Landing</a>
            <a class="btn btn-ghost btn-sm" href={`${base}/mocks/${d.id}/article/`}>Doc article</a>
          </div>
        </div>
      {/each}
    </div>

    <div style="margin-top:34px">
      <div class="sec-eye">FULL ARCHETYPE SET · DIRECTION A</div>
      <div class="grid3">
        <a class="card" href={`${base}/mocks/a/reference/`}><div class="ct">Reference / API</div><p>Dense schema &amp; error tables, anchor rail.</p></a>
        <a class="card" href={`${base}/mocks/a/hub/`}><div class="ct">Section index</div><p>Tile grid hub for a top-level section.</p></a>
        <a class="card" href={`${base}/mocks/a/fathom/`}><div class="ct">Fathom · coming soon</div><p>The teaser surface (Decision 2).</p></a>
        <a class="card" href={`${base}/mocks/a/notfound/`}><div class="ct">404 · game over</div><p>Branded dead-end.</p></a>
      </div>
    </div>
  </div>
</div>
