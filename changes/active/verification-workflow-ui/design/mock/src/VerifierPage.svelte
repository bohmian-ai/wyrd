<script>
  import Graph from './Graph.svelte';
  import Inspector from './Inspector.svelte';
  import { DEFS, versionsOf, toYaml } from './catalog.js';
  import { NOW } from './data.js';
  import { ui, execs, RANGES, verifierHref, verifyHref } from './store.svelte.js';

  // The exact Verifier Card: definition only. Choosing another version views it; nothing is activated.
  let { mode } = $props();
  const def = $derived(DEFS[ui.vdef]);
  const versions = $derived(versionsOf(def.name));
  const latest = $derived(versions[0]);
  // The shortest range that holds every recorded execution of this exact version.
  const range = $derived.by(() => {
    const oldest = Math.min(NOW, ...Object.values(execs).filter((x) => x.def === def.key && !x.hidden).map((x) => x.at));
    return Object.keys(RANGES).find((k) => NOW - RANGES[k][1] <= oldest) ?? '14d';
  });
</script>

<div class="exec" data-testid="verifier-page">
  <div class="xhead">
    <div class="xtitle">
      <div class="xname">
        <h1 data-testid="def-name">{def.name}</h1><span class="kt">Verifier</span>
        <select class="vsel" aria-label="Verifier version" data-testid="def-version" value={def.version} onchange={(e) => (location.hash = verifierHref(`${def.name}@${e.currentTarget.value}`, ui.vtab))}>
          {#each versions as v (v)}<option value={v}>{v}{v === latest ? ' · current' : ''}</option>{/each}
        </select>
      </div>
      <p class="desc">{def.description}</p>
      <div class="xmeta" data-testid="def-identity">Verifier/{def.key}</div>
      {#if def.version !== latest}
        <p class="banner info" data-testid="old-version">You are viewing {def.version}. Viewing a version changes nothing: each binding pins the exact version it runs.</p>
      {/if}
    </div>
    <div class="xside">
      <a class="btn btn-primary" data-testid="results-in-verify" href={verifyHref([['verifier', def.key]], range === '24h' ? undefined : range)}>Results in Verify ↗</a>
      <span class="small muted">Executions of exactly {def.key}</span>
    </div>
  </div>
  <div class="vgrid">
    <div class="vmain">
      <div class="segs" role="tablist" aria-label="Definition view">
        <a class="seg" role="tab" data-vtab="graph" aria-selected={ui.vtab === 'graph'} href={verifierHref(def.key)}>Graph</a>
        <a class="seg" role="tab" data-vtab="yaml" aria-selected={ui.vtab === 'yaml'} href={verifierHref(def.key, 'yaml')}>YAML</a>
      </div>
      {#if ui.vtab === 'graph'}
        {#key def.key}<Graph {mode} />{/key}
        <Inspector />
      {:else}
        <section class="panel">
          <pre class="code yaml" data-testid="yaml">{toYaml(def)}</pre>
          <p class="note small" style="padding: 0 14px 12px">Read-only projection. Field spellings are illustrative, not final.</p>
        </section>
      {/if}
    </div>
    <aside class="panel runs-on" aria-labelledby="runs-on-h" data-testid="runs-on">
      <span class="h" id="runs-on-h">Runs on</span>
      {#each def.runsOn as b (b.target)}
        <div class="binding">
          <span class="gn-row"><span class="kt">{b.kind}</span>{#if b.href}<a class="mono small" href={b.href}>{b.target}</a>{:else}<span class="mono small">{b.target}</span>{/if}</span>
          <span class="small muted">{b.when} · {b.state}</span>
        </div>
      {:else}
        <p class="note small">No binding runs this exact version.</p>
      {/each}
    </aside>
  </div>
</div>
