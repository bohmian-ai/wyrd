<!--
  One account's drawer, for People and Service accounts: its status and sign-in
  method or restriction, its Roles with scope and source, its keys, and
  Suspend in a Danger zone. Every value is the server's; every control posts an
  existing access action.
-->
<script lang="ts" module>
  import type { CardTypes } from 'wyrd-ai';
  /** How a person is labelled: display name, else username or account name, else email. */
  export const labelOf = (account: CardTypes.PrincipalSummary) => account.display_name ?? account.name ?? account.email ?? 'Account';
  /** Up to two initials of the label. */
  export const initialsOf = (account: CardTypes.PrincipalSummary) => {
    const label = labelOf(account).trim();
    return (/\s/.test(label) ? label.split(/\s+/).map((part) => part[0]).join('') : label).slice(0, 2).toUpperCase();
  };
  export const METHODS = { google: 'Google', github: 'GitHub', password: 'Password', sso: 'Company SSO' } as const;
  /** A Service account's runtime family, if restricted to one. */
  export const familyOf = (restriction: CardTypes.WorkloadRestriction | null | undefined) =>
    restriction?.mode === 'family' ? `${restriction.kind}/${restriction.name}` : null;
</script>

<script lang="ts">
  import { enhance } from '$app/forms';
  import type { CredentialList } from 'wyrd-ai';
  import { fmtAgo } from 'wyrd-ai-ui';
  import { scopeLabel } from '$lib/access';
  import { ICONS } from '$lib/icons';
  import Icon from '$lib/Icon.svelte';
  import Mark from '$lib/Mark.svelte';
  import Overlay from '$lib/Overlay.svelte';
  import Pill from '$lib/Pill.svelte';
  import GrantFields from './GrantFields.svelte';

  interface Props {
    /** The account's directory entry, when it is on the loaded page. */
    summary: CardTypes.PrincipalSummary | undefined;
    selected: { access: CardTypes.AccessSummary; keys: CredentialList | null };
    roles: readonly CardTypes.RoleView[];
    /** The page without this drawer. */
    close: string;
    /** A form action that keeps the page's query. */
    action: (name: string) => string;
  }
  let { summary, selected, roles, close, action }: Props = $props();
  let adding = $state(false);
  const access = $derived(selected.access);
  const name = $derived(summary ? labelOf(summary) : 'Account');
  const service = $derived(access.kind === 'service');
  const keyState = (key: { revoked_at: string | null; expires_at: string | null }) =>
    key.revoked_at ? 'revoked' : key.expires_at && Date.parse(key.expires_at) <= Date.now() ? 'expired' : 'active';
</script>

