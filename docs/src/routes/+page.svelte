<script lang="ts">
  import { base } from '$app/paths';
  import CoverScene from '$lib/components/CoverScene.svelte';
  import { firstPage, navGroups } from '$lib/derived-nav';
  import type { Section } from '$lib/content';

  // Landing: the painted cover and value statement, the Declare → Observe →
  // Verify strip, the platform summary, then one door per published section.
  // Every internal action resolves through published content, so a section or
  // the quickstart appears here only once its page exists.
  const GITHUB_URL = 'https://github.com/bohmian-ai/wyrd';
  const DOORS: Record<Section, { blurb: string; kicker: string }> = {
    'Get started': { blurb: 'Learn what Wyrd verifies and set up a local server.', kicker: 'Start' },
    'Build and verify': { blurb: 'Declare an AI system, observe its behavior, and verify it.', kicker: 'How-to' },
    Connect: { blurb: 'Use the SDKs, CLI, MCP, and the AI Gateway.', kicker: 'How-to' },
    Operate: { blurb: 'Deploy Wyrd for a team and manage sign-in and access.', kicker: 'Operate' },
    Understand: { blurb: 'How Cards, verification, Bifrost, and identity fit together.', kicker: 'Explain' },
    Reference: { blurb: 'Card schemas, APIs, CLI commands, errors, and configuration.', kicker: 'Look up' }
  };
  const doors = navGroups().map((group) => ({ label: group.label, path: group.items[0].path, ...DOORS[group.label] }));
  const start = firstPage('Get started');
  const startPages = navGroups().find((group) => group.label === 'Get started')?.items ?? [];
</script>

<svelte:head>
  <title>Wyrd docs</title>
  <meta
    name="description"
    content="Wyrd is open-source verification and assurance infrastructure for AI systems. Verify AI systems before and after release, with verdicts you can trace, reproduce, and audit."
  />
</svelte:head>

