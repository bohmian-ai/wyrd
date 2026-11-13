<script lang="ts">
  import { BeatLoop } from '$lib/beat-loop.svelte';

  // The landing page's finance figure: the reader's own API calls Wyrd once,
  // three requirements settle, the verdict returns, and only then does the
  // API release or hold the report. Only the Agent's model call goes through
  // the Gateway, and capture to Bifrost starts after the API has continued.
  // `code` is the finance API excerpt from examples/finance-release/api.py as
  // Shiki tokens, one array per line. `failing` draws the policy violation,
  // where the API holds the report.
  type Token = { content: string; color: string };
  let { code, failing = false }: { code: Token[][]; failing?: boolean } = $props();

  const loop = new BeatLoop([250, 200, 250, 200, 1800]);
  const at = loop.at;
  const pass = $derived(!failing);
  let figure: HTMLElement;

  $effect(() => loop.attach(figure));
  $effect(() => loop.tick());

  // Requirement rows inside the verification frame: name, check kind, baseline.
  const ROWS = [
    { name: 'Recipient authorized', kind: 'ASSERT', y: 48 },
    { name: 'Sharing policy · PDF', kind: 'AGENT', y: 84 },
    { name: 'Destination restricted', kind: 'ASSERT', y: 120 }
  ];
</script>

