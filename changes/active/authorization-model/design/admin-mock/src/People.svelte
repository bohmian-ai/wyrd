<script>
  import Pill from './Pill.svelte';
  import Mark from './Mark.svelte';
  import Icon from './Icon.svelte';
  import { I } from './icons.js';
  import { USERS, INVITES, ROLES } from './data.js';
  let { query } = $props();
  const base = '#/t/gtest/settings/people';
  const filter = $derived(query.get('filter') ?? 'all');
  const drawer = $derived(query.get('drawer'));
  const person = $derived(USERS.find((u) => u.id === drawer));
  const active = USERS.filter((u) => u.status === 'active').length;
  const suspended = USERS.filter((u) => u.status === 'suspended').length;
  const rows = $derived(
    filter === 'invited' ? [] : filter === 'suspended' ? USERS.filter((u) => u.status === 'suspended') : USERS,
  );
  const label = (u) => u.name ?? u.email;
  const initials = (u) => (u.name ? u.name.split(/\s+/).map((p) => p[0]).join('') : u.email).slice(0, 2).toUpperCase();
</script>

<header class="pagehead">
  <div>
    <h1>People</h1>
    <p>Who can sign in to gtest, how they sign in, and what they can do.</p>
  </div>
  <div class="actions"><a class="app-control primary lg" href="{base}?drawer=invite"><Icon d={I.plus} />Invite people</a></div>
</header>

