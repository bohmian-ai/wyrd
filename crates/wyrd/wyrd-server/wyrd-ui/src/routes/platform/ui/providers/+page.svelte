<script lang="ts">
  import { getContext } from 'svelte';
  import { enhance } from '$app/forms';
  import { invalidateAll } from '$app/navigation';
  import type { SubmitFunction } from '@sveltejs/kit';
  import ConsoleProblem from '$lib/ConsoleProblem.svelte';
  import Icon from '$lib/Icon.svelte';
  import Pill from '$lib/Pill.svelte';
  import { ICONS } from '$lib/icons';
  import type { ConsoleReauth } from '$lib/console-reauth.svelte';
  import { worded } from '$lib/platform-copy';
  import type { WyrdProblem } from '$lib/server/problem';
  import type { PageProps } from './$types';
  let { data, form }: PageProps = $props();
  const reauth = getContext<ConsoleReauth>('console-reauth');
  const names = { google: 'Google', github: 'GitHub' } as const;
  const states = {
    untested: { tone: 'warn', label: 'Untested' },
    tested: { tone: 'info', label: 'Tested · off' },
    enabled: { tone: 'ok', label: 'On' }
  } as const;
  /** The provider whose client form is open; an unconfigured provider always shows it. */
  let replacing = $state<string | null>(null);
  let copied = $state<string | null>(null);
  async function copy(provider: string, url: string) {
    await navigator.clipboard.writeText(url);
    copied = provider;
  }
  let testing = $state<string | null>(null);
  let tested = $state('');
  const problem = $derived(form?.problem && form.problem.code !== 'WYRD_AUTH_403_RECENT_LOGIN_REQUIRED' ? worded(form.problem as WyrdProblem, 'conflict', 409) : null);
  // A newer form result replaces a finished test's notice.
  $effect(() => {
    void form;
    tested = '';
    replacing = null;
  });
  const notice = $derived(tested || form?.notice);

  /**
   * Run a provider test in a pop-up opened by the click, then re-read the
   * providers until the pop-up closes. The result counts only while the
   * provider still shows the revision the test ran against.
   */
  const test: SubmitFunction = ({ formData }) => {
    const name = formData.get('provider') as keyof typeof names;
    const revision = Number(formData.get('revision'));
    const popup = window.open('', `wyrd-provider-test-${name}`, 'popup,width=520,height=680');
    testing = name;
    tested = '';
    return async ({ result, update }) => {
      if (result.type !== 'success' || !popup) {
        popup?.close();
        testing = null;
        return update();
      }
      try {
        popup.location.assign((result.data?.test as { authorization_url: string }).authorization_url);
        for (;;) {
        await new Promise((resolve) => setTimeout(resolve, 1000));
        await invalidateAll();
        const view = data.providers.find((each) => each.provider === name);
        if (view?.revision !== revision) break;
        if (view.state !== 'untested') {
          tested = `Signed in at ${names[name]}. Revision ${revision} is tested${view.state === 'tested' ? '; enable it to offer it' : ''}.`;
          break;
        }
        if (popup.closed) break;
        }
      } finally {
        popup.close();
        testing = null;
      }
    };
  };
</script>

<header class="pagehead"><div><h1>Sign-in providers</h1><p>Google and GitHub sign-in, shared by every workspace on this deployment.</p></div></header>

{#if problem}<ConsoleProblem issue={problem} />{/if}
<div role="status" aria-live="polite">{#if notice}<p class="done" data-testid="notice"><span class="ok">✓</span> {notice}</p>{/if}</div>

{#each data.providers as p (p.provider)}
  {@const state = p.state ? states[p.state] : { tone: 'off', label: 'Not set up' } as const}
  <section class="card" aria-labelledby="provider-{p.provider}" data-testid="provider-{p.provider}">
    <header>
      <h2 id="provider-{p.provider}">{names[p.provider]}</h2>
      <span data-testid="state-{p.provider}"><Pill tone={state.tone} label={state.label} /></span>
      {#if p.state}
        <div class="right">
          <button class="app-control" type="submit" form="test-{p.provider}" disabled={testing === p.provider} aria-busy={testing === p.provider ? 'true' : undefined}>{testing === p.provider ? `Signing in at ${names[p.provider]}…` : 'Test sign-in'}</button>
          <button class="app-control" class:primary={p.state === 'tested'} type="submit" form="enabled-{p.provider}" disabled={p.state === 'untested'} aria-describedby={p.state === 'untested' ? `why-${p.provider}` : undefined}>{p.state === 'enabled' ? 'Turn off' : 'Turn on'}</button>
        </div>
      {/if}
    </header>
    <div class="row">
      <div class="label"><b>Callback URL</b><span>{p.registration_instructions}</span></div>
      <div class="ctl copyfield">
        <input class="app-input mono" readonly value={p.callback_url} aria-label="{names[p.provider]} callback URL" />
        <button class="app-control" type="button" onclick={() => copy(p.provider, p.callback_url)}><Icon d={ICONS.copy} size={14} />{copied === p.provider ? 'Copied' : 'Copy'}</button>
      </div>
    </div>
    {#if p.client_id && replacing !== p.provider}
      <div class="row">
        <div class="label">
          <b>Client</b>
          <span id="why-{p.provider}">Revision {p.revision}{p.state === 'untested' ? `. Test revision ${p.revision} before turning it on.` : '.'}</span>
        </div>
        <div class="ctl"><span class="mono">{p.client_id}</span><button class="app-control" type="button" onclick={() => (replacing = p.provider)}>Replace</button></div>
      </div>
    {:else}
      <form class="row" method="POST" action="?/save" use:enhance={reauth.guard(`Replacing the ${names[p.provider]} client`)}>
        <input type="hidden" name="provider" value={p.provider} />
        <div class="field grow">
          <label for="client-id-{p.provider}">Client ID</label>
          <input id="client-id-{p.provider}" class="app-input mono" name="client_id" required autocomplete="off" spellcheck="false" defaultValue={p.client_id ?? ''} />
          <label for="client-secret-{p.provider}">Client secret</label>
          <input id="client-secret-{p.provider}" class="app-input mono" name="client_secret" type="password" required autocomplete="off" />
          <span class="hint">{p.client_id ? 'The stored secret is never shown. Saving starts a new untested revision.' : 'Stored sealed and never shown again. A saved client stays off until a real sign-in tests it.'}</span>
        </div>
        <div class="ctl">
          {#if p.client_id}<button class="app-control" type="button" onclick={() => (replacing = null)}>Cancel</button>{/if}
          <button class="app-control primary" type="submit">Save client</button>
        </div>
      </form>
    {/if}
    {#if p.state}
      <form id="test-{p.provider}" hidden method="POST" action="?/test" use:enhance={test}>
        <input type="hidden" name="provider" value={p.provider} />
        <input type="hidden" name="revision" value={p.revision} />
      </form>
      <form id="enabled-{p.provider}" hidden method="POST" action="?/enabled" use:enhance={reauth.guard(`${p.state === 'enabled' ? 'Turning off' : 'Turning on'} ${names[p.provider]}`)}>
        <input type="hidden" name="provider" value={p.provider} />
        <input type="hidden" name="revision" value={p.revision} />
        <input type="hidden" name="enabled" value={p.state !== 'enabled'} />
      </form>
    {/if}
  </section>
{/each}

<style>
  .grow {
    flex: 1;
    max-width: 480px;
  }
  form.row {
    align-items: flex-end;
  }
</style>
