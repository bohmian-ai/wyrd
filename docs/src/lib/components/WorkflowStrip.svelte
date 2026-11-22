<script lang="ts">
  import { BeatLoop } from '$lib/beat-loop.svelte';

  // The landing page's workflow strip: four things people build on the one
  // verification engine, driven by one shared loop so they move together.
  // Grey dashed frames are the application's steps and turn solid when they
  // run; amber is agent work, lime recorded evidence, periwinkle the Verifier.
  const loop = new BeatLoop([280, 280, 280, 280, 280, 280, 1900]);
  const at = loop.at;
  let panels: HTMLElement;

  $effect(() => loop.attach(panels));
  $effect(() => loop.tick());
</script>

<div class="panels" class:halt={!loop.running} bind:this={panels}>
  <article class="panel">
      <h3>Real-time verification</h3>
      <p>Your API asks, Wyrd answers, your code decides.</p>
      <svg viewBox="0 0 270 196" role="img" aria-label="A request reaches your API. Your API calls the Wyrd Verifier and gets a passed verdict back, then your API releases the response.">
        <path class="edge" d="M77 39 H97 M173 39 H193" />
        <path class="edge w" d="M135 54 V112" />

        <rect class="frame" x="1" y="24" width="76" height="30" rx="2" />
        <rect class="hot" x="1" y="24" width="76" height="30" rx="2" data-b class:on={at('0-6')} />
        <text class="t mid" x="39" y="43">Request</text>

        <rect class="frame" x="97" y="24" width="76" height="30" rx="2" />
        <rect class="hot" x="97" y="24" width="76" height="30" rx="2" data-b class:on={at('0-6')} />
        <text class="t mid" x="135" y="43">Your API</text>

        <rect class="frame" x="193" y="24" width="76" height="30" rx="2" />
        <rect class="hot ok" x="193" y="24" width="76" height="30" rx="2" data-b class:on={at('4-6')} />
        <text class="t mid" x="231" y="43">Release</text>

        <text class="m out" x="143" y="88" data-b class:on={at('3-6')}>✓ passed</text>

        <rect class="frame wyrd" x="45" y="112" width="180" height="52" rx="2" />
        <text class="t mid" x="135" y="133">Wyrd · Verifier</text>
        <rect class="fill" x="75" y="143" width="120" height="7" rx="2" data-b class:on={at('1-6')} style="transition-duration:.5s,.05s" />
        <rect class="slot" x="75" y="143" width="120" height="7" rx="2" />
        <text class="mark" x="201" y="151" data-b class:on={at('3-6')}>✓</text>

        <circle class="dot" r="3" data-b class:on={at('0')} style="--x0:77px;--y0:39px;--x1:97px;--y1:39px" />
        <circle class="dot w" r="3" data-b class:on={at('1')} style="--x0:135px;--y0:54px;--x1:135px;--y1:112px" />
        <circle class="dot ok" r="3" data-b class:on={at('3')} style="--x0:135px;--y0:112px;--x1:135px;--y1:54px" />
        <circle class="dot ok" r="3" data-b class:on={at('4')} style="--x0:173px;--y0:39px;--x1:193px;--y1:39px" />
      </svg>
    </article>
    <article class="panel">
      <h3>Agent evaluation</h3>
      <p>Your agent runs its steps. A Verifier judges the recorded run.</p>
      <svg viewBox="0 0 270 196" role="img" aria-label="Your agent runs three steps in order: plan, retrieve, compose. Each step is added to a run record. The record is submitted to a Verifier, which judges each of the three steps and returns a passed verdict.">
        <text class="m" x="1" y="12">YOUR AGENT</text>
        <path class="edge a" d="M77 37 H97 M173 37 H193" />
        <path class="tap" d="M39 52 V96 M135 52 V96 M231 52 V96" />
        <path class="edge e" d="M91 122 V150" />
        <path class="edge w" d="M181 170 H199" />

        <rect class="frame" x="1" y="22" width="76" height="30" rx="2" />
        <rect class="hot a" x="1" y="22" width="76" height="30" rx="2" data-b class:on={at('0-6')} />
        <text class="t mid" x="39" y="41">Plan</text>

        <rect class="frame" x="97" y="22" width="76" height="30" rx="2" />
        <rect class="hot a" x="97" y="22" width="76" height="30" rx="2" data-b class:on={at('1-6')} />
        <text class="t mid" x="135" y="41">Retrieve</text>

        <rect class="frame" x="193" y="22" width="76" height="30" rx="2" />
        <rect class="hot a" x="193" y="22" width="76" height="30" rx="2" data-b class:on={at('2-6')} />
        <text class="t mid" x="231" y="41">Compose</text>

        <rect class="frame bif" x="1" y="96" width="268" height="26" rx="2" />
        <text class="t" x="12" y="113">Run record</text>
        <rect class="pip" x="196" y="105" width="8" height="8" data-b class:on={at('1-6')} />
        <rect class="pip" x="212" y="105" width="8" height="8" data-b class:on={at('2-6')} />
        <rect class="pip" x="228" y="105" width="8" height="8" data-b class:on={at('3-6')} />
        <path class="slot" d="M196 105 h8 v8 h-8 z M212 105 h8 v8 h-8 z M228 105 h8 v8 h-8 z" />
        <text class="m" x="99" y="140">submit · queued</text>

        <rect class="frame wyrd" x="1" y="150" width="180" height="40" rx="2" />
        <text class="t" x="13" y="174">Verifier</text>
        <text class="mark" x="118" y="174" data-b class:on={at('4-6')}>✓</text>
        <text class="mark" x="138" y="174" data-b class:on={at('5-6')}>✓</text>
        <text class="mark" x="158" y="174" data-b class:on={at('5-6')} style="transition-delay:.12s">✓</text>

        <rect class="frame" x="199" y="155" width="70" height="30" rx="2" />
        <rect class="hot ok" x="199" y="155" width="70" height="30" rx="2" data-b class:on={at('6')} />
        <text class="t mid" x="228" y="174">Verdict</text>
        <text class="mark" x="253" y="174" data-b class:on={at('6')}>✓</text>

        <circle class="dot a" r="3" data-b class:on={at('1')} style="--x0:77px;--y0:37px;--x1:97px;--y1:37px" />
        <circle class="dot a" r="3" data-b class:on={at('2')} style="--x0:173px;--y0:37px;--x1:193px;--y1:37px" />
        <circle class="dot ev" r="2.5" data-b class:on={at('1')} style="--x0:39px;--y0:52px;--x1:39px;--y1:96px" />
        <circle class="dot ev" r="2.5" data-b class:on={at('2')} style="--x0:135px;--y0:52px;--x1:135px;--y1:96px" />
        <circle class="dot ev" r="2.5" data-b class:on={at('3')} style="--x0:231px;--y0:52px;--x1:231px;--y1:96px" />
        <circle class="dot ev" r="3.5" data-b class:on={at('4')} style="--x0:91px;--y0:122px;--x1:91px;--y1:150px" />
        <circle class="dot ok" r="3" data-b class:on={at('5')} style="--x0:181px;--y0:170px;--x1:199px;--y1:170px" />
      </svg>
    </article>
    <article class="panel">
      <h3>Code review</h3>
      <p>Fan a change out to agent reviewers, then verify the combined result.</p>
      <svg viewBox="0 0 270 196" role="img" aria-label="A change fans out to three agent reviewers working in parallel: correctness, security, and tests. Their three results arrive at different times and fan in to one Verifier, which returns a combined passed verdict.">
        <path class="edge a" d="M31 83 V33 H92 M61 98 H92 M31 113 V163 H92" />
        <path class="edge w" d="M178 33 H238 V60 M178 98 H206 M178 163 H238 V136" />

        <rect class="frame" x="1" y="83" width="60" height="30" rx="2" />
        <rect class="hot" x="1" y="83" width="60" height="30" rx="2" data-b class:on={at('0-6')} />
        <text class="t mid" x="31" y="102">Change</text>

        <rect class="frame" x="92" y="18" width="86" height="30" rx="2" />
        <rect class="hot a" x="92" y="18" width="86" height="30" rx="2" data-b class:on={at('1-6')} />
        <rect class="fill a" x="93" y="44" width="84" height="3" data-b class:on={at('1-6')} style="transition-duration:.3s,.05s" />
        <text class="t mid" x="135" y="37">Correctness</text>

        <rect class="frame" x="92" y="83" width="86" height="30" rx="2" />
        <rect class="hot a" x="92" y="83" width="86" height="30" rx="2" data-b class:on={at('1-6')} />
        <rect class="fill a" x="93" y="109" width="84" height="3" data-b class:on={at('1-6')} style="transition-duration:.8s,.05s" />
        <text class="t mid" x="135" y="102">Security</text>

        <rect class="frame" x="92" y="148" width="86" height="30" rx="2" />
        <rect class="hot a" x="92" y="148" width="86" height="30" rx="2" data-b class:on={at('1-6')} />
        <rect class="fill a" x="93" y="174" width="84" height="3" data-b class:on={at('1-6')} style="transition-duration:.55s,.05s" />
        <text class="t mid" x="135" y="167">Tests</text>

        <rect class="frame wyrd" x="206" y="60" width="63" height="76" rx="2" />
        <text class="t mid" x="237.5" y="82">Verifier</text>
        <rect class="pip ok" x="216" y="92" width="8" height="8" data-b class:on={at('2-6')} />
        <rect class="pip ok" x="233" y="92" width="8" height="8" data-b class:on={at('4-6')} />
        <rect class="pip ok" x="250" y="92" width="8" height="8" data-b class:on={at('3-6')} />
        <path class="slot" d="M216 92 h8 v8 h-8 z M233 92 h8 v8 h-8 z M250 92 h8 v8 h-8 z" />
        <text class="m out mid" x="237.5" y="124" data-b class:on={at('5-6')}>✓ passed</text>

        <circle class="dot a via" r="3" data-b class:on={at('0')} style="--x0:31px;--y0:83px;--xm:31px;--ym:33px;--x1:92px;--y1:33px" />
        <circle class="dot a" r="3" data-b class:on={at('0')} style="--x0:61px;--y0:98px;--x1:92px;--y1:98px" />
        <circle class="dot a via" r="3" data-b class:on={at('0')} style="--x0:31px;--y0:113px;--xm:31px;--ym:163px;--x1:92px;--y1:163px" />
        <circle class="dot ok via" r="3" data-b class:on={at('2')} style="--x0:178px;--y0:33px;--xm:238px;--ym:33px;--x1:238px;--y1:60px" />
        <circle class="dot ok via" r="3" data-b class:on={at('3')} style="--x0:178px;--y0:163px;--xm:238px;--ym:163px;--x1:238px;--y1:136px" />
        <circle class="dot ok" r="3" data-b class:on={at('4')} style="--x0:178px;--y0:98px;--x1:206px;--y1:98px" />
      </svg>
    </article>
    <article class="panel">
      <h3>Session evaluation</h3>
      <p>Turns and events accumulate. Once the session goes quiet, a Verifier judges it whole.</p>
      <svg viewBox="0 0 270 196" role="img" aria-label="Four session events arrive one after another and stack up as session evidence: a user turn, a tool call, an agent turn, and another user turn. When the session goes quiet it is marked ready, the whole group flows through one Verifier, and the Verifier returns a session verdict of passed.">
        <path class="edge e" d="M129 83 H160" />
        <path class="edge w" d="M214 111 V138" />

        <rect class="frame bif" x="1" y="8" width="128" height="150" rx="2" />
        <text class="t" x="12" y="27">Session</text>
        <path class="rule e" d="M1 34 H129" />
        <path class="slot" d="M11 42 h108 v18 h-108 z M11 64 h108 v18 h-108 z M11 86 h108 v18 h-108 z M11 108 h108 v18 h-108 z" />
        <g data-b class:on={at('0-6')}><circle class="pip" cx="20" cy="51" r="3" /><text class="m out" x="30" y="55">user turn</text></g>
        <g data-b class:on={at('1-6')}><circle class="pip" cx="20" cy="73" r="3" /><text class="m out" x="30" y="77">tool call</text></g>
        <g data-b class:on={at('2-6')}><circle class="pip" cx="20" cy="95" r="3" /><text class="m out" x="30" y="99">agent turn</text></g>
        <g data-b class:on={at('3-6')}><circle class="pip" cx="20" cy="117" r="3" /><text class="m out" x="30" y="121">user turn</text></g>
        <text class="m e" x="12" y="147" data-b class:on={at('4-6')}>quiet · ready</text>

        <rect class="frame wyrd" x="160" y="55" width="109" height="56" rx="2" />
        <text class="t mid" x="214.5" y="78">Verifier</text>
        <rect class="fill" x="180" y="90" width="70" height="7" rx="2" data-b class:on={at('5-6')} style="transition-duration:.25s,.05s" />
        <rect class="slot" x="180" y="90" width="70" height="7" rx="2" />

        <rect class="frame" x="160" y="138" width="109" height="30" rx="2" />
        <rect class="hot ok" x="160" y="138" width="109" height="30" rx="2" data-b class:on={at('6')} />
        <text class="t mid" x="208" y="157">Verdict</text>
        <text class="mark" x="240" y="157" data-b class:on={at('6')}>✓</text>

        <circle class="dot ev" r="2.5" data-b class:on={at('0')} style="--x0:20px;--y0:34px;--x1:20px;--y1:51px" />
        <circle class="dot ev" r="2.5" data-b class:on={at('1')} style="--x0:20px;--y0:34px;--x1:20px;--y1:73px" />
        <circle class="dot ev" r="2.5" data-b class:on={at('2')} style="--x0:20px;--y0:34px;--x1:20px;--y1:95px" />
        <circle class="dot ev" r="2.5" data-b class:on={at('3')} style="--x0:20px;--y0:34px;--x1:20px;--y1:117px" />
        <circle class="dot ev" r="4.5" data-b class:on={at('4')} style="--x0:129px;--y0:83px;--x1:160px;--y1:83px" />
        <circle class="dot ok" r="3" data-b class:on={at('5')} style="--x0:214px;--y0:111px;--x1:214px;--y1:138px" />
      </svg>
    </article>
