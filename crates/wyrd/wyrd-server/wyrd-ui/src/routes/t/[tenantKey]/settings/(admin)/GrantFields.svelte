<!--
  The role and explicit scope of one grant, shared by the invitation and grant
  forms. No scope is preselected: the administrator chooses one, and the server
  decides which of the role's operations apply there.
-->
<script lang="ts">
  import type { CardTypes } from 'wyrd-ai';
  import { CARD_KINDS, SCOPE_TYPES, operationName } from '$lib/access';
  let { roles }: { roles: readonly CardTypes.RoleView[] } = $props();
  const uid = $props.id();
  let role = $state('');
  let type = $state('');
  const chosen = $derived(roles.find((candidate) => candidate.name === role));
</script>

<label for="{uid}-role">Role</label>
  <select id="{uid}-role" class="app-input" name="role" required bind:value={role}>
    <option value="" disabled>Choose a role</option>
    {#each roles as candidate (candidate.name)}<option value={candidate.name}>{candidate.name}</option>{/each}
  </select>
<label for="{uid}-scope">Scope</label>
  <select id="{uid}-scope" class="app-input" name="scopeType" required bind:value={type}>
    <option value="" disabled>Choose a scope</option>
    {#each SCOPE_TYPES as [value, label] (value)}<option {value}>{label}</option>{/each}
  </select>
{#if type === 'card_family' || type === 'card_version'}
  <label for="{uid}-kind">Card kind</label>
    <select id="{uid}-kind" class="app-input" name="cardKind" required>
      {#each CARD_KINDS as kind (kind)}<option value={kind}>{kind}</option>{/each}
    </select>
  <label for="{uid}-cardName">Card name</label><input id="{uid}-cardName" class="app-input mono" name="cardName" required autocomplete="off" />
  {#if type === 'card_version'}
    <label for="{uid}-cardUid">Card UID</label><input id="{uid}-cardUid" class="app-input mono" name="cardUid" required autocomplete="off" />
    <p>Only this exact version. A later version of the Card is not covered.</p>
  {:else}
    <p>Covers every version of this Card, including versions registered later.</p>
  {/if}
{:else if type === 'bifrost_schema' || type === 'bifrost_table'}
  <label for="{uid}-catalog">Catalog</label><input id="{uid}-catalog" class="app-input mono" name="catalog" required value="bifrost" autocomplete="off" />
  <label for="{uid}-schema">Schema</label><input id="{uid}-schema" class="app-input mono" name="schema" required autocomplete="off" />
  {#if type === 'bifrost_table'}<label for="{uid}-tableUid">Table UID</label><input id="{uid}-tableUid" class="app-input mono" name="tableUid" required autocomplete="off" />{/if}
{:else if type === 'gateway_deployment' || type === 'gateway_model'}
  <label for="{uid}-deployment">Deployment</label><input id="{uid}-deployment" class="app-input mono" name="deployment" required autocomplete="off" />
  {#if type === 'gateway_model'}<label for="{uid}-model">Model</label><input id="{uid}-model" class="app-input mono" name="model" required autocomplete="off" />{/if}
{:else if type === 'github_connection'}
  <label for="{uid}-connection">Connection ID</label><input id="{uid}-connection" class="app-input mono" name="connection" required autocomplete="off" />
{:else if type === 'tool'}
  <label for="{uid}-tool">Tool</label><input id="{uid}-tool" class="app-input mono" name="tool" required autocomplete="off" />
{:else if type === 'all'}
  <p>Every resource in this tenant, now and later.</p>
{/if}
{#if chosen}
  <p data-role-operations>
    {chosen.name} holds <code>{chosen.operations.map(operationName).join(', ')}</code>. Only the operations that apply at the chosen scope are granted; the saved binding lists them.
  </p>
{/if}

<style>
  label {
    font: 700 11px var(--font-mono);
  }
  p {
    margin: 0;
    font: 12px/18px var(--font-sans);
    color: var(--muted);
  }
  code {
    font: 12px var(--font-mono);
    overflow-wrap: anywhere;
  }
</style>
