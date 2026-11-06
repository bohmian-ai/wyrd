<!--
  An exact Card's server-derived relationships: the Cards it references, then
  the Cards that reference it, each an exact version the host can open.
-->
<script lang="ts">
  import type { ActionProps, CardRef } from '../contract.js';
  import type { RelationshipListProps } from '../cards.js';
  import { fmtCardRef } from '../format.js';
  import ActionNote from './ActionNote.svelte';
  import EmptyState from './EmptyState.svelte';
  import Panel from './Panel.svelte';

  let { input, result, actions = {}, onaction }: RelationshipListProps & ActionProps = $props();

  const state = $derived(actions['card.open']);
  const disabled = $derived(!onaction || state?.state === 'pending' || state?.state === 'unavailable');
  const groups = $derived(
    result.state === 'populated'
      ? ([
          ['references', result.data.outbound],
          ['referenced by', result.data.inbound]
        ] as const)
      : []
  );
  const open = (ref: CardRef) => onaction?.({ name: 'card.open', input: { card_ref: ref } });
</script>

<Panel title="Relationships" note="server-derived">
  {#if result.state === 'populated'}
    <table>
      <tbody>
        {#each groups as [edge, refs] (edge)}
          {#each refs as relationship (fmtCardRef(relationship.ref))}
            <tr>
              <td class="muted">{relationship.alias ?? edge}</td>
              <td>{relationship.ref.kind}</td>
              <td><button type="button" class="open" {disabled} onclick={() => open(relationship.ref)}>{fmtCardRef(relationship.ref)}</button></td>
            </tr>
          {/each}
        {/each}
      </tbody>
    </table>
    <ActionNote {state} />
  {:else if result.state === 'empty'}
    <EmptyState state="empty" title={result.reason} />
  {:else if result.state === 'loading'}
    <EmptyState state="loading" title="Loading relationships of {input.cardRef.name}" />
  {:else}
    <EmptyState state={result.state} problem={result.problem} />
  {/if}
</Panel>
