<script lang="ts">
  import type { Snippet } from 'svelte';

  let {
    title,
    accent = 'var(--rune-strong)',
    chip,
    chipAccent,
    onclose,
    children,
    actions
  }: {
    title: string;
    accent?: string;
    chip?: string;
    chipAccent?: string;
    onclose?: () => void;
    children?: Snippet;
    actions?: Snippet;
  } = $props();
</script>

<section class="wy-drawer" style={`--accent:${accent}`}>
  <div class="top"></div>
  <header class="head">
    <span class="ttl">{title}</span>
    {#if chip}<span class="chip" style={chipAccent ? `--chip-accent:${chipAccent}` : undefined}>{chip}</span>{/if}
    {#if onclose}<button class="x" type="button" aria-label="Close" onclick={onclose}>✕</button>{/if}
  </header>
  <div class="body">{@render children?.()}</div>
  {#if actions}<footer class="foot">{@render actions()}</footer>{/if}
</section>

<style>
  .wy-drawer {
    /* Fill the parent up to the design width; never overflow it. A consumer (e.g. a
       full-width mobile overlay) can widen the cap via --drawer-w. */
    width: 100%;
    max-width: var(--drawer-w, 340px);
    border: 2px solid var(--border);
    border-radius: var(--r);
    background: var(--surface);
    box-shadow: 6px 6px 0 0 var(--shadow);
    overflow: hidden;
    display: flex;
    flex-direction: column;
    color: var(--text);
  }
  .top {
    height: 4px;
    background: var(--accent);
  }
  .head {
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 12px 13px;
    border-bottom: 2px solid var(--border);
  }
  .ttl {
    font-family: var(--fm);
    font-size: 13px;
    font-weight: 700;
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .chip {
    font-family: var(--fm);
    font-size: 8.5px;
    font-weight: 700;
    letter-spacing: 0.4px;
    text-transform: uppercase;
    padding: 3px 7px;
    border: 2px solid var(--chip-accent, var(--accent));
    border-radius: 4px;
    color: var(--chip-accent, var(--accent));
    flex: 0 0 auto;
  }
  .x {
    margin-left: auto;
    width: 24px;
    height: 24px;
    border: 2px solid var(--border);
    border-radius: 4px;
    background: var(--surface);
    display: flex;
    align-items: center;
    justify-content: center;
    font-family: var(--fm);
    font-size: 12px;
    color: var(--muted);
    cursor: pointer;
    flex: 0 0 auto;
  }
  .foot {
    padding: 11px 13px;
    border-top: 2px solid var(--border);
    display: flex;
    gap: 8px;
    background: var(--surface);
  }

  /* ---- drawer body design language (used by SpanPanel / EvalPanel / DriftPanel) ---- */
  .body :global(.d-sec) {
    padding: 12px 13px;
    border-bottom: 2px dashed var(--border);
  }
  .body :global(.d-sec:last-child) {
    border-bottom: 0;
  }
  .body :global(.d-sl) {
    font-family: var(--fm);
    font-size: 8.5px;
    font-weight: 700;
    letter-spacing: 0.6px;
    text-transform: uppercase;
    color: var(--muted);
    margin-bottom: 9px;
  }
  .body :global(.d-kv) {
    display: grid;
    grid-template-columns: 6.2rem 1fr;
    gap: 5px 9px;
    font-family: var(--fm);
    font-size: 10.5px;
    margin: 0;
  }
  .body :global(.d-kv dt) {
    color: var(--muted);
    text-transform: uppercase;
    font-size: 8.5px;
    letter-spacing: 0.3px;
    align-self: center;
  }
  .body :global(.d-kv dd) {
    color: var(--text);
    word-break: break-all;
    margin: 0;
  }
  .body :global(.d-kv .lnk) {
    color: var(--rune-strong);
    font-weight: 700;
  }
  .body :global(.d-row) {
    display: flex;
    align-items: center;
    gap: 9px;
    flex-wrap: wrap;
    font-family: var(--fm);
    font-size: 11px;
  }
  .body :global(.d-dot) {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    display: inline-block;
  }
  .body :global(.d-big) {
    display: flex;
    align-items: baseline;
    gap: 11px;
  }
  .body :global(.d-big .num) {
    font-family: var(--fm);
    font-weight: 700;
    font-size: 30px;
    line-height: 1;
    color: var(--rune-strong);
  }
  .body :global(.d-big .sub) {
    font-family: var(--fm);
    font-size: 10px;
    color: var(--muted);
  }
  .body :global(.d-metric) {
    display: grid;
    grid-template-columns: 6rem 1fr 2.1rem;
    gap: 9px;
    align-items: center;
    font-family: var(--fm);
    font-size: 10px;
    margin-bottom: 7px;
  }
  .body :global(.d-metric:last-child) {
    margin-bottom: 0;
  }
  .body :global(.d-metric .bar) {
    height: 11px;
    border: 2px solid var(--border);
    border-radius: 2px;
    background: var(--surface-2);
    overflow: hidden;
  }
  .body :global(.d-metric .bar i) {
    display: block;
    height: 100%;
  }
  .body :global(.d-metric .v) {
    text-align: right;
    font-weight: 700;
    color: var(--text);
  }
  .body :global(.d-mini) {
    display: flex;
    gap: 8px;
  }
  .body :global(.d-mstat) {
    flex: 1;
    border: 2px solid var(--border);
    border-radius: 4px;
    background: var(--surface);
    padding: 7px 9px;
  }
  .body :global(.d-mstat .k) {
    font-family: var(--fm);
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.4px;
    text-transform: uppercase;
    color: var(--muted);
  }
  .body :global(.d-mstat .n) {
    font-family: var(--fm);
    font-size: 14px;
    font-weight: 700;
    color: var(--text);
    margin-top: 3px;
  }
  .body :global(.d-ev) {
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 4px 0;
    border-left: 2px solid var(--border);
    padding-left: 11px;
    margin-left: 3px;
    position: relative;
    font-family: var(--fm);
    font-size: 10px;
  }
  .body :global(.d-ev::before) {
    content: '';
    position: absolute;
    left: -5px;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--rune-strong);
    border: 2px solid var(--surface);
  }
  .body :global(.d-ev .t) {
    margin-left: auto;
    color: var(--muted);
  }
  .body :global(.d-pos) {
    position: relative;
    height: 18px;
    border: 2px solid var(--border);
    border-radius: 3px;
    background: var(--surface-2);
    overflow: hidden;
    margin-top: 10px;
  }
  .body :global(.d-pos i) {
    position: absolute;
    top: 0;
    bottom: 0;
    background: color-mix(in srgb, var(--rune-strong) 30%, var(--surface));
    border-left: 2px solid var(--rune-strong);
    border-right: 2px solid var(--rune-strong);
  }
  .body :global(.d-legend) {
    display: flex;
    gap: 14px;
    margin-top: 9px;
    font-family: var(--fm);
    font-size: 9.5px;
    color: var(--muted);
  }
  .body :global(.d-legend i) {
    display: inline-block;
    width: 10px;
    height: 10px;
    border: 2px solid var(--border);
    border-radius: 2px;
    margin-right: 5px;
    vertical-align: -1px;
  }
</style>
