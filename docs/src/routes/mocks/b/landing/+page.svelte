<script lang="ts">
  import { base } from '$app/paths';
  import Frame from '$lib/mocks/Frame.svelte';
</script>

<Frame dir="b" product="wyrd">
  <section class="wb-hero">
    <div class="wb-in">
      <div>
        <span class="eyebrow">OPEN CORE · TYPED AI SYSTEMS</span>
        <h1>The typed record for every <span class="hl">model, prompt &amp; agent</span> you ship.</h1>
        <p class="wb-sub">
          <b>Wyrd</b> gives every entity in your AI stack one versioned Card and wires them into
          a single registry graph. <b>Fathom</b> reads that graph to verify how they behave.
        </p>
        <div class="hero-cta">
          <a class="btn btn-lime" href={`${base}/mocks/b/article/`}>Read the quickstart</a>
          <a class="btn btn-ghost" href={`${base}/mocks/b/article/`}>Browse concepts</a>
        </div>
      </div>

      <div class="wb-panel">
        <div class="pt">registry · prod<span class="badge">6 cards</span></div>
        <div class="wb-row head"><span>card</span><span>version</span><span>kind</span></div>
        <div class="wb-row"><span class="ref">churn-classifier</span><span>1.4.0</span><span class="kind control">model</span></div>
        <div class="wb-row"><span class="ref">support-triage</span><span>2.1.0</span><span class="kind server">agent</span></div>
        <div class="wb-row"><span class="ref">refund-policy</span><span>0.9.2</span><span class="kind client">prompt</span></div>
        <div class="wb-row"><span class="ref">orders-features</span><span>3.0.0</span><span class="kind control">dataset</span></div>
        <div class="wb-row"><span class="ref">scoring-svc</span><span>1.0.0</span><span class="kind server">service</span></div>
        <div class="wb-row"><span class="ref">eval-2026-q2</span><span>1.0.0</span><span class="kind client">artifact</span></div>
      </div>
    </div>
  </section>

  <section class="sec">
    <div class="wrap">
      <div class="sec-head" style="text-align:left">
        <div class="sec-eye">START WHERE YOU ARE</div>
        <h2 class="title">Four ways into the docs.</h2>
      </div>
      <div class="grid3" style="text-align:left;margin-top:18px">
        <a class="card" href={`${base}/mocks/b/article/`}><div class="ct"><span class="d client"></span>Quickstart</div><p>Install Wyrd, register your first Card, read it back in five minutes.</p><div class="meta">Start here →</div></a>
        <a class="card" href={`${base}/mocks/b/article/`}><div class="ct"><span class="d rune"></span>Concepts</div><p>Cards, the envelope, Specs, Runs, Observations, and the registry graph.</p><div class="meta">Learn the model →</div></a>
        <a class="card" href={`${base}/mocks/b/article/`}><div class="ct"><span class="d control"></span>Reference</div><p>Schemas, stable error codes, CLI verbs, and MCP tools — the contract.</p><div class="meta">Look it up →</div></a>
        <a class="card" href={`${base}/mocks/b/article/`}><div class="ct"><span class="d server"></span>For agents</div><p>Machine-navigable surfaces, llms.txt, and error-remediation recipes.</p><div class="meta">Wire an agent →</div></a>
      </div>
    </div>
  </section>
</Frame>
