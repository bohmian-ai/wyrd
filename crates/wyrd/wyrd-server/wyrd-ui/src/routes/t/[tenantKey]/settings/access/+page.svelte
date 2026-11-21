<!--
  Settings → Access: tenant administrators manage Users, Service accounts,
  roles, optional groups and their scoped bindings. Every list, summary and
  refusal is the server's; the page words them and submits forms.
-->
<script lang="ts">
  import { enhance } from '$app/forms';
  import { page } from '$app/state';
  import type { CardTypes } from 'wyrd-ai';
  import { OPERATIONS, operationName, scopeLabel } from '$lib/access';
  import { guidance } from '$lib/Entry.svelte';
  import AccessRequest from '$lib/AccessRequest.svelte';
  import GrantFields from './GrantFields.svelte';
  import type { PageProps } from './$types';
  let { data, form }: PageProps = $props();

  const TABS = [
    ['users', 'Users'],
    ['services', 'Service accounts'],
    ['roles', 'Roles'],
    ['groups', 'Groups']
  ] as const;
  const tab = $derived(TABS.find(([key]) => key === page.url.searchParams.get('tab'))?.[0] ?? 'users');
  const access = $derived(data.access);
  const problem = $derived(form && 'problem' in form ? form.problem : null);
  let dismissed = $state<string | null>(null);
  let copied = $state(false);
  const issued = $derived(form?.issued && form.issued.id !== dismissed ? form.issued : null);
  const invitation = $derived(form?.invitation && form.invitation.code !== dismissed ? form.invitation : null);

  const label = (principal: CardTypes.PrincipalSummary) => principal.name ?? principal.email ?? principal.principal_id;
  const nameOf = (principalId: string) => {
    const found = access?.principals.find((principal) => principal.principal_id === principalId);
    return found ? label(found) : principalId;
  };
  const accounts = $derived(access?.principals.filter((principal) => principal.kind === (tab === 'services' ? 'service' : 'user')) ?? []);
  const selected = $derived(access?.selected ?? null);
  const directCount = (principalId: string) =>
    access?.bindings.filter((binding) => 'principal' in binding.subject && binding.subject.principal.id === principalId).length ?? 0;
  const groupBindings = (groupId: string) => access?.bindings.filter((binding) => 'group' in binding.subject && binding.subject.group.id === groupId) ?? [];
  const boundCount = (role: string) => access?.bindings.filter((binding) => binding.role === role).length ?? 0;
  const family = (restriction: CardTypes.WorkloadRestriction | null | undefined) =>
    restriction?.mode === 'family' ? `${restriction.kind}/${restriction.name}` : null;
  const granted = (permissions: readonly CardTypes.Permission[]) => permissions.map(operationName).join(', ');
  const when = (value: string | null | undefined) => (value ? new Date(value).toISOString().slice(0, 16).replace('T', ' ') : '—');
  const keyState = (key: { revoked_at: string | null; expires_at: string | null }) =>
    key.revoked_at ? '⊘ revoked' : key.expires_at && Date.parse(key.expires_at) <= Date.now() ? '– expired' : '✓ active';
  const href = (target: string, account?: string) => `?tab=${target}${account ? `&account=${encodeURIComponent(account)}` : ''}`;
  /** A form action that keeps the section and selected account when the form posts without JavaScript. */
  const action = (name: string) => {
    const kept = [...page.url.searchParams].filter(([key]) => !key.startsWith('/'));
    return `?/${name}${kept.length ? `&${new URLSearchParams(kept)}` : ''}`;
  };
  async function copy(secret: string) {
    await navigator.clipboard.writeText(secret);
    copied = true;
  }
</script>

