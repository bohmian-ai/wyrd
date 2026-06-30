<script lang="ts">
  import { base } from '$app/paths';
  import { getEntry } from '$lib/content';
  import Plane from '$lib/components/Plane.svelte';
  import WyrdMark from '$lib/components/WyrdMark.svelte';
  import type { PageData } from './$types';

  let { data }: { data: PageData } = $props();

  // Resolve the component from the eager content map so it renders
  // synchronously into the prerendered HTML (no async-pending shell).
  const Content = $derived(getEntry(data.slug)?.default);
</script>

<svelte:head>
  <title>{data.metadata.title} — Wyrd docs</title>
  {#if data.metadata.description}
    <meta name="description" content={data.metadata.description} />
  {/if}
</svelte:head>

{#if data.archetype === 'fathom'}
  <!-- Fathom "coming soon" teaser archetype. Plane fills the section behind the
       content so the animated starfield/grid renders in both themes. The layout
       chrome (header, sidebar) still wraps this; the teaser fills the main col. -->
  <section class="fathom-teaser">
    <Plane />
    <div class="ft-in">
      <WyrdMark size={48} />
      <p class="ft-eye">FATHOM · COMING SOON</p>
      <h1>{data.metadata.title}</h1>
      {#if data.metadata.description}
        <p class="ft-lead">{data.metadata.description}</p>
      {:else}
        <p class="ft-lead">
          Wyrd records what every model, prompt, agent, and dataset declared.
          Fathom reads that typed graph and turns runtime behavior into evidence —
          leakage, eval, drift versus spec, and audit. We're building it in the open.
        </p>
      {/if}
      <form class="ft-form" onsubmit={(e) => e.preventDefault()}>
        <input type="email" placeholder="you@team.dev" aria-label="Email address" />
        <button class="ft-btn" type="submit">▶ Notify me</button>
      </form>
      <div class="ft-chips">
        <span>Leakage detection</span>
        <span>Eval as evidence</span>
        <span>Drift vs spec</span>
        <span>Runtime audit</span>
      </div>
    </div>
  </section>
{:else if data.archetype === 'reference'}
  <!-- Reference/API shell archetype. Dense tables + anchor navigation are in
       the generated content (07); this commit provides the layout shell only. -->
  <article class="doc-content doc-ref" data-archetype="reference">
    {#if Content}<Content />{/if}
  </article>
{:else if data.archetype === 'hub'}
  <!-- Section index/hub archetype. Tile grids live in the content itself
       (CardTileGrid is auto-injected by the MDsveX layout set). -->
  <article class="doc-content doc-hub" data-archetype="hub">
    {#if Content}<Content />{/if}
  </article>
{:else}
  <!-- Default: doc-article archetype. -->
  <article class="doc-content" data-archetype="article">
    {#if Content}<Content />{/if}
  </article>
{/if}

<style>
  /* Fathom teaser — fills the main content column with a branded holding page. */
  .fathom-teaser {
    position: relative;
    min-height: 60vh;
    display: flex;
    align-items: center;
    justify-content: center;
    overflow: hidden;
  }
  .ft-in {
    position: relative;
    z-index: 1;
    max-width: 600px;
    margin: 0 auto;
    padding: 48px 24px 56px;
    text-align: center;
  }
  .ft-eye {
    font-family: var(--font-mono);
    font-size: 0.66rem;
    font-weight: 700;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: var(--muted);
    margin: 16px 0 10px;
  }
  .ft-in h1 {
    font-family: var(--font-display);
    font-size: clamp(1.5rem, 4vw, 2.4rem);
    line-height: 1.15;
    letter-spacing: -0.02em;
    margin: 0 0 16px;
  }
  .ft-lead {
    font-size: 1rem;
    line-height: 1.6;
    color: var(--muted);
    margin: 0 0 22px;
  }
  .ft-form {
    display: flex;
    gap: 8px;
    justify-content: center;
    flex-wrap: wrap;
    margin: 0 0 18px;
  }
  .ft-form input {
    font-family: var(--font-mono);
    font-size: 0.82rem;
    padding: 9px 14px;
    background: var(--surface);
    border: 2px solid var(--border);
    border-radius: var(--r);
    color: var(--text);
    min-width: 200px;
  }
  .ft-btn {
    font-family: var(--font-mono);
    font-size: 0.82rem;
    font-weight: 700;
    padding: 9px 18px;
    background: var(--lime);
    color: var(--lime-ink);
    border: 2px solid var(--border);
    border-radius: var(--r);
    box-shadow: 3px 3px 0 0 var(--shadow);
    cursor: pointer;
    letter-spacing: 0.04em;
  }
  .ft-chips {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
    justify-content: center;
  }
  .ft-chips span {
    font-family: var(--font-mono);
    font-size: 0.62rem;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    padding: 4px 10px;
    border: 2px solid var(--border);
    border-radius: var(--r);
    background: var(--surface-2);
    color: var(--muted);
  }
</style>