<figure class="fig" class:halt={!loop.running} bind:this={figure}>
  <p class="sr" id="flow-desc">
    The code panel, labelled Your Finance API, is a Python excerpt of your own service. It imports WyrdState from wyrd,
    loads the state for the finance-release Service, and defines a send_report handler under the route decorator
    api.post for /reports/send. Inside a state.run block the handler makes one synchronous call into Wyrd,
    run.observe.verify, then releases the report if the verdict passed and holds it for review otherwise. During that
    call, Wyrd's real-time verification judges three requirements: an assertion that the recipient is authorized, an
    Agent judgment of the sharing policy using the policy PDF, and an assertion that the destination restricts access.
    {pass
      ? 'All three pass, the verdict returns to the handler, and the handler releases the report.'
      : 'The sharing policy check fails while both assertions pass, the verdict returns to the handler, and the handler holds the report.'}
    Only the Agent check connects to the Gateway: its model call goes out through the Gateway and returns; the assertions
    do not use it. After the handler has continued, records, traces and application data supplied for capture, including
    the verification results, flow asynchronously to Bifrost. The handler's response does not wait for that capture.
  </p>

  <p class="hint" id="flow-hint">Scroll sideways to see the whole diagram.</p>
  <div class="scroll" role="group" aria-label="Diagram" aria-describedby="flow-hint">
    <svg viewBox="0 0 960 256" role="img" aria-label="Your finance API calls Wyrd to verify a report before releasing it" aria-describedby="flow-desc">
      <g transform="translate(0 14)">
        <rect class="frame" x="10" y="4" width="490" height="166" rx="2" />
        <text class="cap mid" x="255" y="19">YOUR FINANCE API</text>
        <path class="rule" d="M10 26 H500 M36 32 V164" />
        <rect class="stripe" x="37" y="118" width="2" height="11" />
        <rect class="band" class:on={at('0-3')} x="37" y="118" width="462" height="11" />
        <rect class="band" class:on={at('4') && pass} x="37" y="140" width="462" height="11" />
        <rect class="band" class:on={at('4') && !pass} x="37" y="151" width="462" height="11" />
        <g class="ln">
          {#each code as _, i (i)}<text x="30" y={38 + 11 * i}>{i + 1}</text>{/each}
        </g>
        <g class="code src">
          {#each code as line, i (i)}
            <text x="44" y={38 + 11 * i}>{#each line as token, j (j)}<tspan style:fill={token.color}>{token.content}</tspan>{/each}</text>
          {/each}
        </g>
        <rect class="fill call" class:on={at('0-4')} x="470" y="120" width="18" height="6" rx="2" />
        <rect class="slot" x="470" y="120" width="18" height="6" rx="2" />
        <text class="mark" class:on={at('4')} x="491" y="126">{pass ? '✓' : '✕'}</text>
        {#if pass}
          <text class="mark out end" class:on={at('4')} x="490" y="148">→ Release</text>
        {:else}
          <text class="mark out end" class:on={at('4')} x="490" y="159">‖ Hold</text>
        {/if}
      </g>

      <path class="link" d="M503 137 H560" />
      <circle class="port" cx="500" cy="137" r="2.5" />
      <text class="cap out mid" class:on={at('3-4')} x="531" y="155">{pass ? '✓ PASSED' : '✕ FAILED'}</text>

      <g transform="translate(560 38)">
        <rect class="frame wyrd" x="0" y="0" width="250" height="146" rx="2" />
        <text class="cap w" x="12" y="17">WYRD · REAL-TIME VERIFICATION</text>
        <path class="rule w" d="M0 24 H250" />
        {#each ROWS as row (row.name)}
          {@const agent = row.kind === 'AGENT'}
          <text class="name" x="12" y={row.y}>{row.name}</text>
          <text class="cap" x="12" y={row.y + 13}>{row.kind}</text>
          <rect class="fill" class:agent class:bad={agent && !pass} class:on={at('1-4')} x="130" y={row.y - 9} width="88" height="8" rx="2" />
          <rect class="slot" x="130" y={row.y - 9} width="88" height="8" rx="2" />
          <text class="mark" class:on={at(agent ? '3-4' : '1-4')} x="226" y={row.y - 1}>{agent && !pass ? '✕' : '✓'}</text>
        {/each}
      </g>

      <path class="link gw" d="M810 117 H876" />
      <rect class="frame gw" x="876" y="108" width="72" height="18" rx="2" />
      <text class="cap gw mid" x="912" y="121">GATEWAY</text>
      <text class="cap gw mid" x="912" y="100">MODEL CALL</text>

      <path class="tap" d="M40 184 V224 M770 184 V224" />
      <circle class="tap-port" cx="40" cy="184" r="1.5" />
      <circle class="tap-port" cx="770" cy="184" r="1.5" />
      <text class="cap bif" x="48" y="208">ASYNC CAPTURE</text>
      <rect class="frame bif" x="10" y="224" width="940" height="26" rx="2" />
      <text class="cap bif" x="24" y="241">BIFROST</text>
      <text class="name" x="96" y="241">Records · traces · app data</text>

      <circle class="dot call" class:on={at('0')} r="3.5" style="--x0:500px;--y0:137px;--x1:560px;--y1:137px" />
      <circle class="dot" class:ok={pass} class:bad={!pass} class:on={at('3')} r="3.5" style="--x0:560px;--y0:137px;--x1:500px;--y1:137px" />
      <circle class="dot gw" class:on={at('1')} r="2.5" style="--x0:810px;--y0:117px;--x1:876px;--y1:117px" />
      <circle class="dot gw" class:on={at('2')} r="2.5" style="--x0:876px;--y0:117px;--x1:810px;--y1:117px" />
      {#each [[40, 0.35], [40, 0.8], [770, 0.5], [770, 0.95]] as [x, delay], i (i)}
        <circle class="dot ev" class:on={at('4')} r="2" style="--x0:{x}px;--y0:184px;--x1:{x}px;--y1:224px;--delay:{delay}s" />
      {/each}
    </svg>
  </div>

  <figcaption>
    <p class="status">{pass ? 'All three pass · the API releases the report' : 'Sharing policy fails · the API holds the report'}</p>
    <div class="controls">
      <span>Illustrative animation</span>
      <button type="button" onclick={loop.toggle}>{loop.playing ? 'Pause' : 'Play'}</button>
    </div>
  </figcaption>
</figure>

<style>
  .fig {
    --gateway: var(--warn);
    margin: 24px 0 0;
  }
  .sr {
    position: absolute;
    width: 1px;
    height: 1px;
    margin: -1px;
    padding: 0;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
    border: 0;
  }
  svg {
    display: block;
    width: 100%;
    height: auto;
    overflow: visible;
  }
  .hint {
    display: none;
    margin: 0 0 8px;
    color: var(--muted);
    font: 12px var(--font-mono);
  }
  /* Below 1000px the diagram keeps its drawn size and scrolls inside the
   * figure, so labels stay readable and the page never scrolls sideways. */
  @media (max-width: 1000px) {
    .hint {
      display: block;
    }
    .scroll {
      overflow-x: auto;
    }
    svg {
      min-width: 960px;
    }
  }

  text {
    fill: var(--text);
    font-family: var(--font-sans);
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
  .rule {
    stroke: var(--muted);
    stroke-opacity: 0.5;
  }
  .rule.w {
    stroke: var(--declare);
    stroke-opacity: 0.6;
  }
  .link {
    fill: none;
    stroke: var(--declare);
    stroke-dasharray: 2 3;
  }
  .port,
  .stripe {
    fill: var(--declare);
  }
  .band {
    fill: color-mix(in srgb, var(--declare) 22%, var(--bg));
  }
  .slot {
    fill: none;
    stroke: var(--muted);
  }
  .src text {
    font: 9px var(--font-mono);
    white-space: pre;
  }
  .ln text {
    fill: var(--muted);
    font: 10px var(--font-mono);
    text-anchor: end;
  }
  .cap {
    fill: var(--muted);
    font: 10px var(--font-mono);
    letter-spacing: 0.12em;
  }
  .cap.w {
    fill: var(--declare);
  }
  .cap.out {
    fill: var(--text);
  }
  .name {
    font-size: 12px;
  }
  .mark {
    font: 10px var(--font-mono);
  }
  .mid {
    text-anchor: middle;
  }
  .end {
    text-anchor: end;
  }

  /* Gateway: amber, small, attached to the Agent check only. */
  .frame.gw,
  .link.gw {
    stroke: var(--gateway);
  }
  .cap.gw,
  .dot.gw {
    fill: var(--gateway);
  }
  /* Bifrost: lime, quiet, downstream of the response. */
  .tap {
    fill: none;
    stroke: var(--evidence);
    stroke-opacity: 0.6;
    stroke-dasharray: 1 3;
  }
  .tap-port,
  .cap.bif,
  .dot.ev {
    fill: var(--evidence);
  }
  .frame.bif {
    stroke: var(--evidence);
    stroke-opacity: 0.6;
  }

  /* A beat element shows only during its beats. */
  .band,
  .mark,
  .cap.out,
  .dot {
    opacity: 0;
    transition: opacity 0.12s;
  }
  .on {
    opacity: 1;
  }
  /* Progress fills grow from the left; the Agent's spans two beats and the call's the whole round trip. */
  .fill {
    fill: var(--ok);
    transform: scaleX(0);
    transform-box: fill-box;
    transform-origin: left center;
    transition: transform 0.15s linear;
  }
  .fill.on {
    transform: none;
  }
  .fill.agent {
    transition-duration: 0.42s;
  }
  .fill.call {
    fill: var(--declare);
    transition-duration: 0.85s;
  }
  .fill.bad {
    fill: var(--danger);
  }

  /* Pulses: each lasts slightly less than its beat, endpoints set per dot. */
  .dot {
    fill: var(--text);
    transition: none;
  }
  .dot.call {
    fill: var(--declare);
  }
  .dot.ok {
    fill: var(--ok);
  }
  .dot.bad {
    fill: var(--danger);
  }
  .dot.on {
    animation: move 0.19s linear both;
  }
  .dot.ev.on {
    animation: move-fade 0.35s linear var(--delay) both;
  }
  @keyframes move {
    from {
      transform: translate(var(--x0), var(--y0));
    }
    to {
      transform: translate(var(--x1), var(--y1));
    }
  }
  /* Capture pulses stay invisible through their delay and fade out on arrival. */
  @keyframes move-fade {
    0% {
      transform: translate(var(--x0), var(--y0));
      opacity: 0;
    }
    15%,
    80% {
      opacity: 1;
    }
    100% {
      transform: translate(var(--x1), var(--y1));
      opacity: 0;
    }
  }
  .halt .dot {
    animation-play-state: paused;
  }

  figcaption {
    margin-top: 12px;
  }
  .status {
    margin: 0;
    font-size: 15px;
  }
  .controls {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin-top: 10px;
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
    .band,
    .mark,
    .cap.out,
    .fill {
      transition: none;
    }
  }
</style>
