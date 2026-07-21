<script lang="ts">
  // Full-width Direction A landing: arcade hero with the Plane backdrop and the
  // two-pillar Fathom/Wyrd stack, the "four ways into the docs" tile grid, and
  // the lifecycle stages. The layout renders `/` full-width (no doc shell), so
  // these sections sit directly inside the `.mk a` cabinet chrome and are styled
  // entirely by the global arcade theme (arcade.css).
  import { base } from '$app/paths';
  import Plane from '$lib/components/Plane.svelte';
</script>

<svelte:head>
  <title>Mitari docs — Build AI systems that explain themselves</title>
  <meta name="description" content="Developer and agent documentation for Wyrd and Fathom." />
</svelte:head>

<section class="hero">
  <Plane />
  <div class="hero-in">
    <div>
      <span class="eyebrow">OPEN CORE · TYPED AI SYSTEMS</span>
      <h1>Build AI systems<br />that <span class="hl">explain themselves</span>.</h1>
      <p class="hero-sub">
        <span class="w">Wyrd</span> gives every model, prompt, agent, and dataset one typed,
        versioned record — a Card — and wires them into a single graph.
        <span class="f">Fathom</span> reads that graph to verify how they behave.
      </p>
      <div class="hero-cta">
        <a class="btn btn-lime" href={`${base}/get-started/`}>▶ Get started</a>
        <a class="btn btn-ghost" href={`${base}/tutorials/`}>Learn Wyrd</a>
      </div>
    </div>

    <div class="stack">
      <div class="layer fathom">
        <div class="lhd">
          <span class="nm">Fathom</span><span class="tg">ENTERPRISE</span><span class="dot"
            >verification layer</span
          >
        </div>
        <p>
          The intelligence layer that <b>verifies how your systems behave</b> — leakage, eval,
          drift, runtime — turned into evidence.
        </p>
        <div class="chips"><span>judgment</span><span>evidence</span><span>drift vs spec</span><span>audit</span></div>
      </div>
      <div class="seam"><span class="ar">▾</span><span>reads the typed model</span><span class="ar">▾</span></div>
      <div class="layer wyrd">
        <div class="lhd">
          <span class="nm">Wyrd</span><span class="tg">OPEN CORE</span><span class="dot"
            >the substrate</span
          >
        </div>
        <p>
          An open framework to build <b>interconnected AI systems</b> — every entity typed,
          declared, and connected.
        </p>
        <div class="chips"><span>model</span><span>prompt</span><span>agent</span><span>service</span><span>dataset</span></div>
      </div>
      <div class="base"><span>THE DATA &amp; AI LIFECYCLE</span><span>develop · deploy · observe · diagnose</span></div>
    </div>
  </div>
</section>

<section class="sec center">
  <div class="wrap">
    <div class="sec-head">
      <div class="sec-eye">FIND THE RIGHT PATH</div>
      <h2 class="title">Start with the question <em>you have</em>.</h2>
    </div>
    <div class="grid3" style="text-align:left;margin-top:8px">
      <a class="card" href={`${base}/get-started/`}>
        <div class="ct"><span class="d client"></span>Start here</div>
        <p>Install Wyrd and run the first local DataCard workflow.</p>
        <div class="meta">Start here →</div>
      </a>
      <a class="card" href={`${base}/tutorials/`}>
        <div class="ct"><span class="d rune"></span>Learn Wyrd</div>
        <p>Follow tutorials, then learn the Card, Spec, Run, and Observation model.</p>
        <div class="meta">Learn the model →</div>
      </a>
      <a class="card" href={`${base}/how-to/`}>
        <div class="ct"><span class="d control"></span>Build with Wyrd</div>
        <p>Declare Cards, build agents and workflows, and work through capability recipes.</p>
        <div class="meta">Build something →</div>
      </a>
      <a class="card" href={`${base}/products/`}>
        <div class="ct"><span class="d server"></span>Products and components</div>
        <p>Find the right component: Wyrd core, Cards, Bifrost, Skald, or Fathom.</p>
        <div class="meta">Choose a component →</div>
      </a>
      <a class="card" href={`${base}/reference/`}>
        <div class="ct"><span class="d control"></span>Reference</div>
        <p>Look up schemas, stable error codes, CLI verbs, and API contracts.</p>
        <div class="meta">Look it up →</div>
      </a>
      <a class="card" href={`${base}/for-agents/`}>
        <div class="ct"><span class="d server"></span>For agents</div>
        <p>Use stable paths, machine-readable indexes, and error-remediation recipes.</p>
        <div class="meta">Wire an agent →</div>
      </a>
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
