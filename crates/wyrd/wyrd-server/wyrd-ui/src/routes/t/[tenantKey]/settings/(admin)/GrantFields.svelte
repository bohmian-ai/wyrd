<!--
  The role and explicit scope of one grant, shared by the invitation and grant
  forms. Roles show as a choice list with each Role's server description, or
  as a select where space is short. The scope starts at the whole workspace;
  any other scope asks for its references, and the server decides which of the
  role's operations apply there.
-->
<script lang="ts">
  import { untrack } from 'svelte';
  import type { CardTypes } from 'wyrd-ai';
  import { CARD_KINDS, SCOPE_TYPES, operationName } from '$lib/access';
  interface Props {
    roles: readonly CardTypes.RoleView[];
    /** `choices` lists each Role with its description; `select` is one compact control. */
    pick?: 'choices' | 'select';
  }
  let { roles, pick = 'select' }: Props = $props();
  const uid = $props.id();
  // The first choice only: once the administrator picks, their pick stands.
  let role = $state(untrack(() => (pick === 'choices' && roles.some((candidate) => candidate.name === 'viewer') ? 'viewer' : '')));
  let type = $state('all');
  const chosen = $derived(roles.find((candidate) => candidate.name === role));
  const SCOPES = SCOPE_TYPES.map(([value, label]) => [value, value === 'all' ? 'The whole workspace' : label] as const);
</script>

{#if pick === 'choices'}
  <fieldset class="field">
    <legend>Role</legend>
    {#each roles as candidate (candidate.name)}
      <label class="check choice" class:on={role === candidate.name}>
        <input type="radio" name="role" value={candidate.name} required bind:group={role} />
        <span><b>{candidate.name}</b>{#if candidate.description}<br /><span class="muted">{candidate.description}</span>{/if}</span>
      </label>
    {/each}
  </fieldset>
{:else}
  <div class="field">
    <label for="{uid}-role">Role</label>
    <select id="{uid}-role" class="app-input" name="role" required bind:value={role}>
      <option value="" disabled>Choose a role</option>
      {#each roles as candidate (candidate.name)}<option value={candidate.name}>{candidate.name}</option>{/each}
    </select>
    {#if chosen}<span class="hint" data-role-operations>{chosen.description ?? chosen.operations.map(operationName).join(', ')}</span>{/if}
  </div>
{/if}
<div class="field">
  <label for="{uid}-scope">Applies to</label>
  <select id="{uid}-scope" class="app-input" name="scopeType" required bind:value={type}>
    {#each SCOPES as [value, label] (value)}<option {value}>{label}</option>{/each}
  </select>
  {#if type === 'all'}<span class="hint">Every resource in this workspace, now and later.</span>{/if}
</div>
{#if type === 'card_family' || type === 'card_version'}
  <div class="field">
    <label for="{uid}-kind">Card kind</label>
    <select id="{uid}-kind" class="app-input" name="cardKind" required>
      {#each CARD_KINDS as kind (kind)}<option value={kind}>{kind}</option>{/each}
    </select>
  </div>
  <div class="field"><label for="{uid}-cardName">Card name</label><input id="{uid}-cardName" class="app-input mono" name="cardName" required autocomplete="off" /></div>
  {#if type === 'card_version'}
    <div class="field">
      <label for="{uid}-cardUid">Card UID</label><input id="{uid}-cardUid" class="app-input mono" name="cardUid" required autocomplete="off" />
      <span class="hint">Only this exact version. A later version of the Card is not covered.</span>
    </div>
  {:else}
    <span class="hint">Covers every version of this Card, including versions registered later.</span>
  {/if}
{:else if type === 'bifrost_schema' || type === 'bifrost_table'}
  <div class="field"><label for="{uid}-catalog">Catalog</label><input id="{uid}-catalog" class="app-input mono" name="catalog" required value="bifrost" autocomplete="off" /></div>
  <div class="field"><label for="{uid}-schema">Schema</label><input id="{uid}-schema" class="app-input mono" name="schema" required autocomplete="off" /></div>
  {#if type === 'bifrost_table'}<div class="field"><label for="{uid}-tableUid">Table UID</label><input id="{uid}-tableUid" class="app-input mono" name="tableUid" required autocomplete="off" /></div>{/if}
{:else if type === 'gateway_deployment' || type === 'gateway_model'}
  <div class="field"><label for="{uid}-deployment">Deployment</label><input id="{uid}-deployment" class="app-input mono" name="deployment" required autocomplete="off" /></div>
  {#if type === 'gateway_model'}<div class="field"><label for="{uid}-model">Model</label><input id="{uid}-model" class="app-input mono" name="model" required autocomplete="off" /></div>{/if}
{:else if type === 'github_connection'}
  <div class="field"><label for="{uid}-connection">Connection ID</label><input id="{uid}-connection" class="app-input mono" name="connection" required autocomplete="off" /></div>
{:else if type === 'tool'}
  <div class="field"><label for="{uid}-tool">Tool</label><input id="{uid}-tool" class="app-input mono" name="tool" required autocomplete="off" /></div>
{/if}

<style>
  fieldset {
    margin: 0;
    padding: 0;
    border: 0;
  }
  legend {
    padding: 0 0 6px;
    font-size: 13px;
    font-weight: var(--weight-strong);
  }
  .choice {
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: var(--r);
  }
  .choice.on {
    border-color: var(--declare);
  }
  /* The browser's own radio margin, which the reset removes, aligns it with the role name. */
  .choice input {
    margin: 3px 3px 0 5px;
  }
</style>
