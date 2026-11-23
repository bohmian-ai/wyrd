<!--
  The shared sheet of the Repository and GitHub connection components: page
  heads, fact strips, ruled tables, state marks, banners, segmented choices
  and notes, all from tokens. Internal; hosts place the components that use it.
-->
<script lang="ts">
  import type { Snippet } from 'svelte';

  let { children }: { children: Snippet } = $props();
</script>

<div class="repo">{@render children()}</div>

<style>
  .repo {
    display: flex;
    flex-direction: column;
    gap: 14px;
    min-width: 0;
    color: var(--text);
    font: 14px/1.45 var(--font-sans);
  }
  .repo :global(.mono) {
    font-family: var(--font-mono);
  }
  .repo :global(.muted) {
    color: var(--muted);
  }
  .repo :global(.small) {
    font-size: 12px;
  }
  .repo :global(.h) {
    margin: 0 auto 0 0;
    color: var(--muted);
    font: 400 11px var(--font-mono);
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }
  .repo :global(.page-head),
  .repo :global(.xhead) {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    gap: 10px 16px;
    padding-bottom: 12px;
    border-bottom: 1px solid var(--border);
  }
  .repo :global(.xhead) {
    align-items: flex-start;
    gap: 12px 24px;
  }
  .repo :global(.grow),
  .repo :global(.xtitle) {
    display: flex;
    flex: 1 1 420px;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
  }
  .repo :global(.xname) {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 10px;
  }
  .repo :global(h1) {
    margin: 0;
    font-size: 22px;
    font-weight: var(--weight-strong);
    overflow-wrap: anywhere;
  }
  .repo :global(.xname h1) {
    font-size: 20px;
  }
  .repo :global(.xmeta) {
    color: var(--muted);
    font: 12px/1.6 var(--font-mono);
    overflow-wrap: anywhere;
  }
  .repo :global(.sep) {
    padding: 0 6px;
  }
  .repo :global(.desc) {
    max-width: 80ch;
    margin: 0;
    color: var(--muted);
    font-size: 14px;
  }
  .repo :global(.xactions),
  .repo :global(.toolbar),
  .repo :global(.ident) {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 8px;
  }
  .repo :global(.btn) {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    height: 32px;
    padding: 0 10px;
    border: 1px solid var(--border);
    border-radius: var(--r);
    background: var(--surface);
    color: var(--text);
    font: 13px var(--font-sans);
    text-decoration: none;
    white-space: nowrap;
    cursor: pointer;
  }
  .repo :global(.btn:hover:not(:disabled)) {
    background: var(--surface-hover);
  }
  .repo :global(.btn:disabled) {
    color: var(--muted);
    cursor: not-allowed;
  }
  .repo :global(.btn-sm) {
    height: 30px;
    font-size: 12px;
  }
  .repo :global(.btn-primary) {
    border-color: var(--declare);
    background: var(--declare);
    color: var(--declare-ink);
  }
  .repo :global(.btn-primary:hover:not(:disabled)) {
    background: var(--declare);
    opacity: 0.9;
  }
  .repo :global(.btn-danger) {
    border-color: var(--danger);
    color: var(--danger);
  }
  .repo :global(.btn-copied),
  .repo :global(.btn-copied:hover:not(:disabled)) {
    border-color: var(--evidence);
    background: var(--evidence);
    color: var(--evidence-ink);
  }
  /* A typed intent drawn as the link the approved design shows. */
  .repo :global(.link) {
    padding: 0;
    border: 0;
    background: transparent;
    color: var(--declare);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .repo :global(.link:hover:not(:disabled)) {
    text-decoration: underline;
  }
  .repo :global(.link:disabled) {
    color: var(--text);
    cursor: default;
  }
  :where(.repo) :global(a) {
    color: var(--declare);
  }
  .repo :global(a.btn) {
    color: var(--text);
  }
  .repo :global(:is(a, button, select, input):focus-visible) {
    outline: 2px solid var(--declare);
    outline-offset: 2px;
  }
  .repo :global(.kt) {
    display: inline-block;
    padding: 0 6px;
    border: 1px solid var(--selected-border);
    border-radius: var(--r);
    background: var(--declare-soft);
    font: 10px/16px var(--font-mono);
    letter-spacing: 0.04em;
    text-transform: uppercase;
    white-space: nowrap;
  }
  .repo :global(.tag) {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 0 6px;
    border: 1px dashed var(--border);
    border-radius: var(--r);
    color: var(--muted);
    font: 11px/17px var(--font-mono);
    white-space: nowrap;
  }
  .repo :global(.badge) {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 0 6px;
    border: 2px solid currentColor;
    border-radius: var(--r);
    font: 11px/16px var(--font-mono);
    white-space: nowrap;
  }
  .repo :global(.st) {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font: 11px var(--font-mono);
    white-space: nowrap;
  }
  .repo :global(.c-ok) {
    color: var(--ok);
  }
  .repo :global(.c-danger) {
    color: var(--danger);
  }
  .repo :global(.c-warn) {
    color: var(--warn);
  }
  .repo :global(.c-muted) {
    color: var(--muted);
  }
  .repo :global(.c-text) {
    color: var(--text);
  }
  .repo :global(.banner) {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
    margin: 0;
    padding: 8px 12px;
    border: 1px solid var(--border);
    border-left: 2px solid var(--declare);
    border-radius: var(--r);
    background: var(--surface);
    font-size: 13px;
  }
  .repo :global(.banner.warn) {
    border-left-color: var(--warn);
  }
  .repo :global(.search) {
    flex: 1 1 260px;
    min-width: 0;
    height: 32px;
    padding: 0 10px;
    border: 1px solid var(--border);
    border-radius: var(--r);
    background: var(--surface);
    color: var(--text);
    font: 12px var(--font-mono);
  }
  .repo :global(form.toolbar) {
    flex: 1 1 260px;
  }
  .repo :global(.segs) {
    display: inline-flex;
    flex-wrap: wrap;
  }
  .repo :global(.seg) {
    height: 28px;
    padding: 0 10px;
    border: 1px solid var(--border);
    background: var(--surface);
    color: var(--muted);
    font: 12px var(--font-sans);
    cursor: pointer;
  }
  .repo :global(.seg + .seg) {
    border-left: 0;
  }
  .repo :global(.seg[aria-pressed='true']) {
    border-color: var(--selected-border);
    background: var(--declare-soft);
    color: var(--text);
  }
  .repo :global(.seg:hover:not(:disabled)) {
    color: var(--text);
  }
  .repo :global(.sub) {
    color: var(--muted);
    font: 11px var(--font-mono);
  }
  .repo :global(.outs-wrap) {
    overflow-x: auto;
    border: 1px solid var(--border-soft);
  }
  .repo :global(.outs) {
    width: 100%;
    border-collapse: collapse;
    background: var(--surface);
    font: 12px var(--font-mono);
  }
  .repo :global(.outs th) {
    padding: 6px 10px;
    border-bottom: 1px solid var(--border-soft);
    color: var(--muted);
    font-size: 11px;
    font-weight: 400;
    letter-spacing: 0.06em;
    text-align: left;
    text-transform: uppercase;
    white-space: nowrap;
  }
  .repo :global(.outs td) {
    padding: 0;
    border-bottom: 1px solid var(--border-soft);
  }
  .repo :global(.outs td span.mono),
  .repo :global(.outs td .st) {
    white-space: nowrap;
  }
  .repo :global(.outs tbody tr:hover td) {
    background: var(--surface-hover);
  }
  .repo :global(.cellbox) {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
    padding: 7px 10px;
  }
  .repo :global(.note) {
    max-width: 80ch;
    margin: 0;
    color: var(--muted);
    font-size: 13px;
  }
  .repo :global(.note.box) {
    padding: 8px 10px;
    border: 1px dashed var(--border);
    border-radius: var(--r);
  }
  .repo :global(.prose) {
    margin: 0;
    max-width: 72ch;
    font: 13px/1.5 var(--font-sans);
  }
  .repo :global(.facts) {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
    margin: 0;
    border: 1px solid var(--border);
    border-radius: var(--r);
    background: var(--surface);
  }
  .repo :global(.facts > div) {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
    padding: 10px 12px;
    border-right: 1px solid var(--border-soft);
  }
  .repo :global(.facts > div:last-child) {
    border-right: 0;
  }
  .repo :global(.facts dt) {
    color: var(--muted);
    font: 11px var(--font-mono);
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }
  .repo :global(.facts dd) {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 3px;
    margin: 0;
  }
  .repo :global(.why) {
    color: var(--muted);
    font-size: 12px;
  }
  .repo :global(.vsel) {
    height: 30px;
    padding: 0 6px;
    border: 1px solid var(--border);
    border-radius: var(--r);
    background: var(--surface);
    color: var(--text);
    font: 12px var(--font-mono);
  }
  .repo :global(.split) {
    display: grid;
    grid-template-columns: minmax(0, 1.2fr) minmax(0, 1fr);
    gap: 14px;
  }
  .repo :global(.section) {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }
  .repo :global(.section-head) {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 12px;
  }
  .repo :global(.panel) {
    min-width: 0;
    border: 1px solid var(--border);
    border-radius: var(--r);
    background: var(--surface);
  }
  .repo :global(.tabs) {
    display: flex;
    padding: 0 6px;
    border-bottom: 1px solid var(--border-soft);
    overflow-x: auto;
  }
  .repo :global(.tab) {
    padding: 9px 12px;
    border: 0;
    border-bottom: 2px solid transparent;
    background: transparent;
    color: var(--muted);
    font: 13px var(--font-sans);
    white-space: nowrap;
    cursor: pointer;
  }
  .repo :global(.tab[aria-selected='true']) {
    border-bottom-color: var(--declare);
    color: var(--text);
  }
  .repo :global(.tabpanel) {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
    padding: 12px;
  }
  .repo :global(pre) {
    margin: 0;
    padding: 10px 12px;
    border: 1px solid var(--border-soft);
    border-radius: var(--r);
    background: var(--bg);
    font: 12px/1.5 var(--font-mono);
    overflow-x: auto;
  }
  .repo :global(dialog) {
    width: min(480px, calc(100vw - 32px));
    margin: auto;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: var(--r);
    background: var(--surface);
    color: var(--text);
  }
  .repo :global(dialog::backdrop) {
    background: color-mix(in srgb, var(--bg) 72%, transparent);
  }
  .repo :global(.dlg) {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin: 0;
    padding: 16px 18px;
  }
  .repo :global(.dlg h2) {
    margin: 0;
    font-size: 16px;
  }
  .repo :global(.dlg-list) {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin: 0;
    padding-left: 18px;
    font-size: 13px;
  }
  .repo :global(.dlg-actions) {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
  @media (max-width: 767px) {
    .repo :global(.split) {
      grid-template-columns: minmax(0, 1fr);
    }
  }
</style>
