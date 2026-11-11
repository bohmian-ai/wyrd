<script>
  import Verify from './Verify.svelte';
  import ExecPage from './ExecPage.svelte';
  import VerifierPage from './VerifierPage.svelte';
  import Repositories from './Repositories.svelte';
  import RepoDetail from './RepoDetail.svelte';
  import Undeclared from './Undeclared.svelte';
  import PullPage from './PullPage.svelte';
  import Settings from './Settings.svelte';
  import { EVENTS, REPOS } from './catalog.js';
  import { ui, execs, onHash, fromHash, nextEvent, setConn, pullHref, reposHref } from './store.svelte.js';
  import logoDark from '../../../../../../crates/wyrd/wyrd-server/wyrd-ui/brand/logo.svg';
  import logoLight from '../../../../../../crates/wyrd/wyrd-server/wyrd-ui/brand/logo-light.svg';

  let mode = $state(new URLSearchParams(location.search).get('mode') === 'light' ? 'light' : 'dark');
  $effect(() => { document.documentElement.dataset.mode = mode; });
  function toggleMode() {
    mode = mode === 'dark' ? 'light' : 'dark';
    const u = new URL(location.href);
    if (mode === 'light') u.searchParams.set('mode', 'light'); else u.searchParams.delete('mode');
    history.replaceState(null, '', u);
  }
  fromHash();
  ui.here = location.hash || '#/verify';
  $effect(() => {
    addEventListener('hashchange', onHash);
    return () => removeEventListener('hashchange', onHash);
  });

  const event = $derived(EVENTS[ui.events]);
  const REPO_ROUTES = ['repos', 'repo', 'undeclared', 'pull'];
  const section = $derived(REPO_ROUTES.includes(ui.route) ? 'repositories' : ui.route === 'settings' ? 'settings' : 'verify');
  const repo = $derived(REPOS.find((r) => r.card === ui.card));

  // Review aid only: every example is also reachable through ordinary rows, facets and links.
  const EXAMPLES = [
    ['Ordinary policy review · exe-7c1e02a4', '#/x/exe-7c1e02a4'],
    ['Session judgment · exe-6c19e5f0', '#/x/exe-6c19e5f0'],
    ['Waiting for input and approval · exe-4d1e9a07', '#/x/exe-4d1e9a07'],
    ['Running PR review · exe-7f3a91c2', '#/x/exe-7f3a91c2'],
    ['Drift window · exe-d1f7a00e', '#/x/exe-d1f7a00e'],
    ['PR #482 review sets (latest)', pullHref('payments-api', 482)],
    ['PR #482 earlier set rs-482-1 (pinned)', pullHref('payments-api', 482, 'rs-482-1')],
    ['Repositories', '#/repositories'],
    ['Repository ready · payments-api', '#/repositories/payments-api'],
    ['Repository access removed · ledger', '#/repositories/ledger'],
    ['Repository not configured · docs-site', '#/repositories/docs-site'],
    ['GitHub repository with no Card', '#/repositories/github/741660215'],
    ['Verifier pr-review@1.3.0', '#/cards/verifiers/pr-review@1.3.0'],
    ['Settings › GitHub', '#/settings/github'],
  ];
</script>

