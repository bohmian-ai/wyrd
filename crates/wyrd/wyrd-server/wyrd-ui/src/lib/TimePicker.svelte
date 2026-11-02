<!--
  The Observe time range control: relative presets, a custom From/To that
  accepts `now`, `now-2h`, `yesterday` or an ISO time in the chosen zone, the
  UTC/Local zone and the ranges recently committed in this browser. It only
  reports a valid range through `onapply`; the host owns the applied state.
-->
<script lang="ts" module>
  /** A committed range: a relative preset or absolute ISO bounds. */
  export type PickedRange = { readonly last: string } | { readonly from: string; readonly to: string };
  /** The zone instants are shown and typed in. */
  export type Zone = 'utc' | 'local';

  const UNITS = { s: 1_000, m: 60_000, h: 3_600_000, d: 86_400_000, w: 604_800_000 } as const;
  const NAMES = { m: 'minute', h: 'hour', d: 'day' } as const;

  /** A preset such as `6h` as "Last 6 hours". */
  export function presetLabel(last: string): string {
    const amount = Number(last.slice(0, -1));
    const name = NAMES[last.slice(-1) as keyof typeof NAMES] ?? last.slice(-1);
    return `Last ${amount} ${name}${amount === 1 ? '' : 's'}`;
  }

  /** Midnight starting the day of `ms` in `zone`. */
  function midnight(ms: number, zone: Zone): number {
    const date = new Date(ms);
    return zone === 'utc' ? Date.UTC(date.getUTCFullYear(), date.getUTCMonth(), date.getUTCDate()) : new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime();
  }

  /**
   * Read one typed instant as epoch milliseconds: `now`, `now-<n><s|m|h|d|w>`,
   * `yesterday` (its midnight), or an ISO date or time. A time without an
   * offset reads in `zone`. Anything else is `undefined`.
   */
  export function readInstant(text: string, zone: Zone, now: number): number | undefined {
    const value = text.trim().toLowerCase();
    if (value === 'now') return now;
    if (value === 'yesterday') return midnight(now, zone) - UNITS.d;
    const relative = /^now\s*-\s*(\d{1,5})\s*([smhdw])$/.exec(value);
    if (relative) return now - Number(relative[1]) * UNITS[relative[2] as keyof typeof UNITS];
    const iso = /^(\d{4}-\d\d-\d\d)(?:[t ](\d\d:\d\d(?::\d\d(?:\.\d{1,9})?)?))?(z|[+-]\d\d:?\d\d)?$/.exec(value);
    if (!iso) return undefined;
    const [, date, time = '00:00', offset] = iso;
    const ms = Date.parse(`${date}T${time}${offset ?? (zone === 'utc' ? 'Z' : '')}`);
    return Number.isNaN(ms) ? undefined : ms;
  }

  /** An instant as `YYYY-MM-DD HH:MM:SS` in `zone`; an unreadable one reads as such. */
  export function zonedText(ms: number, zone: Zone): string {
    if (Number.isNaN(ms)) return 'invalid time';
    const date = new Date(zone === 'utc' ? ms : ms - new Date(ms).getTimezoneOffset() * 60_000);
    return date.toISOString().slice(0, 19).replace('T', ' ');
  }

  /** How a committed range reads on the trigger and in Recent. */
  export function rangeLabel(range: PickedRange, zone: Zone): string {
    if ('last' in range) return presetLabel(range.last);
    const [from, to] = [zonedText(Date.parse(range.from), zone), zonedText(Date.parse(range.to), zone)];
    return from.slice(0, 10) === to.slice(0, 10) ? `${from.slice(11)} → ${to.slice(11)}` : `${from.slice(0, 16)} → ${to.slice(0, 16)}`;
  }
</script>

