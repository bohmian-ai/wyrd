<!--
  The tenant's one GitHub App connection: its redacted state, the
  repositories it reaches beside their Repository Cards, and three actions.
  Connect and disconnect are host intents; managing access is GitHub's own
  page. Disconnect asks first in a native dialog and sends one intent.
-->
<script lang="ts">
  import type { ActionProps, ActionState } from '../contract.js';
  import { READINESS_WORDS, type GithubIntegrationProps } from '../repositories.js';
  import ActionNote from './ActionNote.svelte';
  import EmptyState from './EmptyState.svelte';
  import RepositorySheet from './RepositorySheet.svelte';

  let { result, actions = {}, onaction }: GithubIntegrationProps & ActionProps = $props();

  /** A pending or unavailable action takes no second press. */
  const blocked = (state: ActionState | undefined) => !onaction || state?.state === 'pending' || state?.state === 'unavailable';
  const connection = $derived(result.state === 'populated' ? result.data.connection : undefined);
  const join = $derived(result.state === 'populated' && result.data.cards.state === 'populated' ? result.data.cards.data : undefined);
  /** The Repository Card of one reachable repository; undefined when it has none or the join cannot say. */
  const cardOf = (repositoryId: string) => join?.rows.find((row) => row.card && row.repositoryId === repositoryId);
  const affected = $derived(join?.rows.filter((row) => row.card && row.access === 'granted').map((row) => row.card!.name) ?? []);
  let dialog = $state<HTMLDialogElement>();
</script>