</div>
<div class="controls">
  <span>Illustrative animation</span>
  <button type="button" onclick={loop.toggle}>{loop.playing ? 'Pause' : 'Play'}</button>
</div>

<style>
  .panels {
    --gateway: var(--warn);
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    margin: 32px -20px 0;
  }
  .panel {
    padding: 0 20px;
    border-left: 1px dashed color-mix(in srgb, var(--muted) 60%, transparent);
  }
  .panel:first-child {
    border-left: 0;
  }
  .panel h3 {
    margin: 0;
    font-size: 18px;
    font-weight: 600;
    letter-spacing: -0.01em;
  }
  .panel p {
    min-height: 4.35em;
    margin: 6px 0 14px;
    color: var(--muted);
    font-size: 14px;
    line-height: 1.45;
  }
  svg {
    display: block;
    width: 100%;
    max-width: 340px;
    height: auto;
    overflow: visible;
  }
  @media (max-width: 1000px) {
    .panels {
      grid-template-columns: repeat(2, 1fr);
      row-gap: 40px;
    }
    .panel:nth-child(odd) {
      border-left: 0;
    }
    .panel p {
      min-height: 2.9em;
    }
  }
  @media (max-width: 560px) {
    .panels {
      grid-template-columns: 1fr;
      row-gap: 0;
    }
    .panel {
      padding-block: 24px;
      border-left: 0;
      border-top: 1px dashed color-mix(in srgb, var(--muted) 60%, transparent);
    }
    .panel:first-child {
      padding-top: 0;
      border-top: 0;
    }
    .panel p {
      min-height: 0;
    }
  }

  text {
    fill: var(--text);
    font-family: var(--font-sans);
  }
  .t {
    font-size: 12.5px;
  }
  .m {
    fill: var(--muted);
    font: 12px var(--font-mono);
  }
  .m.out {
    fill: var(--text);
  }
  .m.e {
    fill: var(--evidence);
  }
  .mid {
    text-anchor: middle;
  }
  .mark {
    fill: var(--ok);
    font: 12px var(--font-mono);
  }
  .frame {
    fill: none;
    stroke: var(--muted);
    stroke-width: 1;
    stroke-dasharray: 3 3;
  }
  .frame.wyrd {
    fill: color-mix(in srgb, var(--declare) 22%, var(--bg));
    fill-opacity: 0.4;
    stroke: var(--declare);
    stroke-width: 1.5;
    stroke-dasharray: none;
  }
  .frame.bif {
    stroke: var(--evidence);
    stroke-opacity: 0.6;
  }
  .slot {
    fill: none;
    stroke: var(--muted);
  }
  /* Your application's steps turn solid when they run. */
  .hot {
    fill: none;
    stroke: var(--text);
    stroke-width: 1.5;
  }
  .hot.a {
    stroke: var(--gateway);
  }
  .hot.ok {
    stroke: var(--ok);
  }
  .edge {
    fill: none;
    stroke: var(--muted);
    stroke-dasharray: 2 3;
  }
  .edge.a {
    stroke: var(--gateway);
  }
  .edge.w {
    stroke: var(--declare);
  }
  .edge.e,
  .tap {
    fill: none;
    stroke: var(--evidence);
    stroke-opacity: 0.7;
    stroke-dasharray: 2 3;
  }
  .rule.e {
    stroke: var(--evidence);
    stroke-opacity: 0.4;
  }
  .pip {
    fill: var(--evidence);
  }
  .pip.ok {
    fill: var(--ok);
  }

  /* A beat element shows only during its beats. */
  [data-b] {
    opacity: 0;
    transition: opacity 0.12s;
  }
  [data-b].on {
    opacity: 1;
  }
  .fill {
    fill: var(--ok);
    transform: scaleX(0);
    transform-box: fill-box;
    transform-origin: left center;
    transition-property: transform, opacity;
    transition-duration: 0.15s, 0.05s;
    transition-timing-function: linear;
  }
  .fill.on {
    transform: none;
  }
  .fill.a {
    fill: var(--gateway);
  }

  /* Pulses: endpoints set per dot; `via` turns one corner. */
  .dot {
    fill: var(--text);
    transition: none;
  }
  .dot.a {
    fill: var(--gateway);
  }
  .dot.w {
    fill: var(--declare);
  }
  .dot.ok {
    fill: var(--ok);
  }
  .dot.ev {
    fill: var(--evidence);
  }
  .dot.on {
    animation: move 0.24s linear both;
  }
  .dot.via.on {
    animation-name: move-via;
  }
  @keyframes move {
    from {
      transform: translate(var(--x0), var(--y0));
    }
    to {
      transform: translate(var(--x1), var(--y1));
    }
  }
  @keyframes move-via {
    0% {
      transform: translate(var(--x0), var(--y0));
    }
    50% {
      transform: translate(var(--xm), var(--ym));
    }
    100% {
      transform: translate(var(--x1), var(--y1));
    }
  }
  .halt .dot {
    animation-play-state: paused;
  }

  .controls {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin-top: 28px;
    padding-top: 8px;
    border-top: 1px solid var(--border);
    color: var(--muted);
    font: 12px var(--font-mono);
  }
  .controls button {
    min-height: 28px;
    padding: 0 10px;
    border: 1px solid var(--muted);
    border-radius: var(--r);
    background: transparent;
    color: var(--text);
    font: inherit;
    cursor: pointer;
  }
  .controls button:hover {
    border-color: var(--text);
  }

  @media (prefers-reduced-motion: reduce) {
    .dot {
      display: none;
    }
    [data-b],
    .fill {
      transition: none;
    }
  }
</style>