{#if query.get('sent')}
  <div class="note ok" role="status">
    <span class="g">✓</span>
    <div class="grow"><b>2 invitations sent.</b> <span class="muted">Each link works once and expires in 24 hours. Copy them now if email is off; they are not shown again.</span></div>
    <button class="app-control">Copy links</button>
  </div>
{/if}

<div class="stats">
  <a class="stat" href={base}><span class="v">{active}</span><span class="k">Active</span></a>
  <a class="stat" href="{base}?filter=invited"><span class="v">{INVITES.length}</span><span class="k">Invited · pending</span></a>
  <a class="stat warn" href="{base}?filter=invited"><span class="v">1</span><span class="k">Invitation not delivered</span></a>
  <a class="stat" href="{base}?filter=suspended"><span class="v">{suspended}</span><span class="k">Suspended</span></a>
</div>

<div class="toolbar">
  <div class="seg" role="group" aria-label="Filter">
    <a href={base} aria-current={filter === 'all' ? 'true' : undefined}>Members</a>
    <a href="{base}?filter=invited" aria-current={filter === 'invited' ? 'true' : undefined}>Invited ({INVITES.length})</a>
    <a href="{base}?filter=suspended" aria-current={filter === 'suspended' ? 'true' : undefined}>Suspended</a>
  </div>
  <input class="app-input" placeholder="Filter by name or email" aria-label="Filter people" />
</div>

<div class="tablewrap">
  {#if filter === 'invited'}
    <table>
      <thead><tr><th>Email</th><th>Will get</th><th>Delivery</th><th>Expires</th><th class="right"></th></tr></thead>
      <tbody>
        {#each INVITES as inv (inv.id)}
          <tr>
            <td>{inv.email}</td>
            <td>{#each inv.roles as r (r)}<span class="role">{r}</span>{/each}</td>
            <td>{#if inv.delivery === 'failed'}<Pill tone="warn">Not delivered</Pill>{:else}<Pill tone="info">Emailed</Pill>{/if}</td>
            <td class="muted">{inv.expires}</td>
            <td class="right"><button class="app-control">Resend</button> <button class="app-control">Revoke</button></td>
          </tr>
        {/each}
      </tbody>
    </table>
  {:else}
    <table>
      <thead><tr><th>Person</th><th>Signs in with</th><th>Roles</th><th>Status</th><th>Last sign-in</th></tr></thead>
      <tbody>
        {#each rows as u (u.id)}
          <tr class="link" onclick={() => (location.hash = `${base}?drawer=${u.id}`)}>
            <td><a class="person" href="{base}?drawer={u.id}" style="text-decoration:none"><span class="avatar">{initials(u)}</span><span><b>{label(u)}{u.id === 'u1' ? ' (you)' : ''}</b><span>{u.email}</span></span></a></td>
            <td><span class="method"><Mark method={u.method} />{u.method}</span></td>
            <td>{#each u.roles as r (r)}<span class="role">{r}</span>{/each}</td>
            <td>{#if u.status === 'active'}<Pill tone="ok">Active</Pill>{:else}<Pill tone="off">Suspended</Pill>{/if}</td>
            <td class="muted">{u.last}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</div>

{#if drawer === 'invite'}
  <a class="scrim" href={base} aria-label="Close"></a>
  <aside class="drawer" aria-label="Invite people">
    <header><h2>Invite people</h2><a class="iconbtn" href={base} aria-label="Close">✕</a></header>
    <div class="body">
      <div class="field">
        <label for="emails">Email addresses</label>
        <textarea id="emails" class="app-input" rows="4" placeholder="dana@example.test, omar@example.test"></textarea>
        <span class="hint">Separate with commas or new lines. Each person gets their own link, valid for 24 hours.</span>
      </div>
      <div class="field">
        <span style="font-size:13px;font-weight:var(--weight-strong)">Role</span>
        {#each ROLES.filter((r) => r.name !== 'workload') as r (r.name)}
          <label class="check" style="padding:10px 12px;border:1px solid {r.name === 'viewer' ? 'var(--declare)' : 'var(--border)'};border-radius:var(--r)">
            <input type="radio" name="role" checked={r.name === 'viewer'} />
            <span><b>{r.name}</b><br /><span class="muted">{r.summary}</span></span>
          </label>
        {/each}
      </div>
      <div class="field">
        <label for="scope">Applies to</label>
        <select id="scope" class="app-input"><option>The whole workspace</option><option>One Card…</option><option>One repository…</option></select>
      </div>
      <p class="muted" style="font-size:12px">They choose how to sign in when they accept. Add more roles from their profile later.</p>
    </div>
    <footer><a class="app-control" href={base}>Cancel</a><a class="app-control primary" href="{base}?sent=1">Send invitations</a></footer>
  </aside>
{:else if person}
  <a class="scrim" href={base} aria-label="Close"></a>
  <aside class="drawer" aria-label={label(person)}>
    <header><span class="avatar">{initials(person)}</span><div><h2>{label(person)}</h2><span class="muted" style="font-size:13px">{person.email}</span></div><a class="iconbtn" href={base} aria-label="Close">✕</a></header>
    <div class="body">
      <dl class="dl">
        <dt>Status</dt><dd><Pill tone="ok">Active</Pill></dd>
        <dt>Signs in with</dt><dd><span class="method"><Mark method={person.method} />{person.method}</span></dd>
        <dt>Last sign-in</dt><dd>{person.last}</dd>
      </dl>
      <section class="card" style="margin:0">
        <header><h2>Roles</h2><div class="right"><button class="app-control"><Icon d={I.plus} size={14} />Add role</button></div></header>
        {#each person.roles as r (r)}
          <div class="row"><div class="label"><b>{r}</b><span>Whole workspace</span></div><div class="ctl"><button class="app-control">Remove</button></div></div>
        {/each}
      </section>
      <section class="card" style="margin:0">
        <header><h2>API keys</h2><div class="right"><button class="app-control"><Icon d={I.plus} size={14} />Issue key</button></div></header>
        <div class="row"><div class="label"><b class="mono">wyrd_k7f3…</b><span>Created 12 days ago · used 2 h ago</span></div><div class="ctl"><button class="app-control">Revoke</button></div></div>
      </section>
      <section class="danger-zone">
        <header><h2>Danger zone</h2></header>
        <div class="row"><div class="label"><b>Suspend {label(person)}</b><span>Ends their sessions and keys now. Their roles return if you reinstate them.</span></div><div class="ctl"><button class="app-control danger">Suspend</button></div></div>
      </section>
    </div>
  </aside>
{/if}
