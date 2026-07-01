<script lang="ts">
  import { base } from '$app/paths';
  import { page } from '$app/state';
  import Plane from '$lib/components/Plane.svelte';
  import WyrdMark from '$lib/components/WyrdMark.svelte';
</script>

<svelte:head>
  <title>{page.status} — Wyrd docs</title>
</svelte:head>

<!-- Branded 404 / error dead-end. Plane fills the section behind the content;
     the layout renders this full-width (isError branch in +layout.svelte).
     adapter-static emits this as 404.html via fallback: '404.html'. -->
<section class="nf" aria-label="Page not found">
  <Plane />
  <div class="nf-in">
    <WyrdMark size={48} />
    <div class="nf-big">{page.status}</div>
    <div class="nf-sub">GAME OVER — {page.error?.message ?? 'page not found'}</div>
    <p class="nf-lead">That route was never registered. No Card lives here.</p>
    <div class="nf-coin">INSERT COIN TO CONTINUE</div>
    <nav class="nf-cta" aria-label="Recovery links">
      <a class="nf-btn nf-btn-lime" href="{base}/">▶ Back to start</a>
      <a class="nf-btn nf-btn-ghost" href="{base}/start-here/what-is-wyrd/">Browse the docs</a>
    </nav>
  </div>
</section>

<style>
  .nf {
    position: relative;
    min-height: 70vh;
    display: flex;
    align-items: center;
    justify-content: center;
    overflow: hidden;
  }
  .nf-in {
    position: relative;
    z-index: 1;
    max-width: 520px;
    margin: 0 auto;
    padding: 48px 24px 56px;
    text-align: center;
  }
  .nf-big {
    font-family: var(--font-display);
    font-size: clamp(4rem, 14vw, 8rem);
    font-weight: 900;
    line-height: 1;
    letter-spacing: -0.04em;
    color: var(--rune-strong);
    margin: 12px 0 8px;
  }
  .nf-sub {
    font-family: var(--font-mono);
    font-size: 0.72rem;
    font-weight: 700;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: var(--muted);
    margin: 0 0 14px;
  }
  .nf-lead {
    font-size: 1rem;
    line-height: 1.6;
    color: var(--muted);
    margin: 0 0 14px;
  }
  .nf-coin {
    font-family: var(--font-mono);
    font-size: 0.62rem;
    font-weight: 700;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: var(--muted);
    border: 2px dashed var(--border);
    border-radius: var(--r);
    padding: 6px 14px;
    display: inline-block;
    margin: 0 0 24px;
    opacity: 0.7;
  }
  .nf-cta {
    display: flex;
    gap: 12px;
    justify-content: center;
    flex-wrap: wrap;
  }
  .nf-btn {
    font-family: var(--font-mono);
    font-size: 0.82rem;
    font-weight: 700;
    text-decoration: none;
    padding: 9px 18px;
    border: 2px solid var(--border);
    border-radius: var(--r);
    box-shadow: 3px 3px 0 0 var(--shadow);
  }
  .nf-btn-lime {
    background: var(--lime);
    color: var(--lime-ink);
  }
  .nf-btn-ghost {
    background: var(--surface);
    color: var(--text);
  }
  .nf-btn:hover {
    transform: translate(-1px, -1px);
    box-shadow: 5px 5px 0 0 var(--shadow);
  }
  @media (max-width: 640px) {
    .nf-in {
      padding: 36px 16px 48px;
    }
  }
</style>