<Overlay kind="drawer" title={name} {close}>
  {#snippet heading()}
    <span class="avatar" aria-hidden="true">{summary ? initialsOf(summary) : '·'}</span>
    <div><h2>{name}</h2>{#if summary?.email}<span class="muted" style="font-size:13px">{summary.email}</span>{/if}</div>
  {/snippet}
  <dl class="dl">
    <dt>Status</dt><dd>{#if access.status === 'active'}<Pill label="Active" />{:else}<Pill tone="off" label="Suspended" />{/if}</dd>
    {#if service}
      <dt>Restriction</dt><dd data-runtime-family>{familyOf(access.workload_restriction) ? `${familyOf(access.workload_restriction)} · every release` : 'Not restricted to a Service'}</dd>
    {:else if summary?.authentication}
      <dt>Signs in with</dt><dd><span class="method"><Mark method={summary.authentication} />{METHODS[summary.authentication]}</span></dd>
    {/if}
  </dl>

  <section class="card" style="margin:0" aria-label="Roles of {name}">
    <header>
      <h2>Roles</h2>
      <div class="right"><button class="app-control" type="button" aria-expanded={adding} onclick={() => (adding = !adding)}><Icon d={ICONS.plus} size={14} />Add role</button></div>
    </header>
    {#each access.grants as grant (grant.binding_id)}
      {@const source = grant.source}
      {@const where = grant.scope === 'all' ? 'Whole workspace' : scopeLabel(grant.scope)}
      <div class="row" data-binding={grant.binding_id}>
        <div class="label">
          <b>{grant.role}</b>
          <span>{where}{#if source.kind === 'group'} · Through group {source.name}{:else if source.kind === 'provider'} · from company sign-in{/if}</span>
        </div>
        <div class="ctl">
          {#if source.kind === 'direct'}
            <form method="POST" action={action('unbind')} use:enhance>
              <input type="hidden" name="bindingId" value={grant.binding_id} />
              <button class="app-control" type="submit" aria-label="Remove {grant.role} at {where} from {name}">Remove</button>
            </form>
          {:else if source.kind === 'provider'}
            <span class="muted" style="font-size:12px">Set at company sign-in</span>
          {/if}
        </div>
      </div>
    {:else}
      <div class="row"><div class="label"><span>No roles. This account can sign in but can do nothing.</span></div></div>
    {/each}
    {#if adding}
      <form class="row grant" method="POST" action={action('grant')} aria-label="Grant access to {name}" use:enhance>
        <input type="hidden" name="principalId" value={access.principal_id} />
        <input type="hidden" name="principalKind" value={access.kind} />
        <GrantFields {roles} />
        <div><button class="app-control primary" type="submit">Save role</button></div>
      </form>
    {/if}
  </section>

  {#if selected.keys}
    <section class="card" style="margin:0" aria-label="Keys of {name}">
      <header>
        <h2>API keys</h2>
        {#if service && access.status === 'active'}
          <div class="right">
            <form method="POST" action={action('issueKey')} use:enhance>
              <input type="hidden" name="principalId" value={access.principal_id} />
              <button class="app-control" type="submit"><Icon d={ICONS.plus} size={14} />Issue key</button>
            </form>
          </div>
        {/if}
      </header>
      {#if service && access.status !== 'active'}<div class="row"><div class="label"><span>This account is suspended. Its keys are refused.</span></div></div>{/if}
      {#each selected.keys.credentials as key (key.id)}
        {@const state = keyState(key)}
        <div class="row" data-key={key.id}>
          <div class="label">
            <b class="mono">{key.prefix}…</b>
            <span>Created {fmtAgo(Date.parse(key.created_at))} · {key.last_used_at ? `used ${fmtAgo(Date.parse(key.last_used_at))}` : 'never used'}{state === 'active' ? '' : ` · ${state}`}</span>
          </div>
          <div class="ctl">
            {#if !key.revoked_at}
              <form method="POST" action={action('revokeKey')} use:enhance>
                <input type="hidden" name="principalId" value={access.principal_id} />
                <input type="hidden" name="credentialId" value={key.id} />
                <button class="app-control" type="submit" aria-label="Revoke key {key.prefix}">Revoke</button>
              </form>
            {/if}
          </div>
        </div>
      {:else}
        <div class="row"><div class="label"><span>No keys.{service ? ' To rotate, issue a new key, deploy it, then revoke the old one.' : ''}</span></div></div>
      {/each}
    </section>
  {/if}

  {#if access.status === 'active'}
    <section class="danger-zone">
      <header><h2>Danger zone</h2></header>
      <form class="row" method="POST" action={action('suspend')} aria-label="Suspend {name}" use:enhance>
        <input type="hidden" name="principalId" value={access.principal_id} />
        <input type="hidden" name="principalKind" value={access.kind} />
        <div class="label">
          <b>Suspend {name}</b>
          <span>Ends their sessions and keys now and cancels their unfinished Verifier executions. Their roles are kept; a suspended account cannot be resumed here.</span>
          <label class="field reason">Reason <input class="app-input" name="reason" required maxlength="1024" autocomplete="off" placeholder="Recorded in the audit log; never a credential" /></label>
        </div>
        <div class="ctl"><button class="app-control danger" type="submit">Suspend</button></div>
      </form>
    </section>
  {/if}
</Overlay>

<style>
  .grant {
    display: grid;
    gap: 14px;
    align-items: start;
  }
  .reason {
    margin-top: 10px;
    font-size: 13px;
  }
</style>
