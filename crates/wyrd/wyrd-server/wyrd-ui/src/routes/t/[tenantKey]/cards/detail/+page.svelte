<!--
  One exact Card version: header, then the selected tab. Every Card has
  Overview (facts and relationships), Files and YAML; a Prompt adds its
  authored definition, variables, using Agents and version comparison. The
  URL holds the exact reference, tab and comparison, so every navigation
  keeps the selected version until the reader picks another.
-->
<script lang="ts">
  import { goto } from '$app/navigation';
  import { navigating, page } from '$app/state';
  import {
    ArtifactTable,
    CardFacts,
    CardHeader,
    EmptyState,
    PromptMessages,
    PromptVariables,
    RelationshipList,
    UsedBy,
    VersionDiff,
    type ActionIntent,
    type ActionState,
    type CardRef
  } from 'wyrd-ai-ui';
  import { validateIntent, type ComponentName } from 'wyrd-ai-ui/loaders';
  import { writeDetail } from '$lib/cards-host';
  import type { PageProps } from './$types';

  let { data }: PageProps = $props();
  const base = $derived(`/t/${encodeURIComponent(data.tenant.key)}`);
  let actions = $state<Record<string, ActionState>>({});
  /** A Prompt has no telemetry of its own, so its header offers no Observe. */
  const headerActions = $derived(
    data.cardRef?.kind === 'Prompt'
      ? { ...actions, 'card.open_in_observe': { state: 'unavailable', reason: 'A Prompt records no telemetry; open an Agent that uses it.' } as const }
      : actions
  );

  /** Navigating to another version clears this version's content until the new one loads. */
  const switching = $derived.by(() => {
    const to = navigating.to?.url;
    if (!to || to.pathname !== page.url.pathname || !data.cardRef) return false;
    return ['kind', 'name', 'version'].some((key) => to.searchParams.get(key) !== (data.cardRef as Record<string, unknown>)[key]);
  });

  async function navigate(name: string, href: string) {
    actions[name] = { state: 'pending' };
    await goto(href, { keepFocus: true, noScroll: name !== 'card.open' });
    delete actions[name];
  }

  function onaction(component: ComponentName) {
    return (intent: ActionIntent) => {
      const valid = validateIntent(component, intent);
      if ('code' in valid) return void (actions[intent.name] = { state: 'error', problem: valid });
      if (!data.cardRef) return;
      const input = valid.input as { card_ref?: CardRef; tab?: string; other_version?: string };
      const detail = `${base}/cards/detail`;
      switch (valid.name) {
        case 'card.version':
          return void navigate(valid.name, `${detail}${writeDetail({ ...input.card_ref!, uid: undefined }, { tab: data.tab })}`);
        case 'card.tab':
          return void navigate(valid.name, `${detail}${writeDetail(data.cardRef, { tab: input.tab, compare: data.compare })}`);
        case 'card.open':
          return void navigate(valid.name, `${detail}${writeDetail(input.card_ref!)}`);
        case 'prompt.compare':
          return void navigate(valid.name, `${detail}${writeDetail(data.cardRef, { tab: 'versions', compare: input.other_version })}`);
        case 'card.open_in_observe': {
          const uid = data.header?.result.state === 'populated' ? data.header.result.data.card.metadata.uid : undefined;
          if (data.cardRef.kind === 'Prompt' || !uid) return;
          return void navigate(valid.name, `${base}/observe?${new URLSearchParams({ where: JSON.stringify([{ column: 'workload_card_uid', value: uid }]) })}`);
        }
      }
    };
  }
</script>

<svelte:head><title>{data.cardRef ? `${data.cardRef.name}@${data.cardRef.version}` : 'Card'} · Cards · Wyrd</title></svelte:head>
<div class="app-page detail">
  <p class="crumb"><a href="{base}/cards">Cards</a>{#if data.cardRef} / {data.cardRef.kind} / {data.cardRef.name}{/if}</p>
  {#if data.problem}
    <EmptyState state="error" problem={data.problem} />
  {:else if switching}
    <EmptyState state="loading" title="Loading {navigating.to?.url.searchParams.get('name')}@{navigating.to?.url.searchParams.get('version')}" />
  {:else if data.header}
    <CardHeader {...data.header} actions={headerActions} onaction={onaction('CardHeader')} />
    {#if data.files}
      <ArtifactTable {...data.files} />
    {:else if data.yaml}
      <CardFacts {...data.yaml} />
    {:else if data.diff}
      {#if 'code' in data.diff}<EmptyState state="error" problem={data.diff} />{:else}<VersionDiff {...data.diff} {actions} onaction={onaction('VersionDiff')} />{/if}
    {:else if data.facts && data.relationships}
      <div class="columns">
        <div class="main">
          {#if data.prompt}
            <PromptMessages {...data.prompt.messages} />
            <PromptVariables {...data.prompt.variables} />
            <UsedBy {...data.prompt.usedBy} {actions} onaction={onaction('UsedBy')} />
            <VersionDiff {...data.prompt.diff} {actions} onaction={onaction('VersionDiff')} />
          {:else}
            <CardFacts {...data.facts} />
          {/if}
        </div>
        <aside>
          <RelationshipList {...data.relationships} {actions} onaction={onaction('RelationshipList')} />
          {#if data.prompt}<CardFacts {...data.facts} />{/if}
        </aside>
      </div>
    {/if}
  {/if}
</div>

<style>
  .detail {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .crumb {
    margin: 0;
    color: var(--muted);
    font: 12px var(--font-mono);
  }
  .crumb a {
    color: var(--declare);
  }
  .columns {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(260px, 340px);
    gap: 16px;
    align-items: start;
  }
  @media (max-width: 900px) {
    .columns {
      grid-template-columns: minmax(0, 1fr);
    }
  }
  .main,
  aside {
    display: flex;
    flex-direction: column;
    gap: 16px;
    min-width: 0;
  }
</style>
