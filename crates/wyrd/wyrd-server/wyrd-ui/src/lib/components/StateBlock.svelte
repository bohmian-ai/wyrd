<script lang="ts">
  // The one place the workbench says "there is nothing to draw here, and why". Every
  // async, authorization and earned-absent outcome renders through it so no surface
  // silently shows an empty region — or, worse, reports absent/stale/unauthorized data
  // as healthy. It states the situation in words; the glyph and border are secondary.
  type State = 'loading' | 'empty' | 'partial' | 'error' | 'unauthorized' | 'absent';
  let {
    state,
    title,
    detail,
    code,
    actionLabel,
    actionHref
  }: {
    state: State;
    title: string;
    detail?: string;
    code?: string;
    actionLabel?: string;
    actionHref?: string;
  } = $props();

  const glyphs: Record<State, string> = {
    loading: '◴',
    empty: '○',
    partial: '◐',
    error: '✕',
    unauthorized: '⊘',
    absent: '–'
  };
</script>

<div
  class="wy-state"
  data-state={state}
  role={state === 'loading' ? 'status' : 'note'}
  aria-busy={state === 'loading' ? 'true' : undefined}
>
  {#if state === 'empty'}
    <!-- The brand's node-and-curve motif, whispered behind quiet-empty regions only. -->
    <svg class="bg" aria-hidden="true" viewBox="0 0 320 120" preserveAspectRatio="xMaxYMid slice">
      <path d="M0 90 C 80 90, 120 30, 200 30 S 300 70, 320 70" fill="none" stroke="var(--brand-strong)" stroke-width="1.5" />
      <path d="M0 30 C 90 30, 150 100, 240 100 S 310 40, 320 40" fill="none" stroke="var(--lime-text)" stroke-width="1.5" />
      <circle cx="200" cy="30" r="4" fill="var(--brand-strong)" />
      <circle cx="240" cy="100" r="4" fill="var(--lime-text)" />
      <rect x="112" y="58" width="9" height="9" rx="2" fill="var(--muted)" />
    </svg>
  {/if}
  <div class="hd">
    <span class="gl" aria-hidden="true">{glyphs[state]}</span>
    <span class="st">{state}</span>
    <span class="t">{title}</span>
  </div>
  {#if detail}<p class="d">{detail}</p>{/if}
  {#if code}<p class="c">{code}</p>{/if}
  {#if actionLabel && actionHref}<a class="a" href={actionHref}>{actionLabel}</a>{/if}
</div>

<style>
  .wy-state {
    --sc: var(--muted);
    position: relative;
    overflow: hidden;
    border: 2px dashed var(--sc);
    border-radius: var(--r);
    background: var(--surface);
    padding: 14px;
    color: var(--text);
    font-family: var(--font-sans);
  }
  .bg {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    opacity: 0.14;
    pointer-events: none;
  }
  .hd,
  .d,
  .c,
  .a {
    position: relative;
  }
  .wy-state[data-state='error'] {
    --sc: var(--danger-text);
  }
  .wy-state[data-state='unauthorized'],
  .wy-state[data-state='partial'] {
    --sc: var(--warn-text);
  }
  .hd {
    display: flex;
    align-items: baseline;
    flex-wrap: wrap;
    gap: 7px;
  }
  .gl,
  .st {
    font-family: var(--fm);
    font-size: 8.5px;
    font-weight: 700;
    letter-spacing: 0.5px;
    text-transform: uppercase;
    color: var(--sc);
  }
  .gl {
    font-size: 11px;
  }
  .t {
    font-family: var(--font-display);
    font-size: 13px;
    font-weight: 700;
  }
  .d {
    margin: 6px 0 0;
    font-size: 12px;
    color: var(--muted);
  }
  .c {
    margin: 6px 0 0;
    font-family: var(--fm);
    font-size: 10px;
    color: var(--muted);
  }
  .a {
    display: inline-block;
    margin-top: 9px;
    font-family: var(--fm);
    font-size: 11px;
    font-weight: 700;
    color: var(--brand-strong);
  }
</style>