<RepositorySheet>
  {#if result.state === 'populated' && connection}
    {#if connection.state === 'not_connected'}
      <dl class="facts">
        <div><dt>State</dt><dd><span class="st c-muted"><span aria-hidden="true">○</span>Not connected</span></dd></div>
        <div><dt>Host</dt><dd><span class="mono">{connection.host}</span><span class="why">Configured for this deployment</span></dd></div>
      </dl>
      <p class="prose">GitHub handles App installation, your authorization and which repositories Wyrd may read on {connection.host}. When you finish there, GitHub returns you to Wyrd and the connection is verified.</p>
      <div class="toolbar">
        <button type="button" class="btn btn-primary" disabled={blocked(actions['github.connect'])} aria-busy={actions['github.connect']?.state === 'pending' ? 'true' : undefined} onclick={() => onaction?.({ name: 'github.connect', input: {} })}>Connect GitHub</button>
        <ActionNote state={actions['github.connect']} />
      </div>
    {:else}
      {@const lost = connection.state === 'suspended'}
      <dl class="facts">
        <div>
          <dt>State</dt>
          <dd>
            {#if lost}
              <span class="st c-danger"><span aria-hidden="true">✕</span>Access lost</span>
              <span class="why">GitHub reports the installation suspended or removed. New verification stopped; unfinished executions were cancelled.</span>
            {:else}
              <span class="st c-ok"><span aria-hidden="true">✓</span>Connected</span>
            {/if}
          </dd>
        </div>
        <div><dt>Installation</dt><dd><span class="mono">{connection.installation_id ?? '—'}</span><span class="why mono">App {connection.app.slug}</span></dd></div>
        <div><dt>Host</dt><dd><span class="mono">{connection.host}</span><span class="why">Configured for this deployment</span></dd></div>
      </dl>
      <div class="toolbar">
        <a class="btn btn-sm" href="https://{connection.host}/apps/{connection.app.slug}/installations/new" target="_blank" rel="noopener noreferrer">Manage access on GitHub ↗</a>
        <button type="button" class="btn btn-sm btn-danger" disabled={blocked(actions['github.disconnect'])} aria-busy={actions['github.disconnect']?.state === 'pending' ? 'true' : undefined} onclick={() => dialog?.showModal()}>Disconnect</button>
        <ActionNote state={actions['github.disconnect'] ?? actions['card.open'] ?? actions['repository.declare']} />
      </div>
      <section class="section" aria-labelledby="access-heading">
        <div class="section-head"><h2 class="h" id="access-heading">Repository access granted on GitHub</h2></div>
        {#if result.data.cards.state !== 'populated'}
          <p class="note box">Repository Cards unavailable: {result.data.cards.problem.title} ({result.data.cards.problem.code}). Registration is not shown.</p>
        {:else if join?.partial}
          <p class="note box">{join.partial}</p>
        {/if}
        {#if lost}
          <p class="note box">No repository access while the installation is suspended or removed. Registered Repository Cards and their history stay readable.</p>
        {:else}
          <div class="outs-wrap" role="region" aria-label="Repository access" tabindex="-1">
            <table class="outs">
              <thead>
                <tr><th scope="col">GitHub repository</th><th scope="col">repository_id</th><th scope="col">Repository Card (registration is separate)</th><th scope="col">Automatic verification</th></tr>
              </thead>
              <tbody>
                {#each connection.repositories as repository (repository.repository_id)}
                  {@const row = cardOf(repository.repository_id)}
                  {@const readiness = row ? row.readiness : join && !join.partial ? 'undeclared' : undefined}
                  <tr>
                    <td><div class="cellbox"><a class="mono" href="https://{connection.host}/{repository.full_name}" target="_blank" rel="noopener noreferrer">{repository.full_name} ↗</a></div></td>
                    <td><div class="cellbox"><span class="mono">{repository.repository_id}</span></div></td>
                    <td>
                      <div class="cellbox">
                        {#if row?.card}
                          {@const card = row.card}
                          <button type="button" class="link mono" disabled={!onaction} onclick={() => onaction?.({ name: 'card.open', input: { card_ref: { kind: 'Repository', name: card.name, version: card.opens } } })}>Repository/{card.name}@{card.opens}</button>
                        {:else if readiness === 'undeclared'}
                          <span class="ident"><span class="tag">no Card</span><button type="button" class="link small" disabled={!onaction} onclick={() => onaction?.({ name: 'repository.declare', input: { repository_id: repository.repository_id } })}>How to declare</button></span>
                        {:else}
                          <span class="muted mono">—</span>
                        {/if}
                      </div>
                    </td>
                    <td>
                      <div class="cellbox">
                        {#if readiness}
                          {@const fact = READINESS_WORDS[readiness]}
                          <span class="st c-{fact.tone}"><span aria-hidden="true">{fact.glyph}</span>{fact.word}</span>
                        {:else}
                          <span class="muted mono">—</span>
                        {/if}
                      </div>
                    </td>
                  </tr>
                {:else}
                  <tr><td colspan="4"><div class="cellbox"><span class="prose muted">The installation has no repository selected on GitHub.</span></div></td></tr>
                {/each}
              </tbody>
            </table>
          </div>
        {/if}
        <p class="note">Adding access on GitHub never creates a Repository Card or bindings. Removing access stops new verification for that repository and cancels its unfinished executions; recorded evidence stays.</p>
      </section>
      <dialog
        aria-labelledby="disconnect-title"
        bind:this={dialog}
        onclose={() => {
          if (dialog?.returnValue === 'confirm') onaction?.({ name: 'github.disconnect', input: {} });
          if (dialog) dialog.returnValue = '';
        }}
      >
        <form method="dialog" class="dlg">
          <h2 id="disconnect-title">Disconnect GitHub?</h2>
          <ul class="dlg-list">
            <li>
              {#if join}New verification stops for {affected.length} Repository Card{affected.length === 1 ? '' : 's'} with access{affected.length ? ` (${affected.join(', ')})` : ''}.{:else}New verification stops for every Repository Card.{/if}
            </li>
            <li>Unfinished executions for those repositories are cancelled.</li>
            <li>Recorded executions, verdicts and evidence stay. Repository Cards and versions are unchanged.</li>
          </ul>
          <p class="note">Uninstalling the App stays on GitHub.</p>
          <div class="dlg-actions">
            <!-- svelte-ignore a11y_autofocus -->
            <button class="btn" value="cancel" autofocus>Cancel</button>
            <button class="btn btn-danger" value="confirm">Disconnect GitHub</button>
          </div>
        </form>
      </dialog>
    {/if}
  {:else if result.state === 'loading'}
    <EmptyState state="loading" title="Loading the GitHub connection" />
  {:else if result.state === 'denied' || result.state === 'error'}
    <EmptyState state={result.state} problem={result.problem} />
  {/if}
</RepositorySheet>
