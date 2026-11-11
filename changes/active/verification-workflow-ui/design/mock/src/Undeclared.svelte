<script>
  import CopyBtn from './CopyBtn.svelte';
  import Fact from './Fact.svelte';
  import { REPOS, CONN, UNDECLARED_TOML } from './catalog.js';
  import { ui, access, readiness, connected } from './store.svelte.js';

  // A provider repository with GitHub access but no Repository Card: how to declare one.
  const r = $derived(REPOS.find((x) => !x.card && x.id === ui.ghid));
  const cmd = 'wyrd declare wyrd.toml';
</script>

{#if !connected()}
  <h1 class="mono page-title">{r.full}</h1>
  <div class="banner warn" role="status">Wyrd cannot reach this repository without a GitHub connection, and it has no Repository Card. <a href="#/settings/github">Connection settings</a></div>
{:else}
  <div class="xhead">
    <div class="xtitle">
      <div class="xname"><span class="tag" data-nocard>no Repository Card</span><h1 class="mono">{r.full}</h1></div>
      <div class="xmeta">github · {CONN.host} · repository_id {r.id}</div>
      <p class="desc">GitHub access is granted, but nothing is registered. Access alone never creates a Card, binds a Verifier or starts verification.</p>
    </div>
    <div class="xactions"><a class="btn btn-sm" href="https://{CONN.host}/{r.full}" target="_blank" rel="noopener noreferrer">{r.full} ↗</a></div>
  </div>
  <dl class="facts" data-testid="facts">
    <div><dt>Card</dt><dd><span class="st c-muted" data-card="none"><span aria-hidden="true">○</span>Not registered</span></dd></div>
    <div><dt>GitHub access</dt><dd><Fact f={access(r)} name="access" /></dd></div>
    <div><dt>Automatic verification</dt><dd><Fact f={readiness(r)} name="ready" /></dd></div>
  </dl>
  <section class="section" aria-labelledby="d-h">
    <div class="section-head"><h2 class="h" id="d-h">Declare it</h2><CopyBtn label="Copy wyrd.toml" text={UNDECLARED_TOML} /></div>
    <p class="prose">Add this file at the repository root, point verifier at your own Verifier definitions, then run:</p>
    <pre class="code small-code" data-testid="decl">{UNDECLARED_TOML}</pre>
    <div class="ident"><code class="mono">{cmd}</code><CopyBtn label="Copy command" text={cmd} /></div>
    <p class="note">The server infers the Repository Card name, provider, host and repository_id, verifies them against this connection, registers the Card and selects that exact version.</p>
  </section>
{/if}
