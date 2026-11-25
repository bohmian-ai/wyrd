<!--
  Settings → Secrets: one inventory of the secrets this workspace stores, each
  managed through the server that owns it. Values are write-only: a drawer
  sends a new one and nothing ever reads one back. An owner that cannot be
  read says so beside the inventory instead of looking empty.
-->
<script lang="ts">
  import { fmtInstant } from 'wyrd-ai-ui';
  import { page } from '$app/state';
  import { guidance } from '$lib/Entry.svelte';
  import { ICONS } from '$lib/icons';
  import Icon from '$lib/Icon.svelte';
  import Overlay from '$lib/Overlay.svelte';
  import Pill from '$lib/Pill.svelte';
  import type { Secret } from './+page.server';
  import type { PageProps } from './$types';

  let { data, form }: PageProps = $props();
  const tenantPath = $derived(`/t/${encodeURIComponent(data.tenant.key)}`);
  const base = $derived(`${tenantPath}/settings/secrets`);
  const rowId = (secret: Secret) => `${secret.owner}:${secret.key}`;
  const find = (param: string) => data.secrets.find((secret) => rowId(secret) === page.url.searchParams.get(param)) ?? null;
  const replacing = $derived(find('replace'));
  const revoking = $derived(find('confirm'));
  const adding = $derived(page.url.searchParams.get('drawer') === 'add');
  const denied = $derived(data.problems.length === 4 && data.problems.every(({ problem }) => problem.status === 403));
  const STATE = { active: ['ok', 'Active'], revoked: ['off', 'Revoked'], disabled: ['off', 'Disabled'], candidate: ['info', 'Not on yet'] } as const;
  const PROVIDERS = [
    ['openai', 'OpenAI'],
    ['anthropic', 'Anthropic'],
    ['gemini', 'Google Gemini'],
    ['vertex', 'Google Vertex']
  ] as const;
</script>