<script lang="ts">
  let {
    range,
    zone,
    presets,
    onapply
  }: {
    range: PickedRange;
    zone: Zone;
    presets: readonly string[];
    onapply: (range: PickedRange, zone: Zone) => void;
  } = $props();

  const RECENT_KEY = 'wyrd.observe.recent-ranges';
  const RECENT_MAX = 5;

  let open = $state(false);
  let from = $state('');
  let to = $state('');
  let draftZone = $state<Zone>('utc');
  let problem = $state('');
  let recent = $state.raw<readonly PickedRange[]>([]);
  let trigger = $state<HTMLButtonElement>();
  let root = $state<HTMLDivElement>();

  /** Recently committed ranges; browser storage may be unavailable, so it fails soft. */
  function readRecent(): PickedRange[] {
    try {
      const stored = JSON.parse(localStorage.getItem(RECENT_KEY) ?? '[]');
      return Array.isArray(stored) ? stored.slice(0, RECENT_MAX) : [];
    } catch {
      return [];
    }
  }
  // Every committed range, however it was applied, joins Recent.
  $effect(() => {
    const committed = JSON.stringify(range);
    const next = [range, ...readRecent().filter((each) => JSON.stringify(each) !== committed)].slice(0, RECENT_MAX);
    recent = next.slice(1);
    try {
      localStorage.setItem(RECENT_KEY, JSON.stringify(next));
    } catch {
      // Recent is a convenience; the applied range lives in the URL.
    }
  });

  function show() {
    draftZone = zone;
    problem = '';
    if ('last' in range) {
      from = `now-${range.last}`;
      to = 'now';
    } else {
      from = zonedText(Date.parse(range.from), zone);
      to = zonedText(Date.parse(range.to), zone);
    }
    open = true;
  }
  function hide() {
    open = false;
    trigger?.focus();
  }
  function commit(next: PickedRange, nextZone: Zone = zone) {
    open = false;
    onapply(next, nextZone);
  }
  function custom(event: SubmitEvent) {
    event.preventDefault();
    const now = Date.now();
    const start = readInstant(from, draftZone, now);
    const end = readInstant(to, draftZone, now);
    if (start === undefined) problem = 'From is not now, now-2h, yesterday or an ISO time.';
    else if (end === undefined) problem = 'To is not now, now-2h, yesterday or an ISO time.';
    else if (start >= end) problem = 'From must precede To.';
    else commit({ from: new Date(start).toISOString(), to: new Date(end).toISOString() }, draftZone);
  }
  function outside(event: PointerEvent) {
    if (open && root && !root.contains(event.target as Node)) open = false;
  }
</script>

<svelte:window onpointerdown={outside} />

