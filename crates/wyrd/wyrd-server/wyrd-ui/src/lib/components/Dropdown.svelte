<script lang="ts">
  type Kind = 'client' | 'server' | 'control';
  type Option = { label: string; value?: string; kind?: Kind };

  let {
    label,
    options,
    value,
    onselect
  }: { label: string; options: Option[]; value?: string; onselect?: (value: string) => void } = $props();

  let open = $state(false);
  let selected = $state<string | undefined>(undefined);
  const current = $derived(selected ?? value);

  function choose(o: Option): void {
    const v = o.value ?? o.label;
    selected = v;
    onselect?.(v);
    open = false;
  }
</script>

<div class="wy-dd">
  <button class="trigger" type="button" aria-expanded={open} onclick={() => (open = !open)}>
    {label}<span class="car">▾</span>
  </button>
  {#if open}
    <div class="menu" role="listbox">
      {#each options as o (o.label)}
        {@const v = o.value ?? o.label}
        <button
          class="opt"
          type="button"
          role="option"
          aria-selected={current === v}
          class:sel={current === v}
          data-k={o.kind}
          onclick={() => choose(o)}
        >
          <span class="tick"></span>{o.label}
          {#if current === v}<span class="ck">✓</span>{/if}
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .wy-dd {
    font-family: var(--fm);
  }
  .trigger {
    display: inline-flex;
    align-items: center;
    gap: 9px;
    font-size: 11px;
    font-weight: 700;
    padding: 7px 11px;
    border: 2px solid var(--border);
    border-radius: var(--r);
    background: var(--surface);
    box-shadow: 3px 3px 0 0 var(--shadow);
    color: var(--text);
    cursor: pointer;
  }
  .car {
    color: var(--muted);
  }
  .menu {
    margin-top: 9px;
    border: 2px solid var(--border);
    border-radius: var(--r);
    background: var(--surface);
    box-shadow: 3px 3px 0 0 var(--shadow);
    overflow: hidden;
    width: 210px;
  }
  .opt {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    text-align: left;
    padding: 7px 11px;
    font-family: var(--fm);
    font-size: 11px;
    border: 0;
    border-bottom: 2px solid var(--border);
    background: var(--surface);
    color: var(--text);
    cursor: pointer;
  }
  .opt:last-child {
    border-bottom: 0;
  }
  .opt:hover {
    background: var(--surface-2);
  }
  .opt.sel {
    background: var(--rune-soft);
    color: var(--rune-strong);
    font-weight: 700;
  }
  .ck {
    margin-left: auto;
    color: var(--rune-strong);
    font-weight: 700;
  }
  .tick {
    width: 10px;
    height: 10px;
    border-radius: 2px;
    border: 2px solid var(--border);
    background: var(--surface);
    flex: 0 0 auto;
  }
  .opt[data-k='client'] .tick {
    background: var(--client-bar);
  }
  .opt[data-k='server'] .tick {
    background: var(--server-bar);
  }
  .opt[data-k='control'] .tick {
    background: var(--control-bar);
  }
</style>
