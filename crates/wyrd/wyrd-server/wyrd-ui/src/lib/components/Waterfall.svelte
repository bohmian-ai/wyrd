<script lang="ts">
  type Kind = 'agent' | 'llm' | 'tool' | 'retrieval';
  type Status = 'ok' | 'warn' | 'err';

  type Span = {
    depth: number;
    name: string;
    kind: Kind;
    start: number;
    duration: number;
    status: Status;
  };

  let { spans, total, onselect }: { spans: Span[]; total?: number; onselect?: (name: string) => void } = $props();

  const kindVar: Record<Kind, string> = {
    agent: 'var(--client-bar)',
    llm: 'var(--rune-strong)',
    tool: 'var(--server-bar)',
    retrieval: 'var(--control-bar)'
  };

  const span = $derived(total ?? Math.max(...spans.map((s) => s.start + s.duration), 1));

  const rows = $derived(
    spans.map((s) => {
      const kc = kindVar[s.kind];
      const barColor = s.status === 'err' ? 'var(--danger)' : kc;
      return {
        ...s,
        kindColor: kc,
        barColor,
        dotColor: s.status === 'err' ? 'var(--danger)' : 'var(--ok)',
        left: ((s.start / span) * 100).toFixed(1),
        width: Math.max(0.7, (s.duration / span) * 100).toFixed(1)
      };
    })
  );

  function key(e: KeyboardEvent, cb: () => void): void {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      cb();
    }
  }

  const ticks = [0, 25, 50, 75, 100];
  const tickLabels = $derived(
    [0, 25, 50, 75].map((p) => ({ p, label: `${Math.round((span * p) / 100)}ms` }))
  );
</script>

<div class="wy-wfscroll">
<div class="wy-wf">
  <div class="wfhead">
    <div class="lbl">span · kind</div>
    <div class="ruler">
      {#each ticks as p}<span class="tick" style={`left:${p}%`}></span>{/each}
      {#each tickLabels as t}<span class="tlab" style={`left:${t.p}%`}>{t.label}</span>{/each}
    </div>
  </div>
  {#each rows as s (s.name)}
    <div
      class="srow"
      role="button"
      tabindex="0"
      onclick={() => onselect?.(s.name)}
      onkeydown={(e) => key(e, () => onselect?.(s.name))}
    >
      <div class="sname" style={`padding-left:${s.depth * 15 + 8}px`}>
        <span class="kd" style={`background:${s.kindColor}`}></span>
        <span class="nm">{s.name}</span>
        <span class="dot" style={`background:${s.dotColor}`}></span>
      </div>
      <div class="track">
        <span
          class="span"
          style={`left:${s.left}%;width:${s.width}%;border-color:${s.barColor};background:color-mix(in srgb,${s.barColor} 24%,var(--surface))`}
          >{s.duration}ms</span
        >
      </div>
    </div>
  {/each}
</div>
</div>

<style>
  .wy-wfscroll {
    overflow-x: auto;
  }
  .wy-wf {
    font-family: var(--fm);
    font-size: 10.5px;
    min-width: 380px;
  }
  .wfhead {
    display: grid;
    grid-template-columns: 180px 1fr;
    border-bottom: 2px solid var(--border);
  }
  .lbl {
    padding: 6px 9px;
    font-size: 8.5px;
    font-weight: 700;
    letter-spacing: 0.5px;
    text-transform: uppercase;
    color: var(--muted);
  }
  .ruler {
    position: relative;
    height: 22px;
    border-left: 2px solid var(--border);
  }
  .tick {
    position: absolute;
    top: 0;
    bottom: 0;
    border-left: 1px dashed var(--border);
    opacity: 0.45;
  }
  .tlab {
    position: absolute;
    top: 5px;
    font-size: 8px;
    color: var(--muted);
    padding-left: 4px;
  }
  .srow {
    display: grid;
    grid-template-columns: 180px 1fr;
    align-items: center;
    border-bottom: 2px solid var(--border);
    cursor: pointer;
  }
  .srow:last-child {
    border-bottom: 0;
  }
  .srow:hover {
    background: var(--surface-2);
  }
  .sname {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 6px 8px;
    white-space: nowrap;
    overflow: hidden;
  }
  .kd {
    width: 3px;
    height: 13px;
    border-radius: 2px;
    flex: 0 0 auto;
  }
  .nm {
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--text);
  }
  .dot {
    margin-left: auto;
    width: 7px;
    height: 7px;
    border-radius: 50%;
    flex: 0 0 auto;
  }
  .track {
    position: relative;
    height: 26px;
    border-left: 2px solid var(--border);
  }
  .span {
    position: absolute;
    top: 5px;
    height: 16px;
    border-radius: 3px;
    border: 2px solid;
    display: flex;
    align-items: center;
    padding: 0 5px;
    font-size: 8px;
    font-weight: 700;
    color: var(--text);
    overflow: hidden;
    min-width: 4px;
  }
</style>