{#snippet operationChecks(held: readonly string[])}
  <div class="operations">
    {#each Object.entries(OPERATIONS) as [resource, actions] (resource)}
      <fieldset>
        <legend>{resource}</legend>
        {#each actions as action (action)}
          <label class="check"><input type="checkbox" name="operation" value={`${resource}:${action}`} checked={held.includes(`${resource}:${action}`)} /> {action}</label>
        {/each}
      </fieldset>
    {/each}
  </div>
{/snippet}

{#snippet bindingRows(rows: readonly { id: string; role: string; scope: CardTypes.PermissionScope; permissions: readonly CardTypes.Permission[]; source: CardTypes.GrantSource | CardTypes.RoleSource }[], owner: string)}
  {#each rows as row (row.id)}
    {@const source = typeof row.source === 'string' ? { kind: row.source === 'idp' ? 'provider' : 'direct' } : row.source}
    <tr>
      <td>{row.role}</td>
      <td>{scopeLabel(row.scope)}</td>
      <td>
        {#if source.kind === 'direct'}Direct
        {:else if source.kind === 'group' && 'name' in source}Group <a href={`${href('groups')}#group-${source.id}`}>{source.name}</a>
        {:else}Identity provider{/if}
      </td>
      <td><code>{granted(row.permissions)}</code></td>
      <td>
        {#if source.kind === 'direct'}
          <form method="POST" action={action('unbind')} use:enhance>
            <input type="hidden" name="bindingId" value={row.id} />
            <button class="app-control" type="submit" aria-label="Remove {row.role} at {scopeLabel(row.scope)} from {owner}">Remove</button>
          </form>
        {:else if source.kind === 'group'}
          <span class="muted">Edit in its group</span>
        {:else}
          <span class="muted">Set at provider sign-in</span>
        {/if}
      </td>
    </tr>
  {/each}
{/snippet}

<svelte:head><title>Access · {data.tenant.name} · Wyrd</title></svelte:head>
<div class="app-page">
  <div class="heading">
    <h1>Access</h1>
    <p>Who can do what in {data.tenant.name}. A role is a set of operations; a binding gives a role to an account or group at one explicit scope.</p>
  </div>

  {#if problem}
    <div class="app-problem" role="alert" data-state={problem.status === 403 ? 'unauthorized' : 'error'}>
      <strong>{problem.status === 403 ? '⊘' : '✕'} {problem.title}</strong>
      <p>{guidance(problem)} Nothing was changed.</p>
      <code>{problem.code}</code>
    </div>
  {:else if form && 'done' in form}
    <p role="status">✓ {form.done}</p>
  {/if}

  {#if data.problem?.status === 403}
    <div class="app-problem" data-state="unauthorized">
      <strong>⊘ You can't administer access for this tenant</strong>
      <p>Access administration is for tenant administrators. Ask one to grant what you need.</p>
      <code>{data.problem.code}</code>
    </div>
    <AccessRequest action="Administer access" />
  {:else if data.problem || !access}
    <div class="app-problem" data-state="error">
      <strong>✕ Access administration is unavailable</strong>
      <p>{data.problem?.remediation ?? 'Please try again later.'}</p>
      <code>{data.problem?.code}</code>
    </div>
  {:else}
    <nav class="tabs" aria-label="Access sections">
      {#each TABS as [key, name] (key)}
        <a href={href(key)} aria-current={tab === key ? 'page' : undefined}>{name}</a>
      {/each}
    </nav>

    {#if invitation}
      <div class="secret" role="status">
        <strong>✓ Invitation created. Send the code to the person now; it is not shown again.</strong>
        <code data-secret>{invitation.code}</code>
        <p>They sign up at <a href={invitation.url}>{invitation.url}</a> before {when(invitation.expiresAt)} UTC and receive exactly the access chosen here.</p>
        <div class="row">
          <button class="app-control" type="button" onclick={() => copy(invitation.code)}>{copied ? 'Copied' : 'Copy'}</button>
          <button class="app-control" type="button" onclick={() => ((dismissed = invitation.code), (copied = false))}>Dismiss</button>
        </div>
      </div>
    {/if}
    {#if issued}
      <div class="secret" role="status">
        <strong>✓ Key created. Copy it now; it is not shown again.</strong>
        <code data-secret>{issued.credential}</code>
        <div class="row">
          <button class="app-control" type="button" onclick={() => copy(issued.credential)}>{copied ? 'Copied' : 'Copy'}</button>
          <button class="app-control" type="button" onclick={() => ((dismissed = issued.id), (copied = false))}>Dismiss</button>
        </div>
      </div>
    {/if}

    <div class="grid">
      {#if (tab === 'users' || tab === 'services') && selected}
        {@const name = nameOf(selected.principal_id)}
        {@const runtime = family(selected.workload_restriction)}
        <section aria-label="Access for {name}">
          <h2>Access for {name}</h2>
          <dl>
            <dt>Account</dt><dd>{selected.kind === 'service' ? 'Service account' : 'User'} · {selected.status === 'active' ? '✓ active' : '⊘ suspended'}</dd>
            <dt>Principal</dt><dd>{selected.principal_id}</dd>
            {#if selected.kind === 'service'}
              <dt>Runtime family</dt>
              <dd data-runtime-family>{runtime ? `${runtime} · every release` : 'Not restricted to a Service'}</dd>
            {/if}
          </dl>
          {#if selected.kind === 'service'}
            <p>
              {#if runtime}This account may run only as a release of {runtime}. The application names its exact release; a new release needs no new account or key.{/if}
              The runtime family grants no operation and no access to the Service's dependencies: every grant is a binding below.
            </p>
          {/if}
          {#if selected.status === 'active'}
            <form class="stack" method="POST" action={action('suspend')} aria-label="Suspend {name}" use:enhance>
              <h3>Suspend account</h3>
              <p>
                Suspending stops this account: it can no longer sign in or use a key, what it already holds stops working within seconds, and its unfinished Verifier executions are cancelled. The account
                and its bindings are kept. Access cannot resume a suspended account.
              </p>
              <input type="hidden" name="principalId" value={selected.principal_id} />
              <input type="hidden" name="principalKind" value={selected.kind} />
              <label>Reason <input class="app-input" name="reason" required maxlength="1024" autocomplete="off" /></label>
              <p>Recorded in the audit log. Say why, without pasting a credential.</p>
              <button class="app-control" type="submit">Suspend account</button>
            </form>
          {/if}
          <h3>Bindings</h3>
          {#if selected.grants.length}
            <!-- svelte-ignore a11y_no_noninteractive_tabindex -- a scrolling table must be reachable by keyboard -->
            <div class="scroll" role="region" aria-label="Bindings of {name}" tabindex="0">
              <table>
                <thead><tr><th>Role</th><th>Scope</th><th>Source</th><th>Operations granted</th><th></th></tr></thead>
                <tbody>{@render bindingRows(selected.grants.map((grant) => ({ ...grant, id: grant.binding_id })), name)}</tbody>
              </table>
            </div>
            <p>Removing a binding removes only that binding. Access another binding still grants, directly or through a group, stays in effect.</p>
          {:else}
            <div class="app-problem" data-state="absent"><strong>– No access</strong><p>This account can sign in but holds no binding.</p></div>
          {/if}
          <h3>Combined access</h3>
          <ul class="combined" data-combined>
            {#each selected.permissions as permission (operationName(permission) + JSON.stringify(permission.scope))}
              <li><code>{operationName(permission)}</code> at {scopeLabel(permission.scope)}</li>
            {:else}
              <li>None</li>
            {/each}
          </ul>
          <form class="stack" method="POST" action={action('grant')} aria-label="Grant access to {name}" use:enhance>
            <h3>Grant access</h3>
            <input type="hidden" name="principalId" value={selected.principal_id} />
            <input type="hidden" name="principalKind" value={selected.kind} />
            <GrantFields roles={access.roles} />
            <button class="app-control primary" type="submit">Save binding</button>
          </form>
          {#if access.keys}
            <h3>API keys</h3>
            <!-- svelte-ignore a11y_no_noninteractive_tabindex -- a scrolling table must be reachable by keyboard -->
            <div class="scroll" role="region" aria-label="Keys of {name}" tabindex="0">
              <table>
                <thead><tr><th>Prefix</th><th>Created</th><th>Expires</th><th>Last used</th><th>State</th><th></th></tr></thead>
                <tbody>
                  {#each access.keys.credentials as key (key.id)}
                    <tr>
                      <td><code>{key.prefix}</code></td>
                      <td>{when(key.created_at)}</td>
                      <td>{when(key.expires_at)}</td>
                      <td>{when(key.last_used_at)}</td>
                      <td>{keyState(key)}</td>
                      <td>
                        {#if !key.revoked_at}
                          <form method="POST" action={action('revokeKey')} use:enhance>
                            <input type="hidden" name="principalId" value={selected.principal_id} />
                            <input type="hidden" name="credentialId" value={key.id} />
                            <button class="app-control" type="submit" aria-label="Revoke key {key.prefix}">Revoke</button>
                          </form>
                        {/if}
                      </td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            </div>
            <form method="POST" action={action('issueKey')} use:enhance>
              <input type="hidden" name="principalId" value={selected.principal_id} />
              <p>To rotate, issue a new key, deploy it, then revoke the old one. Keys carry the account's bindings; they add none.</p>
              <button class="app-control" type="submit">Issue key</button>
            </form>
          {:else if selected.kind === 'service'}
            <h3>API keys</h3>
            <p>This account is suspended. Its keys are refused, and none can be issued.</p>
          {/if}
        </section>
      {/if}

      {#if tab === 'users' || tab === 'services'}
        <section aria-label={tab === 'users' ? 'Users' : 'Service accounts'}>
          <h2>{tab === 'users' ? 'Users' : 'Service accounts'}</h2>
          {#if accounts.length}
            <!-- svelte-ignore a11y_no_noninteractive_tabindex -- a scrolling table must be reachable by keyboard -->
            <div class="scroll" role="region" aria-label="{tab === 'users' ? 'User' : 'Service account'} list" tabindex="0">
              <table>
                <thead><tr><th>Name</th>{#if tab === 'services'}<th>Runtime family</th>{/if}<th>Status</th><th>Direct bindings</th><th></th></tr></thead>
                <tbody>
                  {#each accounts as account (account.principal_id)}
                    <tr>
                      <td>{label(account)}</td>
                      {#if tab === 'services'}<td>{family(account.workload_restriction) ?? '—'}</td>{/if}
                      <td>{account.status === 'active' ? '✓ active' : '⊘ suspended'}</td>
                      <td>{directCount(account.principal_id)}</td>
                      <td><a href={href(tab, account.principal_id)} aria-label="Access for {label(account)}">Access</a></td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            </div>
          {:else}
            <div class="app-problem" data-state="absent"><strong>– No {tab === 'users' ? 'Users' : 'Service accounts'} yet</strong></div>
          {/if}
          {#if access.next}<div class="row"><a href={`${href(tab)}&after=${encodeURIComponent(access.next)}`}>More accounts</a></div>{/if}
        </section>
      {/if}

      {#if tab === 'users'}
        <section>
          <h2>Invite a user</h2>
          <form class="stack" method="POST" action={action('invite')} aria-label="Invite a user" use:enhance>
            <p>The person sets their own username and password with a one-time code. Their account is created holding exactly this binding; add more from their Access page afterwards.</p>
            <GrantFields roles={access.roles} />
            <button class="app-control primary" type="submit">Create invitation</button>
          </form>
        </section>
      {:else if tab === 'services'}
        <section>
          <h2>Create a Service account</h2>
          <form class="stack" method="POST" action={action('createService')} aria-label="Create a Service account" use:enhance>
            <label>Name <input class="app-input mono" name="name" required maxlength="128" autocomplete="off" /></label>
            <label>Description <input class="app-input" name="description" maxlength="256" autocomplete="off" /></label>
            <label>Runtime family <input class="app-input mono" name="family" autocomplete="off" placeholder="Service Card name, e.g. checkout" /></label>
            <p>Optional. Name the Service Card family this account runs as, not a release version or UID: the same account and key serve every release. Leave empty for automation that runs as no Service.</p>
            <p>The account starts with no access. Grant its operations from its Access page after creating it.</p>
            <button class="app-control primary" type="submit">Create Service account</button>
          </form>
        </section>
      {:else if tab === 'roles'}
        <section aria-label="Roles">
          <h2>Roles</h2>
          <p>Built-in roles are read-only. A role's operations take effect only at the scope of each binding.</p>
          {#each access.roles as role (role.name)}
            {@const bound = boundCount(role.name)}
            {@const held = role.operations.map(operationName)}
            <div class="item" data-role={role.name}>
              <h3>{role.name} <span class="tag">{role.builtin ? 'built-in · read-only' : 'custom'}</span></h3>
              <p><code>{held.join(', ')}</code></p>
              <p>{bound} binding{bound === 1 ? '' : 's'} use this role.</p>
              {#if !role.builtin}
                <details>
                  <summary>Edit {role.name}</summary>
                  <form class="stack" method="POST" action={action('putRole')} aria-label="Edit role {role.name}" use:enhance>
                    <input type="hidden" name="role" value={role.name} />
                    {#if bound}<p>Saving changes the access of all {bound} binding{bound === 1 ? '' : 's'} that use this role.</p>{/if}
                    {@render operationChecks(held)}
                    <button class="app-control primary" type="submit">Save {role.name}</button>
                  </form>
                </details>
                <form method="POST" action={action('deleteRole')} use:enhance>
                  <input type="hidden" name="role" value={role.name} />
                  <button class="app-control" type="submit" disabled={bound > 0} title={bound > 0 ? 'Remove its bindings first' : undefined}>Delete {role.name}</button>
                </form>
              {/if}
            </div>
          {/each}
        </section>
        <section>
          <h2>Create a custom role</h2>
          <form class="stack wide" method="POST" action={action('putRole')} aria-label="Create a custom role" use:enhance>
            <label>Role name <input class="app-input mono" name="role" required maxlength="64" pattern="[a-z0-9_]+" title="Lowercase letters, digits and underscores" autocomplete="off" /></label>
            <p>Lowercase letters, digits and underscores. A role needs at least one operation.</p>
            {@render operationChecks([])}
            <button class="app-control primary" type="submit">Create role</button>
          </form>
        </section>
      {:else if tab === 'groups'}
        <section aria-label="Groups">
          <h2>Groups</h2>
          <p>Optional. A group is a local list of accounts that share bindings. Changing its members changes their inherited access; a group owns nothing and cannot contain another group.</p>
          {#each access.groups as group (group.id)}
            {@const rows = groupBindings(group.id)}
            <div class="item" id="group-{group.id}" data-group={group.name}>
              <h3>{group.name}</h3>
              <ul class="members">
                {#each group.members as member (member.principal_id)}
                  <li>
                    {nameOf(member.principal_id)} <span class="tag">{member.kind}</span>
                    <form method="POST" action={action('removeMember')} use:enhance>
                      <input type="hidden" name="groupId" value={group.id} />
                      <input type="hidden" name="principalId" value={member.principal_id} />
                      <button class="app-control" type="submit" aria-label="Remove {nameOf(member.principal_id)} from {group.name}">Remove</button>
                    </form>
                  </li>
                {:else}
                  <li class="muted">No members</li>
                {/each}
              </ul>
              <form class="row" method="POST" action={action('addMember')} aria-label="Add a member to {group.name}" use:enhance>
                <input type="hidden" name="groupId" value={group.id} />
                <label>Account
                  <select class="app-input" name="principalId" required>
                    <option value="" disabled selected>Choose an account</option>
                    {#each access.principals as principal (principal.principal_id)}<option value={principal.principal_id}>{label(principal)} ({principal.kind})</option>{/each}
                  </select>
                </label>
                <button class="app-control" type="submit">Add member</button>
              </form>
              {#if rows.length}
                <!-- svelte-ignore a11y_no_noninteractive_tabindex -- a scrolling table must be reachable by keyboard -->
                <div class="scroll" role="region" aria-label="Bindings of {group.name}" tabindex="0">
                  <table>
                    <thead><tr><th>Role</th><th>Scope</th><th>Source</th><th>Operations granted</th><th></th></tr></thead>
                    <tbody>{@render bindingRows(rows, group.name)}</tbody>
                  </table>
                </div>
              {/if}
              <details>
                <summary>Grant access to {group.name}</summary>
                <form class="stack" method="POST" action={action('grant')} aria-label="Grant access to {group.name}" use:enhance>
                  <input type="hidden" name="groupId" value={group.id} />
                  <GrantFields roles={access.roles} />
                  <button class="app-control primary" type="submit">Save binding</button>
                </form>
              </details>
              <form method="POST" action={action('deleteGroup')} use:enhance>
                <input type="hidden" name="groupId" value={group.id} />
                <button class="app-control" type="submit">Delete {group.name}</button>
              </form>
            </div>
          {:else}
            <div class="app-problem" data-state="absent"><strong>– No groups</strong><p>Groups are optional; direct bindings work without them.</p></div>
          {/each}
        </section>
        <section>
          <h2>Create a group</h2>
          <form class="stack" method="POST" action={action('createGroup')} aria-label="Create a group" use:enhance>
            <label>Group name <input class="app-input" name="name" required maxlength="128" autocomplete="off" /></label>
            <button class="app-control primary" type="submit">Create group</button>
          </form>
        </section>
      {/if}
    </div>
  {/if}
</div>

<style>
  .heading {
    margin-bottom: 16px;
  }
  h1 {
    font: var(--weight-strong) 24px var(--font-sans);
  }
  h2 {
    margin: 0 0 12px;
    font: var(--weight-strong) 13px var(--font-sans);
  }
  h3 {
    margin: 16px 0 8px;
    font: var(--weight-strong) 12px var(--font-sans);
  }
  .tabs {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin: 16px 0;
    border-bottom: 1px solid var(--border);
  }
  .tabs a {
    padding: 8px 12px;
    color: var(--muted);
    font: 13px var(--font-sans);
    text-decoration: none;
  }
  .tabs a[aria-current='page'] {
    box-shadow: inset 0 -2px 0 var(--declare);
    color: var(--text);
  }
  .grid {
    display: grid;
    gap: 20px;
    max-width: 1080px;
  }
  section {
    min-width: 0;
    padding: 16px;
    border: 1px solid var(--border);
    border-radius: var(--r);
    background: var(--surface);
  }
  .item {
    display: grid;
    gap: 8px;
    padding: 12px 0;
    border-top: 1px solid var(--border-soft);
  }
  .item h3 {
    margin: 0;
  }
  .scroll {
    overflow-x: auto;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font: 12px var(--font-sans);
  }
  th,
  td {
    padding: 6px 12px 6px 0;
    border-bottom: 1px solid var(--border-soft);
    text-align: left;
    vertical-align: top;
  }
  th {
    color: var(--muted);
    font: 700 11px var(--font-mono);
    white-space: nowrap;
  }
  dl {
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    gap: 6px 16px;
    margin: 0 0 8px;
    font: 12px var(--font-mono);
  }
  dt,
  .muted {
    color: var(--muted);
  }
  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }
  code {
    font: 12px var(--font-mono);
    overflow-wrap: anywhere;
  }
  p,
  li {
    margin: 0;
    font: 12px/18px var(--font-sans);
    color: var(--muted);
  }
  a {
    color: var(--declare);
  }
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .members li {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 2px 0;
    color: var(--text);
  }
  .tag {
    padding: 1px 6px;
    border: 1px solid var(--border);
    border-radius: var(--r);
    color: var(--muted);
    font: 400 10px var(--font-mono);
  }
  .stack {
    display: grid;
    gap: 8px;
    max-width: 480px;
  }
  .stack.wide,
  details .stack {
    max-width: none;
  }
  .stack h3 {
    margin-bottom: 0;
  }
  label,
  legend,
  summary {
    font: 700 11px var(--font-mono);
  }
  .stack > label,
  .row label {
    display: grid;
    gap: 4px;
  }
  summary {
    cursor: pointer;
  }
  .operations {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: 8px;
  }
  fieldset {
    display: grid;
    gap: 4px;
    margin: 0;
    padding: 8px 10px;
    border: 1px solid var(--border-soft);
    border-radius: var(--r);
  }
  .check {
    display: flex;
    align-items: center;
    gap: 6px;
    font-weight: 400;
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: end;
    gap: 8px;
    font: 12px var(--font-sans);
  }
  .secret {
    display: grid;
    gap: 8px;
    max-width: 1080px;
    margin-bottom: 16px;
    padding: 12px;
    border: 1px solid var(--declare);
    border-radius: var(--r);
    background: var(--surface);
    font: 12px var(--font-sans);
  }
  [role='status'] {
    color: var(--text);
  }
</style>
