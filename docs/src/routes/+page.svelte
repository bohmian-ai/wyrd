<script lang="ts">
  import { base } from '$app/paths';
  import CoverScene from '$lib/components/CoverScene.svelte';
  import { firstPage, navGroups } from '$lib/derived-nav';
  import type { Section } from '$lib/content';
  import CodeBlock from '$lib/mdsvex/CodeBlock.svelte';
  import LangTabs from '$lib/components/LangTabs.svelte';
  import LangTab from '$lib/components/LangTab.svelte';
  import Tabs from '$lib/components/Tabs.svelte';
  import Tab from '$lib/components/Tab.svelte';

  let { data } = $props();

  // Landing: the painted cover and value statement, the Declare → Observe →
  // Verify strip, the platform summary, then one door per published section.
  // Every internal action resolves through published content, so a section or
  // a start page appears here only once its page exists.
  const GITHUB_URL = 'https://github.com/bohmian-ai/wyrd';
  const DOORS: Record<Section, { blurb: string; kicker: string }> = {
    'Get started': { blurb: 'Learn what Wyrd verifies and set up a local server.', kicker: 'Start' },
    'Build and verify': { blurb: 'Declare an AI system, observe its behavior, and verify it.', kicker: 'How-to' },
    Connect: { blurb: 'Use the SDKs, CLI, MCP, and the AI Gateway.', kicker: 'How-to' },
    Operate: { blurb: 'Configure how a Wyrd server runs verification.', kicker: 'Operate' },
    Understand: { blurb: 'How Cards, verification, Bifrost, and identity fit together.', kicker: 'Explain' },
    Reference: { blurb: 'The Verifier Card, MCP tools, the HTTP API, errors, and runtime limits.', kicker: 'Look up' }
  };
  const doors = navGroups().map((group) => ({ label: group.label, path: group.items[0].path, ...DOORS[group.label] }));
  const startPages = navGroups().find((group) => group.label === 'Get started')?.items ?? [];
  // The CTA opens setup, the first step a reader can complete, else the first Get started page.
  const start = startPages.find((item) => item.path === '/get-started/setup/') ?? firstPage('Get started');
  const agent = startPages.find((item) => item.path === '/get-started/use-with-your-coding-agent/');
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
    <p class="eyebrow">Open-source verification infrastructure for AI systems · v{__WYRD_VERSION__}</p>
    <h1><span class="d">Declare.</span><br /><span class="o">Observe.</span><br />Verify.</h1>
    <p class="tagline">Know whether your AI system does what you said it would, before release and in production.</p>
    <p class="lede">Verify a single API response or a multi-hour agent coding session. Check a support agent's answers, an uploaded document against your policy, or every step of a long-running workflow.</p>
    <div class="cta">
      {#if start}
        <a class="button primary" href={`${base}${start.path}`}>Get started →</a>
        {#if agent}<a class="button" href={`${base}${agent.path}`}>Use with your coding agent</a>{/if}
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
      <li><span class="n">01</span><b>Declare</b><p>Describe the system in YAML, and bind the Verifiers that check them.</p></li>
      <li><span class="n">02</span><b class="o">Observe</b><p>Record what the system does using Wyrd's purpose-built high-throughput data plane.</p></li>
      <li><span class="n">03</span><b>Verify</b><p>Verify behavior in real time or in the background using Wyrd's verification engine.</p></li>
    </ol>
  </section>

  {#if start}
    <section class="qs" aria-labelledby="qs-h">
      <div>
        <p class="eyebrow">Get started</p>
        <h2 id="qs-h">From intent to evidence.</h2>
        <p class="sub">One support agent and one Verifier, one step per tab: declare what an answer must never say, verify two answers, and read the verdict.</p>
        <nav class="links" aria-label="Get started">
          {#each startPages as item (item.path)}
            <a href={`${base}${item.path}`}>{item.label}</a>
          {/each}
        </nav>
      </div>
      <div class="loop">
        <Tabs>
          <Tab label="01 Declare">
            <p class="step">The Service binds one Verifier. The Verifier declares the evidence it takes and the requirement it checks. Declare both with <code>wyrd declare support-reply.yaml</code>, or from your code with <code>WyrdState.declare</code>.</p>
            <Tabs>
              <Tab label="Service"><CodeBlock lang="yaml" {...data.declare.service} /></Tab>
              <Tab label="Verifier"><CodeBlock lang="yaml" {...data.declare.verifier} /></Tab>
            </Tabs>
          </Tab>
          <Tab label="02 Observe">
            <p class="step">Declare, start a Run, and verify two answers. <code>verify</code> waits for the verdict; <code>submit</code> queues the check and returns at once.</p>
            <LangTabs langs={['python', 'rust', 'typescript']}>
              {#each data.observe as o (o.lang)}
                <LangTab lang={o.lang}><CodeBlock lang={o.lang} title={o.title} html={o.html} /></LangTab>
              {/each}
            </LangTabs>
          </Tab>
          <Tab label="03 Verify">
            <p class="step">The failing answer's verdict names the requirement, the reason, and the value it judged. A failed check is a result, not an exception.</p>
            <CodeBlock lang="json" {...data.verify} />
          </Tab>
        </Tabs>
      </div>
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
    grid-template-columns: minmax(0, 2fr) minmax(0, 3fr);
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
  .loop :global(.tabs) {
    margin: 0;
  }
  .step {
    margin: 0 0 12px;
    color: var(--muted);
    font-size: 15px;
    line-height: 1.5;
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
