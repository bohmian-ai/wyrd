<!--
  One exact Card version: breadcrumb, header, then the selected tab. Every Card has
  Overview (facts and relationships), Files and YAML; an Agent shows each
  exact Prompt it references; a Prompt adds its
  authored definition, variables, using Agents and one line naming what
  changed from the version before, with the comparison on its own tab; a
  Verifier's overview is its declared graph and where that version runs. The
  URL holds the exact reference, tab and comparison, so every navigation
  keeps the selected version until the reader picks another.
-->
<script lang="ts">
  import { goto } from '$app/navigation';
  import { navigating, page } from '$app/state';
  import {
    ActionButton,
    ArtifactTable,
    CardFacts,
    CardHeader,
    EmptyState,
    PromptMessages,
    PromptVariables,
    RelationshipList,
    RunsOn,
    UsedBy,
    VerifierDefinition,
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
  /** A Prompt records no telemetry; its primary action opens the first Agent that uses it. */
  const agent = $derived(data.prompt?.usedBy.result.state === 'populated' ? data.prompt.usedBy.result.data[0]?.ref : undefined);

  /** What this Prompt version changed from the one before it; the comparison itself is the Versions tab. */
  const changed = $derived.by(() => {
    const diff = data.prompt?.diff.result;
    return diff?.state === 'populated' && diff.data.other ? { other: diff.data.other, fields: diff.data.changes.map((change) => change.field) } : undefined;
  });

  /** The newest listed version of a Verifier, when the reader is viewing an older one. */
  const newer = $derived.by(() => {
    const versions = data.verifier && data.header?.result.state === 'populated' ? data.header.result.data.versions : undefined;
    const newest = versions?.state === 'populated' ? versions.data.items[0]?.version : undefined;
    return newest !== undefined && newest !== data.cardRef?.version ? newest : undefined;
  });

  /** The exact Verifier version shown, by the Card UID its executions record. */
  const verifierUid = $derived(data.verifier && data.header?.result.state === 'populated' ? data.header.result.data.card.metadata.uid : undefined);

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
<header class="bar">
  <a href="{base}/cards">Cards</a>
  {#if data.cardRef}<span>/</span><span>{data.cardRef.kind}</span><span>/</span><span class="here">{data.cardRef.name}</span>{/if}
</header>
{#if data.problem}
  <div class="content"><EmptyState state="error" problem={data.problem} /></div>
{:else if switching}
  <div class="content">
    <EmptyState state="loading" title="Loading {navigating.to?.url.searchParams.get('name')}@{navigating.to?.url.searchParams.get('version')}" />
  </div>
{:else if data.header}
  <section class="hero">
    <CardHeader {...data.header} {actions} onaction={onaction('CardHeader')} primary={agent ? openAgent : verifierUid ? resultsInVerify : undefined} />
  </section>
  {#if data.files}
    <div class="content"><ArtifactTable {...data.files} /></div>
  {:else if data.yaml}
    <div class="content"><CardFacts {...data.yaml} /></div>
  {:else if data.diff}
    <div class="content">
      {#if 'code' in data.diff}<EmptyState state="error" problem={data.diff} />{:else}<VersionDiff {...data.diff} {actions} onaction={onaction('VersionDiff')} />{/if}
    </div>
  {:else if data.facts && data.relationships}
    <div class="content columns">
      <div class="main">
        {#if data.verifier}
          {#if newer}
            <p class="note" role="note">You are viewing {data.cardRef?.version}; the newest registered version is {newer}. Viewing a version changes nothing: each binding pins the exact version it runs.</p>
          {/if}
          <VerifierDefinition {...data.verifier.definition} />
        {:else if data.prompt}
          <PromptMessages {...data.prompt.messages} />
          <PromptVariables {...data.prompt.variables} />
          <UsedBy {...data.prompt.usedBy} {actions} onaction={onaction('UsedBy')} />
          {#if changed}
            <p class="note" role="note" aria-label="Changes">
              Changed from v{changed.other}: {changed.fields.join(', ')}.
              <a href="{base}/cards/detail{writeDetail(data.cardRef!, { tab: 'versions' })}">Compare versions</a>
            </p>
          {/if}
        {:else}
          {#each data.prompts ?? [] as prompt (`${prompt.input.cardRef.name}@${prompt.input.cardRef.version}`)}
            <section class="referenced" aria-label="Prompt {prompt.input.cardRef.name}@{prompt.input.cardRef.version}">
              <a href="{base}/cards/detail{writeDetail(prompt.input.cardRef)}">{prompt.input.cardRef.name}@{prompt.input.cardRef.version}</a>
              <PromptMessages {...prompt} />
            </section>
          {/each}
          <CardFacts {...data.facts} />
        {/if}
      </div>
      <aside>
        {#if data.verifier}<RunsOn {...data.verifier.runsOn} {actions} onaction={onaction('RunsOn')} />{/if}
        <RelationshipList {...data.relationships} {actions} onaction={onaction('RelationshipList')} />
        {#if data.prompt || data.verifier}<CardFacts {...data.facts} />{/if}
      </aside>
    </div>
  {/if}
{/if}

{#snippet resultsInVerify()}
  <a class="results" href="{base}/verify?{new URLSearchParams({ where: JSON.stringify([{ column: 'verifier_card_uid', value: verifierUid }]) })}">Results in Verify</a>
{/snippet}

{#snippet openAgent()}
  {#if agent}
    <ActionButton
      variant="primary"
      label="Open {agent.name}"
      state={actions['card.open']}
      onclick={() => onaction('UsedBy')({ name: 'card.open', input: { card_ref: agent } })}
    />
  {/if}
{/snippet}

<style>
  /* The primary action's look, as a link: it navigates, so it is an anchor. */
  .results {
    display: inline-flex;
    align-items: center;
    height: 32px;
    padding: 0 12px;
    border: 1px solid var(--declare);
    border-radius: var(--r);
    background: var(--declare);
    color: var(--declare-ink);
    font: 600 13px var(--font-sans);
    text-decoration: none;
    white-space: nowrap;
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 55px;
    padding: 10px 24px;
    border-bottom: 1px solid var(--border);
    background: var(--nav-surface);
    color: var(--muted);
    font: 12px var(--font-mono);
  }
  .bar a {
    color: inherit;
  }
  .bar a:hover {
    color: var(--declare);
  }
  .here {
    color: var(--text);
  }
  .hero {
    padding: 22px 24px 0;
    border-bottom: 1px solid var(--border);
  }
  .content {
    display: flex;
    flex-direction: column;
    gap: 20px;
    padding: 20px 24px 32px;
  }
  .columns {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 360px;
    align-items: start;
  }
  .main,
  aside {
    display: flex;
    flex-direction: column;
    gap: 20px;
    min-width: 0;
  }
  aside {
    gap: 16px;
  }
  .referenced {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .referenced a {
    color: var(--declare);
    font: 12px var(--font-mono);
  }
  .note a {
    color: var(--declare);
  }
  .note {
    margin: 0;
    border: 1px solid var(--border);
    border-left: 2px solid var(--declare);
    border-radius: var(--r);
    padding: 8px 12px;
    background: var(--surface);
    color: var(--text);
    font: 13px/1.5 var(--font-sans);
  }
  @media (max-width: 1023px) {
    .columns {
      grid-template-columns: minmax(0, 1fr);
    }
  }
  @media (max-width: 767px) {
    .bar,
    .hero,
    .content {
      padding-inline: 16px;
    }
  }
</style>
