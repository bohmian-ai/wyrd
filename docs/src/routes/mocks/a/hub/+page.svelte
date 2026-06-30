<script lang="ts">
  import { base } from '$app/paths';
  import Frame from '$lib/mocks/Frame.svelte';
  import Sidebar from '$lib/mocks/Sidebar.svelte';

  const tiles = [
    { k: 'rune', t: 'Cards & the envelope', d: 'The shared shape every artifact carries — apiVersion, metadata, kind, spec, relationships, status.' },
    { k: 'rune', t: 'Specs, Runs, Observations', d: 'The four durable nouns and how a declared spec turns into runtime evidence.' },
    { k: 'control', t: 'Registry & lineage', d: 'How registration mints versions and how the server derives the relationship graph from refs.' },
    { k: 'server', t: 'Spaces & isolation', d: 'Namespaces, tenant boundaries, and how a CardRef resolves inside a space.' },
    { k: 'client', t: 'Idempotency', d: 'Why registering the same spec twice is a safe no-op for flaky agent loops.' },
    { k: 'control', t: 'Versioning', d: 'Content-hash semantics, immutable prior versions, and how pins work.' }
  ];
</script>

<Frame dir="a" product="wyrd">
  <div class="shell">
    <aside class="side"><Sidebar active="ontology" /></aside>
    <main class="main">
      <div class="crumbs">Wyrd <span class="sep">/</span> Concepts</div>
      <div class="sec-head" style="text-align:left;margin-bottom:6px">
        <div class="sec-eye">SECTION INDEX</div>
        <h2 class="title">Concepts</h2>
      </div>
      <p class="lead" style="margin-bottom:26px">
        The model under Wyrd — the nouns, the envelope, and the graph. Start at the top;
        each page builds on the one before it.
      </p>

      <div class="grid3" style="text-align:left">
        {#each tiles as tile}
          <a class="card" href={`${base}/mocks/a/article/`}>
            <div class={`kbar ${tile.k}`}></div>
            <div class="ct"><span class={`d ${tile.k}`}></span>{tile.t}</div>
            <p>{tile.d}</p>
            <div class="meta">Read →</div>
          </a>
        {/each}
      </div>
    </main>
  </div>
</Frame>
