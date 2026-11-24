<!-- Sign-in: methods first, company SSO as a guided setup, the policy as a switch with an impact confirmation. -->
<script>
  import Pill from './Pill.svelte';
  import Mark from './Mark.svelte';
  import Icon from './Icon.svelte';
  import { I } from './icons.js';
  let { query } = $props();
  const base = '#/t/gtest/settings/sign-in';
  const active = $derived(query.get('sso') === 'active');
  const required = $derived(query.get('required') === '1');
  const step = $derived(Number(query.get('setup') ?? 0));
  const confirming = $derived(query.get('confirm') === 'require');
  const STEPS = ['Register Wyrd with your provider', 'Enter the client details', 'Test sign-in', 'Turn it on'];
</script>

<header class="pagehead">
  <div><h1>Sign-in</h1><p>How people get into gtest.</p></div>
  {#if !active && !step}<div class="actions"><a class="app-control primary lg" href="{base}?setup=1">Set up company SSO</a></div>{/if}
</header>

{#if required}
  <div class="note ok" role="status"><span class="g">✓</span><div class="grow"><b>Company SSO is required.</b> <span class="muted">Password, Google and GitHub sign-in are off for gtest. Their sessions were ended.</span></div></div>
{/if}

<div class="methods">
  <div class="method-tile"><div class="top"><Mark method="Google" size={18} />Google <Pill tone={required ? 'off' : 'ok'}>{required ? 'Off' : 'On'}</Pill></div><p>{required ? 'Turned off by Require company SSO.' : 'Offered by this deployment.'}</p></div>
  <div class="method-tile"><div class="top"><Mark method="GitHub" size={18} />GitHub <Pill tone={required ? 'off' : 'ok'}>{required ? 'Off' : 'On'}</Pill></div><p>{required ? 'Turned off by Require company SSO.' : 'Offered by this deployment.'}</p></div>
  <div class="method-tile"><div class="top"><Mark method="Password" size={18} />Password <Pill tone={required ? 'off' : 'ok'}>{required ? 'Off' : 'On'}</Pill></div><p>{required ? 'Turned off by Require company SSO.' : 'Username and password.'}</p></div>
  <div class="method-tile focus"><div class="top"><Mark method="Company" size={18} />Company SSO {#if active}<Pill tone="ok">Active</Pill>{:else}<Pill tone="off">Not set up</Pill>{/if}</div><p>{active ? 'Okta · okta.example.test' : 'Your identity provider, such as Okta, Entra ID or Google Workspace.'}</p></div>
</div>

{#if step}
  <section class="card">
    <header><h2>Set up company SSO</h2><div class="right"><a class="app-control" href={base}>Cancel</a></div></header>
    <ol class="steps" style="list-style:none;margin:0;padding:0">
      {#each STEPS as label, i (label)}
        <li class="step" class:done={i + 1 < step} class:now={i + 1 === step}><span class="n">{i + 1 < step ? '✓' : i + 1}</span>{label}</li>
      {/each}
    </ol>
    {#if step === 1}
      <div class="stepbody">
        <p>In your identity provider, create a <b>Web application</b> and paste this as its redirect URI.</p>
        <div class="field"><label for="cb">Redirect URI</label><div class="copyfield"><input id="cb" class="app-input mono" readonly value="http://localhost:8080/auth/callback" /><button class="app-control"><Icon d={I.copy} size={14} />Copy</button></div></div>
        <div class="buttons"><a class="app-control primary" href="{base}?setup=2">I've registered it</a></div>
      </div>
    {:else}
      <div class="stepbody">
        <div class="field"><label for="iss">Issuer URL</label><input id="iss" class="app-input mono" value="https://okta.example.test" /><span class="hint">Wyrd reads the provider's settings from this address.</span></div>
        <div class="field"><label for="cid">Client ID</label><input id="cid" class="app-input mono" value="0oa1b2c3d4" /></div>
        <div class="field"><label for="sec">Client secret</label><input id="sec" class="app-input mono" type="password" value="••••••••••" /><span class="hint">Stored encrypted. Wyrd never shows it again.</span></div>
        <div class="buttons"><a class="app-control" href="{base}?setup=1">Back</a><a class="app-control primary" href="{base}?setup=3">Save and test sign-in</a></div>
      </div>
    {/if}
  </section>
{/if}

<section class="card">
  <header><h2>Require company SSO</h2></header>
  <div class="row">
    <div class="label">
      <b>Only allow sign-in through your company</b>
      <span>{active ? 'Turns off password, Google and GitHub sign-in for gtest and ends those sessions.' : 'Available once company SSO is active.'}</span>
    </div>
    <div class="ctl">
      <label class="switch">
        <input type="checkbox" role="switch" checked={required} disabled={!active} onchange={() => (location.hash = required ? `${base}?sso=active` : `${base}?sso=active&confirm=require`)} />
        {required ? 'On' : 'Off'}
      </label>
    </div>
  </div>
  {#if active && !required}
    <div class="row"><div class="note warn" style="margin:0;flex:1"><span class="g">!</span><div class="grow">4 people have no company sign-in yet and would be locked out. <a href="#/t/gtest/settings/people">See who</a></div></div></div>
  {/if}
</section>

{#if active}
  <section class="danger-zone">
    <header><h2>Danger zone</h2></header>
    <div class="row"><div class="label"><b>Deactivate company SSO</b><span>People who sign in with Okta can no longer get in. Turn off Require company SSO first.</span></div><div class="ctl"><button class="app-control danger" disabled={required}>Deactivate</button></div></div>
  </section>
{/if}

{#if confirming}
  <a class="scrim" href="{base}?sso=active" aria-label="Cancel"></a>
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="req-title">
    <div class="body">
      <h2 id="req-title">Require company SSO?</h2>
      <p>This takes effect immediately:</p>
      <ul style="display:grid;gap:6px;padding-left:18px;list-style:disc">
        <li>Password, Google and GitHub sign-in stop for gtest.</li>
        <li><b>4 people</b> without an Okta account are locked out until they sign in with Okta. Matching email does not link accounts.</li>
      </ul>
      <p class="muted" style="font-size:13px">Confirms that you signed in with Okta in the last 5 minutes.</p>
      <label class="check"><input type="checkbox" checked /> I understand 4 people will lose access</label>
    </div>
    <footer><a class="app-control" href="{base}?sso=active">Cancel</a><a class="app-control danger fill" href="{base}?sso=active&required=1">Require company SSO</a></footer>
  </div>
{/if}
