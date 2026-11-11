<script>
  import Fact from './Fact.svelte';
  import { REPOS, CONN } from './catalog.js';
  import { ui, execs, readiness, admin, connect, disconnect, cancellable } from './store.svelte.js';

  // Settings → Integrations → GitHub: the tenant's one GitHub App connection.
  const viewer = $derived(!admin());
  const lost = $derived(ui.conn === 'lost');
  let dlg = $state();
  let cards = $state([]);
  let unfinished = $state(0);
  function openDisconnect() {
    cards = REPOS.filter((r) => r.card && r.access).map((r) => r.card);
    unfinished = Object.values(execs).filter((x) => x.repo && !x.hidden && cancellable(x)).length;
    dlg.returnValue = '';
    dlg.showModal();
  }
  const onclose = () => { if (dlg.returnValue === 'confirm') disconnect(); };
</script>

<div class="page-head"><div class="grow"><h1>GitHub</h1>
  <p class="desc">One GitHub App connection for this tenant. Connecting gives Wyrd access to the repositories you select on GitHub; it does not register Repository Cards or start verification.</p></div></div>

{#snippet viewerNote()}<p class="note box" data-testid="viewer-note">You can view this connection. Connecting, managing access and disconnecting need an admin.</p>{/snippet}

{#if ui.conn === 'handoff'}
  <div class="banner info" role="status" data-testid="handoff">Continuing on GitHub to install the App and choose repositories… GitHub returns here when you finish. (Simulated: no request leaves this page.)</div>
{:else if ui.conn === 'disconnected'}
  <dl class="facts"><div><dt>State</dt><dd><span class="st c-muted" data-testid="conn-state"><span aria-hidden="true">○</span>Not connected</span></dd></div></dl>
  <p class="prose" style="max-width: 72ch; margin: 0">GitHub handles App installation, your authorization and which repositories Wyrd may read on {CONN.host}. When you finish there, GitHub returns you here and the connection is verified.</p>
  {#if viewer}{@render viewerNote()}{:else}<div><button type="button" class="btn btn-primary" data-testid="connect" onclick={connect}>Connect GitHub</button></div>{/if}
{:else}
  <dl class="facts">
    <div><dt>State</dt><dd>
      {#if lost}<span class="st c-danger" data-testid="conn-state"><span aria-hidden="true">✕</span>Access lost</span>
        <span class="why">GitHub reports the installation suspended or removed. New verification stopped; unfinished executions were cancelled.</span>
      {:else}<span class="st c-ok" data-testid="conn-state"><span aria-hidden="true">✓</span>Connected</span>{/if}
    </dd></div>
    <div><dt>Account</dt><dd><span class="mono">{CONN.account}</span><span class="why">GitHub organization</span></dd></div>
    <div><dt>Installation</dt><dd><span class="mono">{CONN.installation}</span><span class="why mono">App {CONN.app}</span></dd></div>
    <div><dt>Host</dt><dd><span class="mono">{CONN.host}</span><span class="why">Configured for this deployment</span></dd></div>
  </dl>
  {#if viewer}{@render viewerNote()}{:else}
    <div class="toolbar">
      <a class="btn btn-sm" href="https://{CONN.host}/organizations/{CONN.account}/settings/installations/{CONN.installation}" target="_blank" rel="noopener noreferrer" data-testid="manage">Manage access on GitHub ↗</a>
      <button type="button" class="btn btn-sm btn-danger" data-testid="disconnect" onclick={openDisconnect}>Disconnect</button>
    </div>
  {/if}
  <section class="section" aria-labelledby="acc-h">
    <div class="section-head"><h2 class="h" id="acc-h">Repository access granted on GitHub</h2></div>
    {#if lost}
      <p class="note box">No repository access while the installation is suspended or removed. Registered Repository Cards and their history stay readable.</p>
    {:else}
      <div class="outs-wrap">
        <table class="outs" data-testid="access-table">
          <thead><tr><th scope="col">GitHub repository</th><th scope="col">repository_id</th><th scope="col">Repository Card (registration is separate)</th><th scope="col">Automatic verification</th></tr></thead>
          <tbody>
            {#each REPOS.filter((r) => r.access) as r (r.full)}
              <tr data-access-row={r.full}>
                <td class="cell"><div class="cellbox"><a class="mono" href="https://{CONN.host}/{r.full}" target="_blank" rel="noopener noreferrer">{r.full} ↗</a></div></td>
                <td class="cell"><div class="cellbox"><span class="mono">{r.id}</span></div></td>
                <td class="cell"><div class="cellbox">
                  {#if r.card}<a class="mono" href="#/repositories/{r.card}">Repository/{r.card}@{r.selected}</a>
                  {:else}<span class="ident"><span class="tag" data-nocard>no Card</span><a class="small" href="#/repositories/github/{r.id}">How to declare</a></span>{/if}
                </div></td>
                <td class="cell"><div class="cellbox"><Fact f={readiness(r)} name="ready" /></div></td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}
    <p class="note">Adding access on GitHub never creates a Repository Card or bindings. Removing access stops new verification for that repository and cancels its unfinished executions; recorded evidence stays.</p>
  </section>
{/if}

<dialog id="disconnect" aria-labelledby="disconnect-title" bind:this={dlg} {onclose}>
  <form method="dialog" class="dlg">
    <h2 id="disconnect-title">Disconnect GitHub?</h2>
    <ul class="dlg-list" data-testid="disconnect-list">
      <li>New verification stops for {cards.length} Repository Cards with access ({cards.join(', ')}).</li>
      <li>{unfinished} unfinished execution{unfinished === 1 ? ' is' : 's are'} cancelled.</li>
      <li>Recorded executions, verdicts and evidence stay. Repository Cards and versions are unchanged.</li>
    </ul>
    <p class="note">This preview only changes local state. Uninstalling the App stays on GitHub.</p>
    <div class="dlg-actions">
      <!-- svelte-ignore a11y_autofocus -->
      <button class="btn" value="cancel" autofocus>Cancel</button>
      <button class="btn btn-danger" value="confirm" data-testid="disconnect-confirm">Disconnect GitHub</button>
    </div>
  </form>
</dialog>
