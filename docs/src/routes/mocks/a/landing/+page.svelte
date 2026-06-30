<script lang="ts">
  import { base } from '$app/paths';
  import Frame from '$lib/mocks/Frame.svelte';
  import Plane from '$lib/mocks/Plane.svelte';
</script>

<Frame dir="a" product="wyrd">
  <section class="hero">
    <Plane />
    <div class="hero-in">
      <div>
        <span class="eyebrow">OPEN CORE · TYPED AI SYSTEMS</span>
        <h1>Build AI systems<br />that <span class="hl">explain themselves</span>.</h1>
        <p class="hero-sub">
          <span class="w">Wyrd</span> gives every model, prompt, agent, and dataset one
          typed, versioned record — a Card — and wires them into a single graph.
          <span class="f">Fathom</span> reads that graph to verify how they behave.
        </p>
        <div class="hero-cta">
          <a class="btn btn-lime" href={`${base}/mocks/a/article/`}>▶ Quickstart</a>
          <a class="btn btn-ghost" href={`${base}/mocks/a/hub/`}>Explore concepts</a>
        </div>
      </div>

      <div class="stack">
        <div class="layer fathom">
          <div class="lhd"><span class="nm">Fathom</span><span class="tg">ENTERPRISE</span><span class="dot">verification layer</span></div>
          <p>The intelligence layer that <b>verifies how your systems behave</b> — leakage, eval, drift, runtime — turned into evidence.</p>
          <div class="chips"><span>judgment</span><span>evidence</span><span>drift vs spec</span><span>audit</span></div>
        </div>
        <div class="seam"><span class="ar">▾</span><span>reads the typed model</span><span class="ar">▾</span></div>
        <div class="layer wyrd">
          <div class="lhd"><span class="nm">Wyrd</span><span class="tg">OPEN CORE</span><span class="dot">the substrate</span></div>
          <p>An open framework to build <b>interconnected AI systems</b> — every entity typed, declared, and connected.</p>
          <div class="chips"><span>model</span><span>prompt</span><span>agent</span><span>service</span><span>dataset</span></div>
        </div>
        <div class="base"><span>THE DATA &amp; AI LIFECYCLE</span><span>develop · deploy · observe · diagnose</span></div>
      </div>
    </div>
  </section>

  <section class="sec center">
    <div class="wrap">
      <div class="sec-head">
        <div class="sec-eye">START WHERE YOU ARE</div>
        <h2 class="title">Four ways into the <em>docs</em>.</h2>
      </div>
      <div class="grid3" style="text-align:left;margin-top:8px">
        <a class="card" href={`${base}/mocks/a/article/`}><div class="ct"><span class="d client"></span>Quickstart</div><p>Install Wyrd, register your first Card, and read it back in under five minutes.</p><div class="meta">Start here →</div></a>
        <a class="card" href={`${base}/mocks/a/hub/`}><div class="ct"><span class="d rune"></span>Concepts</div><p>Cards, the shared envelope, Specs, Runs, Observations, and the registry graph.</p><div class="meta">Learn the model →</div></a>
        <a class="card" href={`${base}/mocks/a/reference/`}><div class="ct"><span class="d control"></span>Reference</div><p>Schemas, stable error codes, CLI verbs, and MCP tools — the literal contract.</p><div class="meta">Look it up →</div></a>
        <a class="card" href={`${base}/mocks/a/reference/`}><div class="ct"><span class="d server"></span>For agents</div><p>Machine-navigable surfaces, llms.txt, and error-remediation recipes.</p><div class="meta">Wire an agent →</div></a>
      </div>
    </div>
  </section>

  <section class="sec center alt">
    <div class="wrap">
      <div class="sec-head">
        <div class="sec-eye">ENTIRE LIFECYCLE</div>
        <h2 class="title">One typed model. <em>Every stage.</em></h2>
      </div>
      <div class="life" style="margin-top:8px">
        <div class="stages">
          <div class="stage"><span class="n">1</span><h4>DEVELOP</h4><p>Build and connect typed AI systems on Wyrd. Traces and experiments stream in.</p></div>
          <div class="stage"><span class="n">2</span><h4>DEPLOY</h4><p>Models, services, and agents ship as declared Cards with a versioned spec.</p></div>
          <div class="stage"><span class="n">3</span><h4>OBSERVE</h4><p>Runtime behavior is measured against what each entity declared.</p></div>
          <div class="stage"><span class="n">4</span><h4>DIAGNOSE</h4><p>Fathom isolates the failure, proves the cause, returns a fix as code.</p></div>
        </div>
        <div class="rail">
          <span class="lab"><span class="w">Wyrd</span> captures</span>
          <span class="seg"></span>
          <span class="lab"><span class="f">Fathom</span> verifies</span>
        </div>
      </div>
    </div>
  </section>
</Frame>