<svelte:head><title>Secrets · {data.tenant.name} · Wyrd</title></svelte:head>
<header class="pagehead">
  <div><h1>Secrets</h1><p>Credentials {data.tenant.name} stores for providers, Operators and sign-in. Values are write-only and never shown.</p></div>
  {#if !denied}<div class="actions"><a class="app-control primary lg" href="{base}?drawer=add"><Icon d={ICONS.plus} />Add provider key</a></div>{/if}
</header>

{#if form?.problem}
  <div class="app-problem" role="alert" data-state={form.problem.status === 403 ? 'unauthorized' : 'error'}>
    <strong>{form.problem.status === 403 ? '⊘' : '✕'} {form.problem.title}</strong>
    <p>{guidance(form.problem)} Nothing was changed.</p>
    <code>{form.problem.code}</code>
  </div>
{:else if form?.done}
  <div class="note ok" role="status"><span class="g" aria-hidden="true">✓</span><div class="grow">{form.done}</div></div>
{/if}

{#if denied}
  <div class="app-problem" data-state="unauthorized">
    <strong>⊘ You can't manage secrets in this workspace</strong>
    <p>This is for workspace administrators. Ask one to grant what you need.</p>
    <code>{data.problems[0].problem.code}</code>
  </div>
{:else}
  {#each data.problems as { owner, label, problem } (owner)}
    <div class="app-problem" role="alert" data-state={problem.status === 403 ? 'unauthorized' : 'error'}>
      <strong>{problem.status === 403 ? '⊘' : '✕'} {label} are not listed</strong>
      <p>{problem.status === 403 ? 'You may not read these. Ask a workspace administrator if you need them.' : guidance(problem)}</p>
      <code>{problem.code}</code>
    </div>
  {/each}

  <div class="stats">
    <div class="stat"><span class="v">{data.secrets.filter((secret) => secret.state === 'active').length}</span><span class="k">Active</span></div>
    <div class="stat"><span class="v">{data.secrets.filter((secret) => secret.state === 'revoked').length}</span><span class="k">Revoked</span></div>
  </div>

  <!-- svelte-ignore a11y_no_noninteractive_tabindex -- a scrolling table must be reachable by keyboard -->
  <div class="tablewrap" role="region" aria-label="Secrets" tabindex="0">
    <table>
      <thead><tr><th>Name</th><th>Used by</th><th>Last rotated</th><th>Status</th><th class="right"><span class="sr-only">Actions</span></th></tr></thead>
      <tbody>
        {#each data.secrets as secret (rowId(secret))}
          {@const [tone, label] = STATE[secret.state]}
          <tr>
            <td class="nowrap"><b>{secret.name}</b><br /><span class="muted small">{secret.kind}</span></td>
            <td>{secret.usedBy}</td>
            <td><span class="muted">{secret.rotated ? fmtInstant(Date.parse(secret.rotated)) : '—'}</span></td>
            <td><Pill {tone} {label} /></td>
            <td class="right nowrap">
              {#if secret.replace === 'stage'}
                <a class="app-control" href="{tenantPath}/settings/sign-in?setup=2">Replace</a>
              {:else if secret.replace}
                <a class="app-control" href="{base}?replace={encodeURIComponent(rowId(secret))}" data-sveltekit-noscroll>Replace</a>
              {/if}
              {#if secret.renew || secret.revoke}
                <details class="menu">
                  <summary class="app-control" aria-label="More actions for {secret.name}">⋯</summary>
                  <div class="menu-list">
                    {#if secret.renew}
                      <form method="POST" action="?/renew">
                        <input type="hidden" name="owner" value={secret.owner} />
                        <input type="hidden" name="key" value={secret.key} />
                        <button type="submit">Renew encryption<span>New data key; the value is unchanged</span></button>
                      </form>
                    {/if}
                    {#if secret.revoke}
                      <a class="danger" href="{base}?confirm={encodeURIComponent(rowId(secret))}" data-sveltekit-noscroll>Revoke…</a>
                    {/if}
                  </div>
                </details>
              {/if}
            </td>
          </tr>
        {:else}
          <tr><td colspan="5" class="muted">No secrets are stored{data.problems.length ? ' in the owners listed here' : ''}.</td></tr>
        {/each}
      </tbody>
    </table>
  </div>
{/if}

{#if adding}
  <Overlay kind="drawer" title="Add provider key" close={base}>
    <form id="add" method="POST" action="?/add">
      <div class="field"><label for="add-name">Name</label><input id="add-name" class="app-input mono" name="name" required autocomplete="off" /><span class="hint">Gateway deployments refer to the key by this name.</span></div>
      <div class="field">
        <label for="add-provider">Provider</label>
        <select id="add-provider" class="app-input" name="provider">
          {#each PROVIDERS as [value, label] (value)}<option {value}>{label}</option>{/each}
        </select>
      </div>
      <div class="field"><label for="add-value">API key</label><input id="add-value" class="app-input mono" type="password" name="value" required autocomplete="off" /><span class="hint">Encrypted before it is stored. Wyrd never shows it again.</span></div>
    </form>
    {#snippet footer()}
      <a class="app-control" href={base}>Cancel</a>
      <button class="app-control primary" type="submit" form="add">Save key</button>
    {/snippet}
  </Overlay>
{/if}

{#if replacing}
  <Overlay kind="drawer" title="Replace {replacing.name}" close={base}>
    <form id="replace" method="POST" action="?/replace">
      <input type="hidden" name="owner" value={replacing.owner} />
      <input type="hidden" name="key" value={replacing.key} />
      {#each Object.entries(replacing.detail) as [name, value] (name)}<input type="hidden" {name} {value} />{/each}
      <p class="muted">{replacing.kind} · {replacing.usedBy}</p>
      {#if replacing.replace === 'basic'}
        <div class="field"><label for="replace-username">Username</label><input id="replace-username" class="app-input mono" type="password" name="username" required autocomplete="off" /></div>
      {/if}
      <div class="field">
        <label for="replace-value">{replacing.replace === 'basic' ? 'Password' : replacing.field}</label>
        <input id="replace-value" class="app-input mono" type="password" name="value" required autocomplete="off" />
        <span class="hint">Replaces the stored value at once. Encrypted before it is stored; Wyrd never shows it again.</span>
      </div>
    </form>
    {#snippet footer()}
      <a class="app-control" href={base}>Cancel</a>
      <button class="app-control primary" type="submit" form="replace">Replace</button>
    {/snippet}
  </Overlay>
{/if}

{#if revoking?.revoke}
  <Overlay kind="dialog" title="Revoke {revoking.name}?" close={base}>
    <p>Gateway deployments that use {revoking.name} stop admitting new calls. Revoking cannot be undone: a revoked key cannot be replaced or turned back on.</p>
    <form id="revoke" method="POST" action="?/revoke"><input type="hidden" name="key" value={revoking.key} /></form>
    {#snippet footer()}
      <a class="app-control" href={base}>Cancel</a>
      <button class="app-control danger fill" type="submit" form="revoke">Revoke key</button>
    {/snippet}
  </Overlay>
{/if}

<style>
  .nowrap {
    white-space: nowrap;
  }
  .small {
    font-size: 12px;
  }
  .menu-list form {
    display: contents;
  }
  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
  }
  :global(.drawer) form {
    display: grid;
    gap: 14px;
  }
</style>