<div class="picker" bind:this={root}>
  <button
    bind:this={trigger}
    type="button"
    class="trigger"
    aria-haspopup="dialog"
    aria-expanded={open}
    aria-label="Time range: {rangeLabel(range, zone)}{zone === 'utc' ? ' UTC' : ' local'}"
    onclick={() => (open ? hide() : show())}
  >
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><rect x="3" y="5" width="18" height="16"></rect><path d="M3 10h18M8 3v4M16 3v4"></path></svg>
    <span>{rangeLabel(range, zone)}</span>
    <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><path d={open ? 'M18 15l-6-6-6 6' : 'M6 9l6 6 6-6'}></path></svg>
  </button>
  {#if open}
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <div class="dialog" role="dialog" aria-label="Time range" tabindex="-1" onkeydown={(event) => event.key === 'Escape' && hide()}>
      <div class="relative">
        <p class="h">Relative</p>
        {#each presets as preset (preset)}
          {@const current = 'last' in range && range.last === preset}
          <button type="button" class="p" aria-current={current ? 'true' : undefined} onclick={() => commit({ last: preset })}>{presetLabel(preset)}{current ? ' ✓' : ''}</button>
        {/each}
      </div>
      <form class="custom" onsubmit={custom} novalidate>
        <p class="h">Custom range</p>
        <label>From<input class="in" bind:value={from} aria-invalid={problem.startsWith('From') || undefined} autocomplete="off" spellcheck="false" /></label>
        <label>To<input class="in" bind:value={to} aria-invalid={problem.startsWith('To') || undefined} autocomplete="off" spellcheck="false" /></label>
        <p class="note">Also accepts now-2h, yesterday, an ISO time.</p>
        <div class="zone" role="group" aria-label="Time zone">
          <button type="button" aria-pressed={draftZone === 'utc'} onclick={() => (draftZone = 'utc')}>UTC</button>
          <button type="button" aria-pressed={draftZone === 'local'} onclick={() => (draftZone = 'local')}>Local</button>
        </div>
        {#if problem}<p class="problem" role="alert">{problem}</p>{/if}
        <button type="submit" class="apply">Apply range</button>
        {#if recent.length}
          <p class="h recent">Recent</p>
          {#each recent as each (JSON.stringify(each))}
            <button type="button" class="p past" onclick={() => commit(each)}>{rangeLabel(each, zone)}</button>
          {/each}
        {/if}
      </form>
    </div>
  {/if}
</div>

<style>
  .picker {
    position: relative;
  }
  .trigger {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    min-width: 176px;
    height: 34px;
    box-sizing: border-box;
    padding: 0 10px;
    border: 1px solid var(--border);
    border-radius: var(--r);
    background: var(--surface);
    color: var(--text);
    font: 13px var(--font-sans);
    cursor: pointer;
  }
  .trigger[aria-expanded='true'] {
    border-color: var(--declare);
    background: var(--declare-soft);
  }
  .dialog {
    position: absolute;
    top: calc(100% + 6px);
    right: 0;
    z-index: 30;
    display: grid;
    grid-template-columns: 180px minmax(0, 1fr);
    width: min(472px, calc(100vw - 32px));
    box-sizing: border-box;
    border: 1px solid var(--border);
    border-radius: var(--r);
    background: var(--surface);
    box-shadow: 0 12px 32px #00000040;
  }
  .relative {
    display: flex;
    flex-direction: column;
    padding: 10px;
    border-right: 1px solid var(--border);
  }
  .custom {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-width: 0;
    padding: 14px;
  }
  .h {
    margin: 0;
    color: var(--muted);
    font: 11px var(--font-mono);
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }
  .relative .h {
    padding: 4px 10px 8px;
  }
  .recent {
    margin-top: 6px;
  }
  .p {
    display: block;
    padding: 7px 10px;
    border: 0;
    border-radius: var(--r);
    background: transparent;
    color: var(--text);
    font: 12px var(--font-mono);
    text-align: left;
    cursor: pointer;
  }
  .p:hover,
  .p:focus-visible,
  .p[aria-current='true'] {
    background: var(--declare-soft);
  }
  .past {
    padding-left: 0;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 4px;
    color: var(--muted);
    font: 11px var(--font-mono);
  }
  .in {
    width: 100%;
    height: 32px;
    box-sizing: border-box;
    padding: 0 8px;
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--text);
    font: 12px var(--font-mono);
  }
  .in[aria-invalid='true'] {
    border-color: var(--danger);
  }
  .note,
  .problem {
    margin: 0;
    color: var(--muted);
    font: 11px var(--font-mono);
  }
  .problem {
    color: var(--danger);
  }
  .zone {
    display: inline-flex;
  }
  .zone button {
    height: 28px;
    padding: 0 12px;
    border: 1px solid var(--border);
    background: var(--surface);
    color: var(--muted);
    font: 12px var(--font-mono);
    cursor: pointer;
  }
  .zone button + button {
    margin-left: -1px;
  }
  .zone button[aria-pressed='true'] {
    border-color: var(--declare);
    background: var(--declare-soft);
    color: var(--text);
  }
  .apply {
    height: 34px;
    border: 0;
    background: var(--declare);
    color: var(--declare-ink);
    font: 600 13px var(--font-sans);
    cursor: pointer;
  }
  @media (max-width: 559px) {
    .dialog {
      grid-template-columns: minmax(0, 1fr);
    }
    .relative {
      border-right: 0;
      border-bottom: 1px solid var(--border);
    }
  }
</style>
