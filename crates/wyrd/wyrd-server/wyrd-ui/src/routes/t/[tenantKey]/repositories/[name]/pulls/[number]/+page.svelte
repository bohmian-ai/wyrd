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
  import AccessRequest from '$lib/AccessRequest.svelte';
  import Crumbs from '$lib/Crumbs.svelte';
  import { pullHref, repositoryHref } from '$lib/repositories-host';
  import type { PageProps } from './$types';

  let { data }: PageProps = $props();
  const base = $derived(`/t/${encodeURIComponent(data.tenant.key)}`);
  /** The policy version the shown sets recorded: where the Repository crumb leads. */
  const policy = $derived('code' in data.sets || data.sets.result.state !== 'populated' ? undefined : (data.sets.result.data.selected ?? data.sets.result.data.sets[0]).policy.version);
  let actions = $state<Record<string, ActionState>>({});

  async function onaction(intent: ActionIntent) {
    const valid = validateIntent('ReviewSets', intent);
    if ('code' in valid) return void (actions[intent.name] = { state: 'error', problem: valid });
    const input = valid.input as { generation?: string; execution_id?: string; card_ref?: CardRef };
    actions[valid.name] = { state: 'pending' };
    if (valid.name === 'review_set.open') await goto(pullHref(base, data.name, data.number, input.generation), { keepFocus: true, noScroll: true });
    else if (valid.name === 'record.open' && input.execution_id) await goto(`${base}/verify/executions/${encodeURIComponent(input.execution_id)}`);
    else if (valid.name === 'card.open' && input.card_ref) await goto(repositoryHref(base, input.card_ref));
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
    <ReviewSets {...data.sets} labels={data.labels} {actions} {onaction} />
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