<section class="cover dark-field" aria-label="Wyrd">
  <CoverScene />
  <div class="wrap hero">
    <p class="eyebrow">Open-source verification and assurance for AI systems</p>
    <h1><span class="d">Declare.</span><br /><span class="o">Observe.</span><br />Verify.</h1>
    <p class="tagline">Declare what an AI system should do, record what it does, and get a verdict on whether they match.</p>
    <p class="lede">For an individual LLM, a deployed agent service, or a traditional HTTP service. Verify before release and keep verifying in production.</p>
    <div class="cta">
      {#if start}
        <a class="button primary" href={`${base}${start.path}`}>Get started →</a>
        <a class="button" href={GITHUB_URL} rel="noreferrer" target="_blank">GitHub</a>
      {:else}
        <a class="button primary" href={GITHUB_URL} rel="noreferrer" target="_blank">Wyrd on GitHub →</a>
      {/if}
    </div>
  </div>
</section>

<div class="wrap">
  <section class="layers" aria-labelledby="layers-h">
    <p class="eyebrow">Core platform</p>
    <h2 id="layers-h">One loop, three steps.</h2>
    <ol class="strip">
      <li><span class="n">01</span><b>Declare</b><p>Describe the AI system as a set of versioned Cards, and bind the Verifiers that check them.</p></li>
      <li><span class="n">02</span><b class="o">Observe</b><p>Record what the system does. Observations, traces, logs, metrics, and custom data land in Bifrost, Wyrd's SQL data plane.</p></li>
      <li><span class="n">03</span><b>Verify</b><p>Verifiers judge that behavior in real time or on a schedule, before and after deployment. Each verdict names the exact versions that ran.</p></li>
    </ol>
  </section>

  {#if start}
    <section class="qs" aria-labelledby="qs-h">
      <div>
        <p class="eyebrow">Get started</p>
        <h2 id="qs-h">From intent to evidence.</h2>
        <p class="sub">Learn what Wyrd verifies, then run a local server and connect a client.</p>
        <nav class="links" aria-label="Get started">
          {#each startPages as item (item.path)}
            <a href={`${base}${item.path}`}>{item.label}</a>
          {/each}
        </nav>
      </div>
      <figure class="code">
        <figcaption><span>Example declaration</span><span>YAML</span></figcaption>
        <pre><span class="ln">1</span><span class="k">apiVersion</span>: wyrd/v1
<span class="ln">2</span><span class="k">kind</span>: Service
<span class="ln">3</span><span class="k">metadata</span>:
<span class="ln">4</span>  <span class="k">space</span>: retention
<span class="ln">5</span>  <span class="k">name</span>: support-copilot
<span class="ln">6</span><span class="k">spec</span>:
<span class="ln">7</span>  <span class="k">components</span>:
<span class="ln">8</span>    - <span class="k">alias</span>: triage
<span class="ln">9</span>      <span class="k">ref</span>: ./agents/triage.yaml
<span class="ln">10</span>      <span class="k">verified_by</span>:
<span class="ln">11</span>        - <span class="k">verifier</span>: ./verifiers/triage-quality.yaml
<span class="ln">12</span>          <span class="k">on_failure</span>: [./operators/page-oncall.yaml]</pre>
      </figure>
    </section>
  {/if}

  <section class="platform" aria-labelledby="platform-h">
    <p class="eyebrow">Also in the platform</p>
    <h2 id="platform-h">Evidence others can check.</h2>
    <div class="trio">
      <div><b>Identity and access</b><p>Role-based access, OIDC sign-in for people, and API keys for workloads. Delegated tokens (RFC 8693 token exchange) keep calls made on someone's behalf attributed to them. Every access decision is written to the audit log.</p></div>
      <div><b>Gateway</b><p>Route LLM calls through the Wyrd AI Gateway. Each call is authorized for the calling agent, attributed to it, and recorded for end-to-end verification.</p></div>
      <div><b>Interfaces</b><p>Python, TypeScript, and Rust SDKs, OpenTelemetry (OTLP) ingest, HTTP, CLI, and MCP. You run the server yourself, on Postgres and object storage.</p></div>
    </div>
  </section>

  {#if doors.length > 0}
    <nav class="doors" aria-label="Documentation sections">
      {#each doors as door (door.label)}
        <a href={`${base}${door.path}`}><b>{door.label}</b><span>{door.blurb}</span><small>{door.kicker} →</small></a>
      {/each}
    </nav>
  {:else}
    <p class="empty">
      The guides are being rewritten, and each one appears here once it has been checked against the current code.
      Until then, the <a href={GITHUB_URL} rel="noreferrer" target="_blank">repository</a> README and its
      <code>architecture/</code> documents describe how Wyrd works.
    </p>
  {/if}
</div>

<style>
  .wrap {
    width: min(1240px, calc(100% - 40px));
    margin: 0 auto;
  }

  .cover {
    position: relative;
    min-height: 720px;
    overflow: hidden;
  }
  .hero {
    position: relative;
    max-width: 560px;
    margin-left: max(20px, calc((100% - 1240px) / 2));
    padding: 64px 0 72px;
  }
  .eyebrow {
    margin: 0 0 12px;
    color: var(--observe);
    font: 13px/1.4 var(--font-mono);
    letter-spacing: 0.18em;
    text-transform: uppercase;
  }
  .wrap .eyebrow {
    color: var(--muted);
  }
  h1 {
    margin: 0;
    font-size: clamp(56px, 7.5vw, 104px);
    font-weight: 700;
    line-height: 0.92;
    letter-spacing: -0.04em;
  }
  h1 .d {
    color: var(--declare);
  }
  h1 .o {
    box-shadow: inset 0 -0.08em 0 0 var(--observe);
  }
  .tagline {
    margin: 22px 0 8px;
    font-size: clamp(24px, 2.6vw, 34px);
    font-weight: 500;
    line-height: 1.15;
    letter-spacing: -0.02em;
  }
  .lede {
    max-width: 40ch;
    margin: 0;
    color: var(--muted);
    font-size: 18px;
    line-height: 1.45;
  }
  .cta {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
    margin-top: 30px;
  }
  .cover .button {
    min-height: 46px;
    padding: 0 22px;
    border-color: var(--text);
    background: var(--bg);
    font-family: var(--font-mono);
    font-size: 15px;
  }
  .cover .button.primary {
    border-color: var(--declare);
    background: var(--declare);
  }
  .layers,
  .platform {
    padding: 56px 0 0;
  }
  .layers .strip {
    margin-top: 24px;
  }
  .trio {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 14px;
    margin-top: 24px;
    padding-bottom: 56px;
  }
  .trio div {
    padding: 18px;
    border: 1px solid var(--border);
    border-radius: var(--r);
    background: var(--surface);
  }
  .trio b {
    display: block;
    font-size: 17px;
  }
  .trio p {
    margin: 6px 0 0;
    color: var(--muted);
    font-size: 15px;
    line-height: 1.5;
  }
  .strip {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    margin: 0;
    padding: 0;
    list-style: none;
    border-top: 1px solid var(--declare);
    border-bottom: 1px solid var(--declare);
  }
  .strip li {
    display: grid;
    grid-template-columns: 34px 1fr;
    gap: 8px 12px;
    padding: 22px 16px;
  }
  .strip li + li {
    padding-left: 28px;
    border-left: 1px solid var(--declare);
  }
  .strip .n {
    color: var(--warn);
    font: 17px var(--font-mono);
  }
  .strip b {
    justify-self: start;
    font: 500 18px var(--font-mono);
  }
  .strip b.o {
    padding: 1px 6px;
    border-radius: var(--r);
    background: var(--observe);
    color: var(--evidence-ink);
  }
  .strip p {
    grid-column: 2;
    margin: 0;
    color: var(--muted);
    font-size: 15px;
    line-height: 1.5;
  }

  .qs {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 48px;
    align-items: start;
    padding: 48px 0 56px;
  }
  h2 {
    margin: 0;
    font-size: clamp(36px, 4vw, 52px);
    font-weight: 700;
    line-height: 1;
    letter-spacing: -0.03em;
  }
  .sub {
    max-width: 40ch;
    margin: 10px 0 0;
    color: var(--muted);
    font-size: 18px;
    line-height: 1.45;
  }
  .links {
    display: grid;
    gap: 10px;
    margin-top: 22px;
    padding-left: 16px;
    border-left: 2px solid var(--warn);
    font: 16px var(--font-mono);
  }
  .links a {
    color: var(--muted);
    text-decoration: none;
  }
  .links a:hover {
    color: var(--text);
  }
  .links a:first-child {
    color: var(--text);
  }
  .links a:first-child::before {
    content: '→  ';
    color: var(--warn);
  }
  .code {
    margin: 0;
    border: 1px solid var(--border);
    border-radius: var(--r);
    background: var(--code-bg);
    color: var(--code-text);
  }
  .code figcaption {
    display: flex;
    justify-content: space-between;
    padding: 10px 14px;
    border-bottom: 1px solid var(--code-muted);
    color: var(--code-muted);
    font: 13px var(--font-mono);
  }
  .code pre {
    margin: 0;
    padding: 16px 14px;
    overflow-x: auto;
    font: 14px/1.7 var(--font-mono);
  }
  .ln {
    display: inline-block;
    width: 28px;
    margin-right: 14px;
    border-right: 1px solid var(--code-muted);
    color: var(--code-muted);
  }
  .k {
    color: var(--code-keyword);
  }
  .empty {
    max-width: 68ch;
    margin: 0;
    padding: 8px 0 64px;
    color: var(--muted);
    font-size: 16px;
    line-height: 1.55;
  }
  .doors {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(260px, 1fr));
    gap: 14px;
    padding-bottom: 64px;
  }
  .doors a {
    display: grid;
    gap: 6px;
    padding: 18px;
    border: 1px solid var(--border);
    border-radius: var(--r);
    background: var(--surface);
    color: var(--text);
    text-decoration: none;
    transition: border-color var(--ease);
  }
  .doors a:hover {
    border-color: var(--declare);
  }
  .doors b {
    font-size: 17px;
  }
  .doors span {
    color: var(--muted);
    font-size: 14.5px;
  }
  .doors small {
    padding-top: 8px;
    color: var(--declare);
    font: 12px var(--font-mono);
  }

  @media (max-width: 900px) {
    .cover {
      min-height: 0;
    }
    .hero {
      margin-left: 20px;
      margin-right: 20px;
      padding-bottom: 420px;
    }
    .strip,
    .qs,
    .trio,
    .doors {
      grid-template-columns: minmax(0, 1fr);
    }
    .strip li + li {
      padding-left: 16px;
      border-left: 0;
      border-top: 1px solid var(--declare);
    }
  }
</style>
