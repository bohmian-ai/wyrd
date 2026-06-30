<script lang="ts">
  import { base } from '$app/paths';
  import Frame from '$lib/mocks/Frame.svelte';
</script>

<Frame dir="c" product="wyrd">
  <section class="term-hero">
    <h1 class="term-h1">WYRD<span class="v">.</span></h1>
    <p class="term-tag">
      An open framework for building typed, interconnected AI systems. Every model, prompt,
      agent, and dataset becomes a versioned Card the registry can reason about.
    </p>

    <div class="term-card">
      <div class="term-bar"><i class="r"></i><i class="y"></i><i class="g"></i><span class="ttl">bash — wyrd@registry.prod</span></div>
      <div class="term-body">
        <div><span class="pr">$</span> <span class="cmd">wyrd cards register ./cards/model.yaml --space prod</span></div>
        <div><span class="out">→ resolving spec hash …</span></div>
        <div><span class="ok">✓ registered</span> <span class="ref">model/churn-classifier@1.4.0</span> <span class="out">(control)</span></div>
        <div><span class="out">→ derived 3 relationships · 0 conflicts</span></div>
        <div><span class="pr">$</span> <span class="cmd cursor"> </span></div>
      </div>
    </div>

    <div class="term-cta" style="margin-top:30px">
      <a class="btn btn-lime" href={`${base}/mocks/c/article/`}>▶ Quickstart</a>
      <a class="btn btn-ghost" href={`${base}/mocks/c/article/`}>Read the docs</a>
    </div>
  </section>

  <section class="sec">
    <div class="wrap">
      <div class="sec-head" style="text-align:left">
        <div class="sec-eye">ENTRY POINTS</div>
        <h2 class="title">pick a path</h2>
      </div>
      <div class="grid3" style="text-align:left;margin-top:18px">
        <a class="card" href={`${base}/mocks/c/article/`}><div class="ct">quickstart</div><p>Install, register your first Card, read it back in five minutes.</p><div class="meta">cd ./start →</div></a>
        <a class="card" href={`${base}/mocks/c/article/`}><div class="ct">concepts</div><p>Cards, the envelope, Specs, Runs, Observations, the graph.</p><div class="meta">man concepts →</div></a>
        <a class="card" href={`${base}/mocks/c/article/`}><div class="ct">reference</div><p>Schemas, error codes, CLI verbs, MCP tools — the contract.</p><div class="meta">grep refs →</div></a>
        <a class="card" href={`${base}/mocks/c/article/`}><div class="ct">for agents</div><p>Machine-navigable surfaces, llms.txt, remediation recipes.</p><div class="meta">curl llms.txt →</div></a>
      </div>
    </div>
  </section>
</Frame>
