<script lang="ts">
  type Kind = 'agent' | 'llm' | 'tool' | 'retrieval';
  type Status = 'ok' | 'warn' | 'err';
  type ScoreTone = 'hi' | 'mid' | 'lo';

  type TraceRow = {
    id: string;
    op: string;
    kind: Kind;
    spans: number;
    durationMs: number;
    tokens?: string;
    cost?: string;
    score?: string;
    scoreTone?: ScoreTone;
    status: Status;
  };

  let {
    rows,
    selectedId,
    onselect
  }: { rows: TraceRow[]; selectedId?: string; onselect?: (id: string) => void } = $props();

  const maxDur = $derived(Math.max(...rows.map((r) => r.durationMs), 1));

  function fmt(ms: number): string {
    return ms < 1000 ? `${ms}ms` : `${(ms / 1000).toFixed(2)}s`;
  }

  function key(e: KeyboardEvent, id: string): void {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      onselect?.(id);
    }
  }
</script>

<div class="wy-tracetable">
  <table>
    <thead>
      <tr>
        <th>trace</th>
        <th>root op</th>
        <th class="r">spans</th>
        <th>duration</th>
        <th class="r">tokens</th>
        <th class="r">cost</th>
        <th class="r">score</th>
        <th>status</th>
      </tr>
    </thead>
    <tbody>
      {#each rows as t (t.id)}
        <tr
          class:sel={t.id === selectedId}
          role="button"
          tabindex="0"
          onclick={() => onselect?.(t.id)}
          onkeydown={(e) => key(e, t.id)}
        >
          <td class="id">{t.id}</td>
          <td><span class="op" data-k={t.kind}>{t.op}</span></td>
          <td class="r">{t.spans}</td>
          <td>
            <span class="mini" style={`width:${((t.durationMs / maxDur) * 60).toFixed(1)}px`}></span>{fmt(t.durationMs)}
          </td>
          <td class="r">{t.tokens ?? '—'}</td>
          <td class="r">{t.cost ?? '—'}</td>
          <td class="r"><span class="sc" data-tone={t.scoreTone}>{t.score ?? '—'}</span></td>
          <td><span class="st" data-st={t.status}>{t.status}</span></td>
        </tr>
      {/each}
    </tbody>
  </table>
</div>

<style>
  .wy-tracetable {
    border: 2px solid var(--border);
    border-radius: var(--r);
    background: var(--surface);
    box-shadow: 3px 3px 0 0 var(--shadow);
    overflow: hidden;
  }
  table {
    width: 100%;
    border-collapse: separate;
    border-spacing: 0;
    font-family: var(--fm);
    font-size: 11px;
  }
  th {
    text-align: left;
    font-size: 8.5px;
    letter-spacing: 0.5px;
    text-transform: uppercase;
    color: var(--muted);
    font-weight: 700;
    padding: 7px 9px;
    border-bottom: 2px solid var(--border);
    background: var(--surface);
  }
  th.r,
  td.r {
    text-align: right;
  }
  td {
    padding: 7px 9px;
    border-bottom: 2px solid var(--border);
    color: var(--text);
  }
  tbody tr:last-child td {
    border-bottom: 0;
  }
  tbody tr:hover {
    background: var(--surface-2);
  }
  tr.sel td {
    background: var(--rune-soft);
  }
  .id {
    font-weight: 700;
  }
  tr.sel .id {
    color: var(--rune-strong);
  }
  tr.sel .id::before {
    content: '▸ ';
    color: var(--rune-strong);
  }
  .op {
    display: inline-block;
    padding-left: 7px;
    border-left: 3px solid var(--muted);
  }
  .op[data-k='agent'] {
    border-left-color: var(--client-bar);
  }
  .op[data-k='llm'] {
    border-left-color: var(--rune-strong);
  }
  .op[data-k='tool'] {
    border-left-color: var(--server-bar);
  }
  .op[data-k='retrieval'] {
    border-left-color: var(--control-bar);
  }
  .mini {
    display: inline-block;
    height: 9px;
    border: 2px solid var(--border);
    background: var(--client-bar);
    border-radius: 2px;
    vertical-align: middle;
    margin-right: 6px;
  }
  .st {
    font-size: 8.5px;
    font-weight: 700;
    letter-spacing: 0.4px;
    text-transform: uppercase;
  }
  .st[data-st='ok'] {
    color: var(--ok);
  }
  .st[data-st='warn'] {
    color: var(--warn);
  }
  .st[data-st='err'] {
    color: var(--danger);
  }
  .sc {
    font-weight: 700;
  }
  .sc[data-tone='hi'] {
    color: var(--ok);
  }
  .sc[data-tone='mid'] {
    color: var(--text);
  }
  .sc[data-tone='lo'] {
    color: var(--warn);
  }
</style>