<div class="preview-strip" role="region" aria-label="Design preview controls">
  <span class="preview-label">Design preview · sample data · simulated server responses · not product controls</span>
  <span class="preview-gap"></span>
  <label class="preview-pick">Examples
    <select data-testid="examples" value="" onchange={(e) => { if (e.currentTarget.value) location.hash = e.currentTarget.value; e.currentTarget.value = ''; }}>
      <option value="">Open…</option>
      {#each EXAMPLES as [text, href] (href)}<option value={href}>{text}</option>{/each}
    </select>
  </label>
  <button class="preview-btn" data-testid="simulate" disabled={!event} title={event?.label ?? 'No further simulated event'} onclick={nextEvent}>
    {event ? `Simulate live event ${ui.events + 1}/${EVENTS.length}: ${event.label}` : 'No further live event'}
  </button>
  <div role="group" aria-label="Preview role" class="preview-seg">
    <button data-testid="role-admin" aria-pressed={ui.role === 'admin'} onclick={() => (ui.role = 'admin')}>Admin</button>
    <button data-testid="role-viewer" aria-pressed={ui.role === 'viewer'} onclick={() => (ui.role = 'viewer')}>Viewer</button>
  </div>
  <div role="group" aria-label="Preview connection state" class="preview-seg">
    <button data-conn="connected" aria-pressed={ui.conn === 'connected'} onclick={() => setConn('connected')}>Connected</button>
    <button data-conn="disconnected" aria-pressed={ui.conn === 'disconnected' || ui.conn === 'handoff'} onclick={() => setConn('disconnected')}>Disconnected</button>
    <button data-conn="lost" aria-pressed={ui.conn === 'lost'} onclick={() => setConn('lost')}>Access lost</button>
  </div>
  <label class="preview-pick">Decision response
    <select data-testid="respond" bind:value={ui.respond}><option value="accepted">accepted</option><option value="conflict">conflict</option><option value="expired">expired</option></select>
  </label>
  <label class="preview-pick">Cancel response
    <select data-testid="race" bind:value={ui.race}><option value="cancelled">cancelled</option><option value="finished">finished first</option></select>
  </label>
</div>

<div class="app">
  <aside class="nav" aria-label="Primary">
    <div class="nav-brand">
      <a class="brand" href="#/verify" aria-label="Wyrd">
        <img class="mark mark-dark" src={logoDark} alt="" width="24" height="24">
        <img class="mark mark-light" src={logoLight} alt="" width="24" height="24">
        <span class="wordmark">Wyrd</span>
      </a>
    </div>
    <nav class="nav-links">
      <span class="nav-item" aria-disabled="true" title="Not part of this preview"><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M3 11l9-7 9 7v9h-6v-6H9v6H3z"/></svg><span>Home</span></span>
      <span class="nav-item" aria-disabled="true" title="Not part of this preview"><svg viewBox="0 0 24 24" aria-hidden="true"><rect x="4" y="4" width="16" height="16"/><path d="M4 9h16"/></svg><span>Cards</span></span>
      <a class="nav-item" href={reposHref()} data-nav="repositories" aria-current={section === 'repositories' ? 'page' : undefined}><svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="6" cy="6" r="2"/><circle cx="6" cy="18" r="2"/><path d="M6 8v8M10 6h8v12h-8"/></svg><span>Repositories</span></a>
      <span class="nav-rule" aria-hidden="true"></span>
      <span class="nav-item" aria-disabled="true" title="Not part of this preview"><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M3 12h4l3-7 4 14 3-7h4"/></svg><span>Observe</span></span>
      <a class="nav-item" href="#/verify" data-nav="verify" aria-current={section === 'verify' ? 'page' : undefined}><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M5 12l5 5L20 7"/></svg><span>Verify</span></a>
      <span class="nav-item" aria-disabled="true" title="Not part of this preview"><svg viewBox="0 0 24 24" aria-hidden="true"><ellipse cx="12" cy="6" rx="8" ry="3"/><path d="M4 6v12c0 1.7 3.6 3 8 3s8-1.3 8-3V6"/></svg><span>Query</span></span>
      <span class="nav-rule" aria-hidden="true"></span>
      <span class="nav-item" aria-disabled="true" title="Not part of this preview"><svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="6" cy="6" r="2"/><circle cx="6" cy="18" r="2"/><circle cx="18" cy="12" r="2"/><path d="M6 8v8M8 6h3a5 5 0 0 1 5 5"/></svg><span>Changes</span></span>
    </nav>
    <div class="nav-bottom">
      <a class="nav-item" href="#/settings/github" data-nav="settings" aria-current={section === 'settings' ? 'page' : undefined}><svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="3"/><path d="M12 2v3M12 19v3M2 12h3M19 12h3M5 5l2 2M17 17l2 2M5 19l2-2M17 7l2-2"/></svg><span>Settings</span></a>
      <div class="nav-foot">
        <span class="nav-user"><span class="avatar" aria-hidden="true">a</span><span class="nav-user-text"><span>alice</span><span class="muted" data-testid="nav-role">ui-live · {ui.role}</span></span></span>
      </div>
    </div>
  </aside>
  <div class="main">
    <header class="topbar">
      <nav aria-label="Breadcrumb" class="crumbs mono" data-testid="crumbs">
        <span>ui-live</span> /
        {#if ui.route === 'verify'}<span aria-current="page">Verify</span>
        {:else if ui.route === 'exec'}<a href="#/verify">Verify</a> / <span>{execs[ui.x].def.split('@')[0]}</span> / <span aria-current="page">{ui.x}</span>
        {:else if ui.route === 'repos'}<span aria-current="page">Repositories</span>
        {:else if ui.route === 'repo'}<a href={reposHref()}>Repositories</a> / <span aria-current="page">{ui.card}</span>
        {:else if ui.route === 'undeclared'}<a href={reposHref()}>Repositories</a> / <span aria-current="page">{REPOS.find((r) => r.id === ui.ghid)?.full}</span>
        {:else if ui.route === 'pull'}<a href={reposHref()}>Repositories</a> / <a href="#/repositories/{ui.card}">{ui.card}</a> / <span aria-current="page">PR #{ui.pr}</span>
        {:else if ui.route === 'verifier'}<a href="#/verify">Verify</a> / <span>Verifier</span> / <span aria-current="page">{ui.vdef}</span>
        {:else if ui.route === 'settings'}<span>Settings</span> / <span>Integrations</span> / <span aria-current="page">GitHub</span>
        {:else}<span aria-current="page">Not found</span>{/if}
      </nav>
      <button class="btn btn-sm" data-testid="theme" aria-label="Switch to {mode === 'dark' ? 'light' : 'dark'} theme" onclick={toggleMode}>
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M2 12h2M20 12h2M5 5l1.4 1.4M17.6 17.6L19 19M5 19l1.4-1.4M17.6 6.4L19 5"/></svg>
        {mode === 'dark' ? 'Light' : 'Dark'}
      </button>
    </header>
    <main class="view" data-route={ui.route}>
      {#if ui.route === 'verify'}<Verify />
      {:else if ui.route === 'exec'}{#key ui.x}<ExecPage {mode} />{/key}
      {:else if ui.route === 'verifier'}{#key ui.vdef}<VerifierPage {mode} />{/key}
      {:else if ui.route === 'repos'}<Repositories />
      {:else if ui.route === 'repo' && repo}{#key ui.card}<RepoDetail />{/key}
      {:else if ui.route === 'undeclared'}<Undeclared />
      {:else if ui.route === 'pull'}{#key ui.card + ui.pr}<PullPage />{/key}
      {:else if ui.route === 'settings'}<Settings />
      {:else}
        <p class="note box" data-testid="missing">Nothing at {ui.missing} in this preview. <a href="#/verify">Go to Verify</a> · <a href="#/repositories">Back to Repositories</a></p>
      {/if}
    </main>
  </div>
</div>

{#if ui.fallback}
  <div class="copy-fallback">
    <label class="small muted" for="copy-input">Copy this text</label>
    <textarea id="copy-input" class="mono" readonly rows="3" value={ui.fallback}></textarea>
    <button class="btn btn-sm" onclick={() => (ui.fallback = '')}>Done</button>
  </div>
{/if}
