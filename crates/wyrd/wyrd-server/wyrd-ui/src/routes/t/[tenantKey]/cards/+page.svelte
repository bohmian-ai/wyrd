<!--
  The registry inventory. Filters and pages live in the URL, so Back from a
  Card returns to the same rows; opening a row navigates to its exact version.
-->
<script lang="ts">
  import { goto } from '$app/navigation';
  import { CardInventory, EmptyState, type ActionIntent, type ActionState, type CardRef } from 'wyrd-ai-ui';
  import { validateIntent, type CardInventoryInput } from 'wyrd-ai-ui/loaders';
  import { writeDetail, writeInventory } from '$lib/cards-host';
  import type { PageProps } from './$types';

  let { data }: PageProps = $props();
  const base = $derived(`/t/${encodeURIComponent(data.tenant.key)}`);
  let actions = $state<Record<string, ActionState>>({});

  async function onaction(intent: ActionIntent) {
    const valid = validateIntent('CardInventory', intent);
    if ('code' in valid) return void (actions[intent.name] = { state: 'error', problem: valid });
    const props = 'code' in data.inventory ? undefined : data.inventory;
    actions[valid.name] = { state: 'pending' };
    if (valid.name === 'card.open') {
      await goto(`${base}/cards/detail${writeDetail((valid.input as { card_ref: CardRef }).card_ref)}`);
    } else if (valid.name === 'cards.more') {
      await goto(`${base}/cards${writeInventory({ ...props?.input, ...(valid.input as { pages: number }) })}`, { replaceState: true, noScroll: true, keepFocus: true });
    } else {
      await goto(`${base}/cards${writeInventory(valid.input as CardInventoryInput)}`, { keepFocus: true, noScroll: true });
    }
    delete actions[valid.name];
  }
</script>

<svelte:head><title>Cards · {data.tenant.name} · Wyrd</title></svelte:head>
<div class="app-page cards">
  <header>
    <p class="crumb"><span>{data.tenant.key}</span> / Cards</p>
    <h1>Cards</h1>
  </header>
  {#if 'code' in data.inventory}
    <EmptyState state="error" problem={data.inventory} />
  {:else}
    <CardInventory {...data.inventory} {actions} {onaction} />
  {/if}
</div>

<style>
  .cards {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .crumb {
    margin: 0 0 4px;
    color: var(--muted);
    font: 12px var(--font-mono);
  }
  h1 {
    margin: 0;
    font: var(--weight-strong) 24px var(--font-sans);
  }
</style>
