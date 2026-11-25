<script lang="ts">
  import { getContext } from 'svelte';
  import { enhance } from '$app/forms';
  import type { SubmitFunction } from '@sveltejs/kit';
  import ConsoleProblem from '$lib/ConsoleProblem.svelte';
  import Pill from '$lib/Pill.svelte';
  import type { ConsoleReauth } from '$lib/console-reauth.svelte';
  import type { PageProps } from './$types';
  let { data, form }: PageProps = $props();
  const reauth = getContext<ConsoleReauth>('console-reauth');
  const security = { starttls: 'STARTTLS', tls: 'TLS (SMTPS)', none: 'None' } as const;
  const problem = $derived(form?.problem?.code === 'WYRD_AUTH_403_RECENT_LOGIN_REQUIRED' ? null : form?.problem);
  const result = $derived(form && 'result' in form ? form.result : null);
  let sending = $state(false);
  const guarded = reauth.guard('Sending a test message', false);
  /** The test form's guard, showing the send as busy until Rust answers. */
  const send: SubmitFunction = (input) => {
    sending = true;
    return async (outcome) => {
      sending = false;
      await (await guarded(input))?.(outcome);
    };
  };
</script>

<header class="pagehead"><div><h1>Mail</h1><p>The relay Wyrd uses for invitations, sign-up codes and password resets.</p></div></header>

{#if problem}<ConsoleProblem issue={problem} />{/if}
<div role="status" aria-live="polite">{#if form && 'notice' in form && form.notice}<p class="done" data-testid="notice"><span class="ok">✓</span> {form.notice}</p>{/if}</div>

<section class="card" aria-labelledby="relay">
  <header><h2 id="relay">SMTP relay</h2>{#if !data.relay}<Pill tone="off" label="Not set up" />{/if}</header>
  {#if data.relay}
    <div data-testid="relay">
      <div class="row"><div class="label"><b>Host and port</b></div><div class="ctl mono">{data.relay.host}:{data.relay.port}</div></div>
      <div class="row"><div class="label"><b>Sender</b></div><div class="ctl mono">{data.relay.sender}</div></div>
      <div class="row">
        <div class="label"><b>TLS</b>{#if data.relay.tls === 'none'}<span>No TLS is only for a relay on the same host.</span>{/if}</div>
        <div class="ctl"><Pill tone={data.relay.tls === 'none' ? 'warn' : 'ok'} label={security[data.relay.tls]} /></div>
      </div>
      <div class="row"><div class="label"><b>Username</b></div><div class="ctl mono">{data.relay.username ?? '—'}</div></div>
      <div class="row"><div class="label"><b>Password</b></div><div class="ctl">{data.relay.password_set ? '✓ stored, never shown' : '– none'}</div></div>
    </div>
  {:else}
    <div class="row" data-testid="no-relay"><div class="label"><span>– No relay. Wyrd emails nothing: invitations are copied links, and email signup, finding workspaces by email, recovery email and reset requests are not offered.</span></div></div>
  {/if}
</section>

<section class="card" aria-labelledby="change">
  <header><h2 id="change">{data.relay ? 'Replace the relay' : 'Set up a relay'}</h2></header>
  <form class="row" method="POST" action="?/save" use:enhance={reauth.guard('Replacing the mail relay')}>
    <div class="field grow">
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
      <span class="hint">The stored password is never shown. Leave it blank to keep it; a new username needs its password.</span>
      {#if data.relay?.username}
        <label class="check"><input type="checkbox" name="remove" /> Remove the stored username and password</label>
      {/if}
      <label for="sender">Sender address</label><input id="sender" class="app-input" name="sender" type="email" required autocomplete="off" defaultValue={data.relay?.sender ?? ''} />
    </div>
    <div class="ctl"><button class="app-control primary" type="submit">Save relay</button></div>
  </form>
</section>

{#if data.relay}
  <section class="card" aria-labelledby="test">
    <header><h2 id="test">Send test email</h2></header>
    <form class="row" method="POST" action="?/test" use:enhance={send}>
      <div class="field grow"><label for="recipient">Recipient</label><input id="recipient" class="app-input" name="recipient" type="email" required /></div>
      <div class="ctl"><button class="app-control" type="submit" disabled={sending} aria-busy={sending ? 'true' : undefined}>{sending ? 'Sending…' : 'Send test'}</button></div>
    </form>
    <div role="status">
      {#if result?.accepted}
        <p class="row" data-testid="test-result"><span class="ok">✓</span> The relay accepted the message. That does not prove it reached the inbox.</p>
      {:else if result}
        <p class="row" data-testid="test-result"><span class="bad">✕</span> The relay did not accept the message. <code>{result.error}</code></p>
      {/if}
    </div>
  </section>
{/if}

<style>
  .grow {
    flex: 1;
    max-width: 480px;
  }
  form.row {
    align-items: flex-end;
  }
  .check {
    display: flex;
    gap: 8px;
    align-items: center;
    font-weight: 400;
  }
  [data-testid='relay'] > .row:first-child {
    border-top: 0;
  }
</style>
