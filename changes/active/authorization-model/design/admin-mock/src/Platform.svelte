<!-- Platform console: the same shell and page pattern as tenant Admin. -->
<script>
  import Pill from './Pill.svelte';
  import Icon from './Icon.svelte';
  import { I } from './icons.js';
  import { WORKSPACES } from './data.js';
  let { view } = $props();
</script>

<main class="work">
  {#if view === ''}
    <header class="pagehead"><div><h1>Overview</h1><p>The state of this Wyrd deployment.</p></div></header>
    <div class="stats">
      <a class="stat" href="#/platform/ui/workspaces"><span class="v">3</span><span class="k">Active workspaces</span></a>
      <a class="stat" href="#/platform/ui/providers"><span class="v">2 of 2</span><span class="k">Sign-in providers on</span></a>
      <a class="stat" href="#/platform/ui/mail"><span class="v">✓</span><span class="k">Mail relay tested</span></a>
      <a class="stat warn" href="#/platform/ui/secrets"><span class="v">!</span><span class="k">Rewrap pending after key activation</span></a>
    </div>
    <section class="card">
      <header><h2>Sign-up</h2></header>
      <div class="row"><div class="label"><b>Allow new workspaces</b><span>Anyone can create a workspace from the sign-in page.</span></div><div class="ctl"><label class="switch"><input type="checkbox" role="switch" checked />On</label></div></div>
      <div class="row"><div class="label"><b>Workspaces per person</b><span>How many workspaces one email can create.</span></div><div class="ctl"><input class="app-input" style="width:80px" value="3" aria-label="Workspaces per person" /></div></div>
      <div class="row"><div class="label"><b>Password sign-in</b><span>Offer username and password on every workspace's sign-in page.</span></div><div class="ctl"><label class="switch"><input type="checkbox" role="switch" checked />On</label></div></div>
    </section>
  {:else if view === 'workspaces'}
    <header class="pagehead"><div><h1>Workspaces</h1><p>Every tenant on this deployment.</p></div><div class="actions"><button class="app-control primary lg"><Icon d={I.plus} />New workspace</button></div></header>
    <div class="tablewrap"><table>
      <thead><tr><th>Workspace</th><th>Key</th><th class="right">People</th><th>Created</th><th>Status</th></tr></thead>
      <tbody>{#each WORKSPACES as w (w.slug)}<tr class="link"><td><b>{w.name}</b></td><td class="mono">{w.slug}</td><td class="right">{w.users}</td><td class="muted">{w.created}</td><td>{#if w.status === 'active'}<Pill tone="ok">Active</Pill>{:else}<Pill tone="off">Suspended</Pill>{/if}</td></tr>{/each}</tbody>
    </table></div>
  {:else if view === 'providers'}
    <header class="pagehead"><div><h1>Sign-in providers</h1><p>Google and GitHub sign-in, shared by every workspace on this deployment.</p></div></header>
    {#each [['Google', 'google'], ['GitHub', 'github']] as [name, key] (key)}
      <section class="card">
        <header><h2>{name}</h2><Pill tone="ok">On</Pill><div class="right"><button class="app-control">Test sign-in</button><button class="app-control">Turn off</button></div></header>
        <div class="row"><div class="label"><b>Callback URL</b><span>Paste into the {name} OAuth app's {key === 'github' ? 'Authorization callback URL' : 'Authorized redirect URIs'}.</span></div><div class="ctl copyfield"><input class="app-input mono" readonly value="http://localhost:8080/auth/social/{key}/callback" aria-label="{name} callback URL" /><button class="app-control"><Icon d={I.copy} size={14} />Copy</button></div></div>
        <div class="row"><div class="label"><b>Client</b><span>Tested at revision 1, today.</span></div><div class="ctl"><span class="mono">{key === 'google' ? '1234-abc.apps.googleusercontent.com' : 'Ov23li…'}</span><button class="app-control">Replace</button></div></div>
      </section>
    {/each}
  {:else if view === 'mail'}
    <header class="pagehead"><div><h1>Mail</h1><p>The relay Wyrd uses for invitations, sign-up codes and password resets.</p></div><div class="actions"><button class="app-control lg">Send test email</button></div></header>
    <section class="card">
      <header><h2>SMTP relay</h2><Pill tone="ok">Tested</Pill></header>
      <div class="row"><div class="label"><b>Host and port</b></div><div class="ctl mono">127.0.0.1:35518</div></div>
      <div class="row"><div class="label"><b>Sender</b></div><div class="ctl mono">wyrd@example.test</div></div>
      <div class="row"><div class="label"><b>TLS</b><span>No TLS is only for a relay on the same host.</span></div><div class="ctl"><Pill tone="warn">None</Pill></div></div>
    </section>
  {:else if view === 'admins'}
    <header class="pagehead"><div><h1>Admins</h1><p>People who can change this deployment.</p></div><div class="actions"><button class="app-control primary lg"><Icon d={I.plus} />Add admin</button></div></header>
    <div class="tablewrap"><table>
      <thead><tr><th>Name</th><th>Signs in with</th><th>Status</th></tr></thead>
      <tbody><tr><td><b>operator</b> <span class="muted">(you)</span></td><td>Password</td><td><Pill tone="ok">Active</Pill></td></tr></tbody>
    </table></div>
  {:else}
    <header class="pagehead"><div><h1>Stored secrets</h1><p>The deployment key that protects every workspace's secrets.</p></div><div class="actions"><button class="app-control primary lg">Run maintenance</button></div></header>
    <div class="note warn"><span class="g">!</span><div class="grow"><b>Rewrap pending.</b> <span class="muted">A new wrapping key is active. 214 secrets still use the previous key; run maintenance to move them.</span></div></div>
    <section class="card">
      <div class="row"><div class="label"><b>Active wrapping key</b><span>File key · activated today</span></div><div class="ctl mono">kms/v2</div></div>
      <div class="row"><div class="label"><b>Last maintenance run</b><span>Routine rewrap · 12 days ago · 0 failures</span></div><div class="ctl"><Pill tone="ok">Complete</Pill></div></div>
    </section>
  {/if}
</main>
