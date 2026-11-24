<script lang="ts">
  import { getContext } from 'svelte';
  import { enhance } from '$app/forms';
  import ConsoleProblem from '$lib/ConsoleProblem.svelte';
  import type { ConsoleReauth } from '$lib/console-reauth.svelte';
  import type { PageProps } from './$types';
  let { data, form }: PageProps = $props();
  const reauth = getContext<ConsoleReauth>('console-reauth');
  const security = { starttls: 'STARTTLS', tls: 'TLS (SMTPS)', none: 'None' } as const;
  const problem = $derived(form?.problem?.code === 'WYRD_AUTH_403_RECENT_LOGIN_REQUIRED' ? null : form?.problem);
  const result = $derived(form && 'result' in form ? form.result : null);
</script>

{#if problem}<ConsoleProblem issue={problem} />{/if}
<div role="status" aria-live="polite">{#if form && 'notice' in form && form.notice}<p>✓ {form.notice}</p>{/if}</div>

<section aria-labelledby="relay">
  <h2 id="relay">SMTP relay</h2>
  {#if data.relay}
    <dl data-testid="relay">
      <dt>Host</dt><dd><code>{data.relay.host}:{data.relay.port}</code></dd>
      <dt>Security</dt><dd>{security[data.relay.tls]}</dd>
      <dt>Username</dt><dd>{data.relay.username ?? '—'}</dd>
      <dt>Password</dt><dd>{data.relay.password_set ? '✓ stored, never shown' : '– none'}</dd>
      <dt>Sender</dt><dd>{data.relay.sender}</dd>
    </dl>
  {:else}
    <p data-testid="no-relay">– No relay. Wyrd emails nothing: invitations are copied links, and email signup, finding workspaces by email, recovery email and reset requests are not offered.</p>
  {/if}
  <form class="stack" method="POST" action="?/save" use:enhance={reauth.guard('Replacing the mail relay')}>
    <label for="host">Host</label><input id="host" class="app-input mono" name="host" required autocomplete="off" defaultValue={data.relay?.host ?? ''} />
    <label for="port">Port</label><input id="port" class="app-input mono" name="port" type="number" min="1" max="65535" required defaultValue={data.relay?.port ?? 587} />
    <label for="tls">Security</label>
    <select id="tls" class="app-input" name="tls">
      {#each [['starttls', 'STARTTLS (usually port 587)'], ['tls', 'TLS from the first byte (usually port 465)'], ['none', 'None (trusted local network only)']] as [value, label] (value)}
        <option {value} selected={(data.relay?.tls ?? 'starttls') === value}>{label}</option>
      {/each}
    </select>
    <label for="relay-user">Username</label><input id="relay-user" class="app-input mono" name="username" autocomplete="off" defaultValue={data.relay?.username ?? ''} />
    <label for="relay-password">Password</label><input id="relay-password" class="app-input mono" name="password" type="password" autocomplete="off" />
    <p>The stored password is never shown. Leave it blank to keep it; a new username needs its password.</p>
    {#if data.relay?.username}
      <label class="check"><input type="checkbox" name="remove" /> Remove the stored username and password</label>
    {/if}
    <label for="sender">Sender address</label><input id="sender" class="app-input" name="sender" type="email" required autocomplete="off" defaultValue={data.relay?.sender ?? ''} />
    <button class="app-control primary" type="submit">Save relay</button>
  </form>
</section>
{#if data.relay}
  <section aria-labelledby="test">
    <h2 id="test">Send a test message</h2>
    <form class="stack" method="POST" action="?/test" use:enhance={reauth.guard('Sending a test message', false)}>
      <label for="recipient">Recipient</label><input id="recipient" class="app-input" name="recipient" type="email" required />
      <button class="app-control" type="submit">Send test</button>
    </form>
    <div role="status">
      {#if result?.accepted}
        <p data-testid="test-result">✓ The relay accepted the message. That does not prove it reached the inbox.</p>
      {:else if result}
        <p data-testid="test-result"><span class="bad">✕</span> The relay did not accept the message. <code>{result.error}</code></p>
      {/if}
    </div>
  </section>
{/if}

<style>
  [role='status']:empty {
    display: none;
  }
</style>
