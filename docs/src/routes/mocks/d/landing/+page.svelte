<script lang="ts">
  import { base } from '$app/paths';
  import Frame from '$lib/mocks/Frame.svelte';
  import Plane from '$lib/mocks/Plane.svelte';

  const stages = [
    { n: 'STAGE 01', k: 'client', t: 'QUICKSTART', d: 'Install Wyrd, register your first Card, read it back. ~5 min.', go: 'CD ./START ▸' },
    { n: 'STAGE 02', k: 'rune', t: 'CONCEPTS', d: 'Cards, the envelope, Specs, Runs, Observations, the graph.', go: 'LEARN ▸' },
    { n: 'STAGE 03', k: 'control', t: 'REFERENCE', d: 'Schemas, error codes, CLI verbs, MCP tools — the contract.', go: 'LOOK UP ▸' },
    { n: 'STAGE 04', k: 'server', t: 'FOR AGENTS', d: 'Machine-navigable surfaces, llms.txt, remediation recipes.', go: 'WIRE IN ▸' }
  ];
</script>

<Frame dir="d" product="wyrd">
  <section class="qhero">
    <Plane />
    <div class="inner">
      <div class="q-eye">★ OPEN CORE · TYPED AI SYSTEMS ★</div>
      <h1 class="q-title">WYRD</h1>
      <p class="q-sub">Give every model, prompt, agent &amp; dataset one typed, versioned Card.</p>
      <div class="press">▶ PRESS START</div>
      <div class="q-cta">
        <a class="btn btn-lime" href={`${base}/mocks/d/article/`}>START QUEST</a>
        <a class="btn btn-ghost" href={`${base}/mocks/d/article/`}>CONTINUE</a>
      </div>
      <div class="copyline">© 2026 MITARI · INSERT COIN</div>
    </div>
  </section>

  <section class="sec center">
    <div class="wrap">
      <div class="stage-head">SELECT YOUR <span class="blip">STAGE</span></div>
      <div class="stage-grid">
        {#each stages as s}
          <a class="stage-tile" href={`${base}/mocks/d/article/`}>
            <div class="st-num">{s.n}</div>
            <div class={`st-ico ${s.k}`}></div>
            <div class="st-t">{s.t}</div>
            <p>{s.d}</p>
            <span class="go">{s.go}</span>
          </a>
        {/each}
      </div>
    </div>
  </section>

  <section class="sec center alt">
    <div class="wrap">
      <div class="sec-head">
        <div class="sec-eye">TWO WORLDS</div>
        <h2 class="title">One stack. <em>Two layers.</em></h2>
      </div>
      <div class="worlds">
        <div class="world wyrd">
          <div class="wt">WORLD 1 · WYRD <span class="lvl">[ OPEN CORE ]</span></div>
          <p>The substrate. Build interconnected AI systems — every entity typed, declared, and connected in one registry graph.</p>
          <div class="chips"><span>MODEL</span><span>PROMPT</span><span>AGENT</span><span>SERVICE</span><span>DATASET</span></div>
        </div>
        <div class="world locked">
          <div class="wt">WORLD 2 · FATHOM <span class="lvl">[ ENTERPRISE ]</span></div>
          <p>The intelligence layer. Reads the typed graph to verify behavior — leakage, eval, drift vs spec, audit — as evidence.</p>
          <span class="lock">🔒 LOCKED · COMING SOON</span>
        </div>
      </div>
    </div>
  </section>

  <section class="sec center">
    <div class="wrap">
      <div class="sec-head">
        <div class="sec-eye">THE LEVEL PATH</div>
        <h2 class="title">One typed model. <em>Every stage.</em></h2>
      </div>
      <div class="life" style="margin-top:8px">
        <div class="stages">
          <div class="stage"><span class="n">1</span><h4>DEVELOP</h4><p>Build and connect typed AI systems on Wyrd. Traces stream in.</p></div>
          <div class="stage"><span class="n">2</span><h4>DEPLOY</h4><p>Models, services, agents ship as declared Cards with a versioned spec.</p></div>
          <div class="stage"><span class="n">3</span><h4>OBSERVE</h4><p>Runtime behavior is measured against what each entity declared.</p></div>
          <div class="stage"><span class="n">4</span><h4>DIAGNOSE</h4><p>Fathom isolates the failure, proves the cause, returns a fix.</p></div>
        </div>
        <div class="rail">
          <span class="lab"><span class="w">WYRD</span> captures</span>
          <span class="seg"></span>
          <span class="lab"><span class="f">FATHOM</span> verifies</span>
        </div>
      </div>
    </div>
  </section>
</Frame>
