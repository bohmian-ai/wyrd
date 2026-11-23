<!--
  One exact Repository Card version: its policy and access, then this
  Repository's executions from the population Verify reads. The URL holds
  the viewed version and declaration tab; viewing a version changes no
  selection. A row opens its exact execution page, and its pull request
  opens the review set that admitted it.
-->
<script lang="ts">
  import { requestable } from '$lib/access';
  import AccessRequest from '$lib/AccessRequest.svelte';
  import { goto } from '$app/navigation';
  import { EmptyState, RecordTable, RepositoryPolicy, type ActionIntent, type ActionState, type CardRef } from 'wyrd-ai-ui';
  import { validateIntent, type ComponentName } from 'wyrd-ai-ui/loaders';
  import Crumbs from '$lib/Crumbs.svelte';
  import { writeDetail } from '$lib/cards-host';
  import { pullHref, repositoryHref, verifyHref } from '$lib/repositories-host';
  import type { PageProps } from './$types';

  let { data }: PageProps = $props();
  const base = $derived(`/t/${encodeURIComponent(data.tenant.key)}`);
  const shown = $derived('code' in data.policy ? undefined : data.policy.input);
  let actions = $state<Record<string, ActionState>>({});

  function onaction(component: ComponentName) {
    return async (intent: ActionIntent) => {
      const valid = validateIntent(component, intent);
      if ('code' in valid) return void (actions[intent.name] = { state: 'error', problem: valid });
      if (!shown) return;
      const input = valid.input as { card_ref?: CardRef; declaration?: string; execution_id?: unknown; repository?: string; number?: number; generation?: string };
      const here = { declaration: shown.declaration, back: data.back || undefined };
      const stay = { keepFocus: true, noScroll: true };
      actions[valid.name] = { state: 'pending' };
      if (valid.name === 'card.version' && input.card_ref) await goto(repositoryHref(base, input.card_ref, here), stay);
      else if (valid.name === 'repository.declaration') await goto(repositoryHref(base, shown.cardRef, { ...here, declaration: input.declaration }), stay);
      else if (valid.name === 'card.open' && input.card_ref) await goto(`${base}/cards/detail${writeDetail(input.card_ref)}`);
      else if (valid.name === 'connection.open') await goto(`${base}/settings/integrations/github`);
      else if (valid.name === 'pull_request.open' && input.repository && input.number) await goto(pullHref(base, input.repository, input.number, input.generation));
      else if (valid.name === 'record.open' && typeof input.execution_id === 'string') await goto(`${base}/verify/executions/${encodeURIComponent(input.execution_id)}`);
      else return void (actions[valid.name] = { state: 'unavailable', reason: 'Open Verify to sort, filter or page these executions.' });
      delete actions[valid.name];
    };
  }
</script>

<svelte:head><title>{data.policy && 'input' in data.policy ? data.policy.input.cardRef.name : 'Repository'} · Repositories · Wyrd</title></svelte:head>
<Crumbs items={[{ label: data.tenant.key }, { label: 'Repositories', href: `${base}/repositories${data.back}` }, { label: shown?.cardRef.name ?? 'Repository' }]} />
<div class="content">
  {#if 'code' in data.policy}
    <EmptyState state="error" problem={data.policy} />
  {:else}
    <RepositoryPolicy {...data.policy} {actions} onaction={onaction('RepositoryPolicy')} />
    {#if data.activity && shown}
      <section aria-labelledby="activity-heading" aria-label="Verification activity">
        <div class="head">
          <h2 id="activity-heading">Verification activity</h2>
          <a class="app-control" href={verifyHref(base, shown.cardRef.name)}>View all in Verify</a>
        </div>
        {#if 'code' in data.activity}
          <EmptyState state="error" problem={data.activity} />
        {:else}
          <RecordTable {...data.activity} labels={data.labels} {actions} onaction={onaction('RecordTable')} />
        {/if}
        <p class="note">Each row is one exact execution of this Repository in the last 30 days, pinned to its Repository version and Verifier version. Cancelled or completed executions keep their recorded evidence.</p>
      </section>
    {/if}
  {/if}
  {#if requestable(data.policy)}<AccessRequest action="Read this Repository" />{/if}
</div>

<style>
  .content {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 16px 24px 24px;
  }
  section {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }
  .head {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 12px;
  }
  h2 {
    margin: 0 auto 0 0;
    color: var(--muted);
    font: 400 11px var(--font-mono);
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }
  .note {
    max-width: 80ch;
    margin: 0;
    color: var(--muted);
    font-size: 13px;
  }
  @media (max-width: 767px) {
    .content {
      padding: 16px;
    }
  }
</style>
