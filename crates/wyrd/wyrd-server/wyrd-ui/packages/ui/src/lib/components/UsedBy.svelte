<!--
  The Agents whose spec references an exact Prompt, each with its execution
  route and the Services that reference it. A refused Agent read keeps its row.
-->
<script lang="ts">
  import type { ActionProps, CardRef } from '../contract.js';
  import type { UsedByProps } from '../cards.js';
  import { fmtCardRef } from '../format.js';
  import ActionNote from './ActionNote.svelte';
  import EmptyState from './EmptyState.svelte';
  import Panel from './Panel.svelte';

  let { input, result, actions = {}, onaction }: UsedByProps & ActionProps = $props();

  const state = $derived(actions['card.open']);
  const disabled = $derived(!onaction || state?.state === 'pending' || state?.state === 'unavailable');
  const open = (ref: CardRef) => onaction?.({ name: 'card.open', input: { card_ref: ref } });
</script>

<Panel title="Used by" note="server-derived">
  {#if result.state === 'populated'}
    <table>
      <thead><tr><th>Agent</th><th>Route</th><th>Service</th></tr></thead>
      <tbody>
        {#each result.data as user (fmtCardRef(user.ref))}
          <tr>
            <td><button type="button" class="open" {disabled} onclick={() => open(user.ref)}>{fmtCardRef(user.ref)}</button></td>
            {#if user.agent.state === 'populated'}
              {@const route = user.agent.data.route}
              <td>{route.kind === 'wyrd_gateway' ? `gateway ${route.deployment}` : 'native'}</td>
              <td>
                {#each user.agent.data.services as service (fmtCardRef(service))}
                  <button type="button" class="open" {disabled} onclick={() => open(service)}>{fmtCardRef(service)}</button>
                {:else}<span class="muted">—</span>{/each}
              </td>
            {:else if user.agent.state === 'denied' || user.agent.state === 'error'}
              <td colspan="2" class="muted">{user.agent.problem.title} ({user.agent.problem.code})</td>
            {/if}
          </tr>
        {/each}
      </tbody>
    </table>
    <ActionNote {state} />
  {:else if result.state === 'empty'}
    <EmptyState state="empty" title={result.reason} />
  {:else if result.state === 'loading'}
    <EmptyState state="loading" title="Loading Agents using {input.cardRef.name}" />
  {:else}
    <EmptyState state={result.state} problem={result.problem} />
  {/if}
</Panel>
