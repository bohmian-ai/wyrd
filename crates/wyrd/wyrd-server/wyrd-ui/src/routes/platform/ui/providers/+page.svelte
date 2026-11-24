<script lang="ts">
  import { getContext } from 'svelte';
  import { enhance } from '$app/forms';
  import { invalidateAll } from '$app/navigation';
  import type { SubmitFunction } from '@sveltejs/kit';
  import ConsoleProblem from '$lib/ConsoleProblem.svelte';
  import type { ConsoleReauth } from '$lib/console-reauth.svelte';
  import { worded } from '$lib/platform-copy';
  import type { WyrdProblem } from '$lib/server/problem';
  import type { PageProps } from './$types';
  let { data, form }: PageProps = $props();
  const reauth = getContext<ConsoleReauth>('console-reauth');
  const names = { google: 'Google', github: 'GitHub' } as const;
  const states = { untested: '◌ untested · not offered', tested: '◐ tested · not offered', enabled: '✓ enabled · offered' } as const;
  let testing = $state<string | null>(null);
  let tested = $state('');
  const problem = $derived(form?.problem && form.problem.code !== 'WYRD_AUTH_403_RECENT_LOGIN_REQUIRED' ? worded(form.problem as WyrdProblem, 'conflict', 409) : null);
  // A newer form result replaces a finished test's notice.
  $effect(() => {
    void form;
    tested = '';
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

<p class="lead">Shared Google and GitHub.com sign-in for every workspace. Register the client in the provider console first. A saved client is a new, untested revision that no page offers until a real sign-in tests it and you enable it.</p>
{#if problem}<ConsoleProblem issue={problem} />{/if}
<div role="status" aria-live="polite">{#if notice}<p>✓ {notice}</p>{/if}</div>

{#each data.providers as p (p.provider)}
  <section aria-labelledby="provider-{p.provider}" data-testid="provider-{p.provider}">
    <h2 id="provider-{p.provider}">{names[p.provider]} <span class="state" data-testid="state-{p.provider}">{p.state ? states[p.state] : '– not configured'}</span></h2>
    <dl>
      <dt>Callback URL</dt><dd><code>{p.callback_url}</code></dd>
      <dt>Client ID</dt><dd><code>{p.client_id ?? '—'}</code></dd>
      <dt>Revision</dt><dd>{p.revision ?? '—'}</dd>
    </dl>
    <p>{p.registration_instructions}</p>
    <form class="stack" method="POST" action="?/save" use:enhance={reauth.guard(`Replacing the ${names[p.provider]} client`)}>
      <input type="hidden" name="provider" value={p.provider} />
      <label for="client-id-{p.provider}">Client ID</label>
      <input id="client-id-{p.provider}" class="app-input mono" name="client_id" required autocomplete="off" spellcheck="false" value={p.client_id ?? ''} />
      <label for="client-secret-{p.provider}">Client secret</label>
      <input id="client-secret-{p.provider}" class="app-input mono" name="client_secret" type="password" required autocomplete="off" />
      <p>{p.client_id ? 'The stored secret is never shown. Saving replaces the client and starts a new untested revision.' : 'Stored sealed and never shown again.'}</p>
      <div class="row">
        <button class="app-control" type="submit">Save client</button>
        {#if p.state}
          <button class="app-control" type="submit" form="test-{p.provider}" disabled={testing === p.provider} aria-busy={testing === p.provider ? 'true' : undefined}>{testing === p.provider ? `Signing in at ${names[p.provider]}…` : 'Test sign-in'}</button>
          <button class="app-control primary" type="submit" form="enabled-{p.provider}" disabled={p.state === 'untested'} aria-describedby={p.state === 'untested' ? `why-${p.provider}` : undefined}>{p.state === 'enabled' ? 'Disable' : 'Enable'}</button>
        {/if}
      </div>
      {#if p.state === 'untested'}<p id="why-{p.provider}">Test revision {p.revision} before enabling it.</p>{/if}
    </form>
    {#if p.state}
      <form id="test-{p.provider}" method="POST" action="?/test" use:enhance={test}>
        <input type="hidden" name="provider" value={p.provider} />
        <input type="hidden" name="revision" value={p.revision} />
      </form>
      <form id="enabled-{p.provider}" method="POST" action="?/enabled" use:enhance={reauth.guard(`${p.state === 'enabled' ? 'Disabling' : 'Enabling'} ${names[p.provider]}`)}>
        <input type="hidden" name="provider" value={p.provider} />
        <input type="hidden" name="revision" value={p.revision} />
        <input type="hidden" name="enabled" value={p.state !== 'enabled'} />
      </form>
    {/if}
  </section>
{/each}

<style>
  .lead {
    max-width: 720px;
  }
  .state {
    margin-left: 8px;
    font: 11px var(--font-mono);
    color: var(--muted);
  }
  [role='status']:empty {
    display: none;
  }
</style>
