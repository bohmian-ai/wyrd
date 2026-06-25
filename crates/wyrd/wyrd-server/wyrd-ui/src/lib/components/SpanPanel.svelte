<script lang="ts">
  import Drawer from './Drawer.svelte';
  import Button from './Button.svelte';
  import CodeBlock from './CodeBlock.svelte';

  type Kind = 'agent' | 'llm' | 'tool' | 'retrieval';
  type KV = { label: string; value: string; link?: boolean };
  type Ev = { label: string; at: string };

  let {
    name,
    kind,
    status = 'ok',
    summary,
    position,
    attributes,
    tokens,
    events,
    outputPreview,
    onclose
  }: {
    name: string;
    kind: Kind;
    status?: 'ok' | 'err';
    summary?: string;
    position?: { left: number; width: number };
    attributes: KV[];
    tokens?: { input: string; output: string; cache: string };
    events?: Ev[];
    outputPreview?: string;
    onclose?: () => void;
  } = $props();

  const kindVar: Record<Kind, string> = {
    agent: 'var(--client-bar)',
    llm: 'var(--rune-strong)',
    tool: 'var(--server-bar)',
    retrieval: 'var(--control-bar)'
  };

  const accent = $derived(status === 'err' ? 'var(--danger)' : kindVar[kind]);
</script>

<Drawer title={name} {accent} chip={kind} {onclose}>
  <div class="d-sec">
    <div class="d-row">
      <span class="d-dot" style={`background:${status === 'err' ? 'var(--danger)' : 'var(--ok)'}`}></span>
      <span class="state">{status}</span>
      {#if summary}<span class="sub">{summary}</span>{/if}
    </div>
    {#if position}<div class="d-pos"><i style={`left:${position.left}%;width:${position.width}%`}></i></div>{/if}
  </div>

  <div class="d-sec">
    <div class="d-sl">attributes</div>
    <dl class="d-kv">
      {#each attributes as a (a.label)}
        <dt>{a.label}</dt>
        <dd>{#if a.link}<span class="lnk">{a.value}</span>{:else}{a.value}{/if}</dd>
      {/each}
    </dl>
  </div>

  {#if tokens}
    <div class="d-sec">
      <div class="d-sl">tokens</div>
      <div class="d-mini">
        <div class="d-mstat"><div class="k">input</div><div class="n">{tokens.input}</div></div>
        <div class="d-mstat"><div class="k">output</div><div class="n">{tokens.output}</div></div>
        <div class="d-mstat"><div class="k">cache</div><div class="n">{tokens.cache}</div></div>
      </div>
    </div>
  {/if}

  {#if events && events.length}
    <div class="d-sec">
      <div class="d-sl">events</div>
      {#each events as e (e.label)}
        <div class="d-ev">{e.label}<span class="t">{e.at}</span></div>
      {/each}
    </div>
  {/if}

  {#if outputPreview}
    <div class="d-sec">
      <div class="d-sl">output preview</div>
      <CodeBlock code={outputPreview} copy={false} />
    </div>
  {/if}

  {#snippet actions()}
    <Button variant="rune">open in trace</Button>
    <Button variant="ghost">copy span_id</Button>
  {/snippet}
</Drawer>

<style>
  .state {
    font-family: var(--fm);
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
  }
  .sub {
    font-family: var(--fm);
    font-size: 11px;
    color: var(--muted);
  }
</style>
