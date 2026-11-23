<!--
  One pull request of a Repository: its review sets and the selected set's
  members. The URL holds the pinned set, so reload, a copied link and Back
  restore the same set; the plain link follows the latest. A member opens its
  exact execution page.
-->
<script lang="ts">
  import { goto } from '$app/navigation';
  import { EmptyState, ReviewSets, type ActionIntent, type ActionState, type CardRef } from 'wyrd-ai-ui';
  import { validateIntent } from 'wyrd-ai-ui/loaders';
  import { requestable } from '$lib/access';
  import { writeDetail } from '$lib/cards-host';
  import AccessRequest from '$lib/AccessRequest.svelte';
  import Crumbs from '$lib/Crumbs.svelte';
  import { pullHref, repositoryHref } from '$lib/repositories-host';
  import type { PageProps } from './$types';

  let { data }: PageProps = $props();
  const base = $derived(`/t/${encodeURIComponent(data.tenant.key)}`);
  /** The policy version the shown sets recorded: where the Repository crumb leads. */
  const policy = $derived('code' in data.sets || data.sets.result.state !== 'populated' ? undefined : (data.sets.result.data.selected ?? data.sets.result.data.sets[0]).policy.version);
  let actions = $state<Record<string, ActionState>>({});

  /** The URL of each navigation the review sets offer, so it is a real link. */
  function link(intent: ActionIntent): string | undefined {
    const input = intent.input as { generation?: string; execution_id?: string; card_ref?: CardRef };
    if (intent.name === 'review_set.open') return pullHref(base, data.name, data.number, input.generation);
    if (intent.name === 'record.open' && input.execution_id) return `${base}/verify/executions/${encodeURIComponent(input.execution_id)}`;
    if (intent.name !== 'card.open' || !input.card_ref) return undefined;
    return input.card_ref.kind === 'Repository' ? repositoryHref(base, input.card_ref) : `${base}/cards/detail${writeDetail(input.card_ref)}`;
  }

  async function onaction(intent: ActionIntent) {
    const valid = validateIntent('ReviewSets', intent);
    if ('code' in valid) return void (actions[intent.name] = { state: 'error', problem: valid });
    const to = link(valid);
    if (!to) return;
    actions[valid.name] = { state: 'pending' };
    await goto(to, valid.name === 'review_set.open' ? { keepFocus: true, noScroll: true } : undefined);
    delete actions[valid.name];
  }
</script>

<svelte:head><title>{data.name}#{data.number} · Repositories · Wyrd</title></svelte:head>
<Crumbs
  items={[
    { label: data.tenant.key },
    { label: 'Repositories', href: `${base}/repositories` },
    { label: data.name, ...(policy ? { href: repositoryHref(base, { kind: 'Repository', name: data.name, version: policy }) } : {}) },
    { label: `#${data.number}` }
  ]}
/>
<div class="content">
  {#if 'code' in data.sets}
    <EmptyState state="error" problem={data.sets} />
  {:else}
    <ReviewSets {...data.sets} labels={data.labels} {link} {actions} {onaction} />
  {/if}
  {#if requestable(data.sets)}<AccessRequest action="Read this Repository's pull requests" />{/if}
</div>

<style>
  .content {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 16px 24px 24px;
  }
  @media (max-width: 767px) {
    .content {
      padding: 16px;
    }
  }
</style>
