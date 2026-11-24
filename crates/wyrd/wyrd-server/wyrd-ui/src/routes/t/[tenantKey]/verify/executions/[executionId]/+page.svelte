<!--
  One exact verification execution: breadcrumb, refresh and copy link, then
  the shared inspector. The URL holds the selected node, invocation attempt,
  output and details, so reload, a copied link and Back restore the same
  selection; refresh rereads recorded state and keeps the graph where it is.
-->
<script lang="ts">
  import { goto, invalidateAll } from '$app/navigation';
  import { ActionButton, EmptyState, VerificationExecution, type ActionIntent, type ActionState, type CardRef, type Problem } from 'wyrd-ai-ui';
  import { validateIntent, type ExecutionInput } from 'wyrd-ai-ui/loaders';
  import { writeDetail } from '$lib/cards-host';
  import { pullHref } from '$lib/repositories-host';
  import { writeExecution } from '$lib/verify-host';
  import type { PageProps } from './$types';

  let { data }: PageProps = $props();
  const base = $derived(`/t/${encodeURIComponent(data.tenant.key)}`);
  const here = $derived(`${base}/verify/executions/${encodeURIComponent(data.executionId)}`);
  let actions = $state<Record<string, ActionState>>({});

  /** Navigate as one pending action. SvelteKit drops the answer of a navigation a newer one replaced; only the newest clears the pending state. */
  let navigations = 0;
  async function navigate(name: string, href: string, scroll = false) {
    const mine = ++navigations;
    actions[name] = { state: 'pending' };
    await goto(href, { keepFocus: true, noScroll: !scroll });
    if (mine === navigations) delete actions[name];
  }

  /** Reread recorded state in place: same URL, same component, same viewport. */
  async function refresh() {
    actions.refresh = { state: 'pending' };
    await invalidateAll();
    delete actions.refresh;
  }

  async function copy() {
    try {
      await navigator.clipboard.writeText(window.location.href);
      actions.copy = { state: 'success', message: 'Link copied' };
    } catch {
      actions.copy = { state: 'error', problem: { code: 'WYRD_UI_CLIPBOARD_UNAVAILABLE', title: 'The browser refused clipboard access; copy the address instead.' } };
    }
  }

  /** Cancel once: the control stays pending until the server answers, then recorded status is read back. */
  async function cancel() {
    if (actions['execution.cancel']?.state === 'pending') return;
    actions['execution.cancel'] = { state: 'pending' };
    const response = await fetch(`${here}/cancel`, { method: 'POST' }).catch(() => undefined);
    const body = (await response?.json().catch(() => null)) as { status?: unknown; code?: unknown } | null;
    await invalidateAll().catch(() => undefined);
    if (response?.ok && typeof body?.status === 'string') actions['execution.cancel'] = { state: 'success', message: `Recorded status: ${body.status.replace('_', ' ')}` };
    else if (body && typeof body.code === 'string') actions['execution.cancel'] = { state: response?.status === 401 || response?.status === 403 ? 'denied' : 'error', problem: body as unknown as Problem };
    else actions['execution.cancel'] = { state: 'error', problem: { code: 'WYRD_SPEC_500_INTERNAL', title: 'The cancel request did not reach the server.' } };
  }

  /**
   * Decide one exact occurrence once. The answer belongs to that occurrence
   * alone. After a lost or unavailable answer the status is read again and that
   * occurrence's recorded state decides what is reported; nothing is resent
   * until the reader asks.
   */
  async function decide(input: { execution_id: string; approval_id: string; decision: string; reason?: string }) {
    const name = 'approval.decide';
    if (actions[name]?.state === 'pending') return;
    if (input.execution_id !== data.executionId) return void (actions[name] = { state: 'error', problem: { code: 'WYRD_SPEC_400_VALIDATION', title: 'The decision names another execution than this page shows.' } });
    actions[name] = { state: 'pending' };
    const response = await fetch(`${here}/approvals/${encodeURIComponent(input.approval_id)}`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ decision: input.decision, ...(input.reason ? { reason: input.reason } : {}) })
    }).catch(() => undefined);
    const body = (await response?.json().catch(() => null)) as { status?: unknown; actor_id?: unknown; decided_at?: unknown; code?: unknown } | null;
    // A failed reread leaves the last status shown; the occurrence below then reads as unknown, never as pending.
    const reread = await invalidateAll().then(() => true, () => false);
    const shown = reread && data.execution?.result.state === 'populated' ? data.execution.result.data.status.approvals?.find((each) => each.approval_id === input.approval_id) : undefined;
    if (response?.ok && typeof body?.status === 'string')
      actions[name] = { state: 'success', message: `Accepted by the server: ${body.status} ${input.approval_id} (decided by ${body.actor_id} at ${body.decided_at}). The same execution continues; no new work was started.` };
    else if (response?.status === 409 && shown?.status === 'expired')
      actions[name] = { state: 'error', problem: { code: body?.code as string, title: `Refreshed: ${input.approval_id} has expired. Nothing was applied.` } };
    else if (response?.status === 409 && shown?.actor_id)
      actions[name] = { state: 'error', problem: { code: body?.code as string, title: `Refreshed: ${shown.actor_id} already ${shown.status} ${input.approval_id}. Your ${input.decision === 'approved' ? 'approval' : 'rejection'} was not applied.` } };
    else if (response && response.status < 500 && body && typeof body.code === 'string')
      actions[name] = { state: response.status === 401 || response.status === 403 ? 'denied' : 'error', problem: body as unknown as Problem };
    else {
      const code = typeof body?.code === 'string' ? body.code : 'WYRD_SPEC_500_INTERNAL';
      if (!shown) actions[name] = { state: 'error', problem: { code, title: `Whether ${input.approval_id} was decided is unknown: its status could not be read. Refresh before deciding again.` } };
      else if (shown.status === 'pending') actions[name] = { state: 'error', problem: { code, title: `No decision is recorded for ${input.approval_id}; submit it again to retry.` } };
      else actions[name] = { state: 'success', message: `Refreshed: the answer was lost; the server records ${shown.status} ${input.approval_id}${shown.actor_id ? ` by ${shown.actor_id} at ${shown.decided_at}` : ''}.` };
    }
  }

  function onaction(intent: ActionIntent) {
    const valid = validateIntent('VerificationExecution', intent);
    if ('code' in valid) return void (actions[intent.name] = { state: 'error', problem: valid });
    const shown = data.execution?.input;
    const input = valid.input as Record<string, unknown>;
    switch (valid.name) {
      case 'execution.select':
        return void navigate(valid.name, `${here}${writeExecution(input as Omit<ExecutionInput, 'executionId'>)}`);
      case 'execution.page':
        return void navigate(valid.name, `${here}${writeExecution({ ...shown, ...input })}`);
      case 'execution.open':
        return void navigate(valid.name, `${base}/verify/executions/${encodeURIComponent(input.execution_id as string)}`, true);
      case 'card.open':
        return void navigate(valid.name, `${base}/cards/detail${writeDetail(input.card_ref as CardRef)}`, true);
      case 'pull_request.open':
        return void navigate(valid.name, pullHref(base, input.repository as string, input.number as number, input.generation as string | undefined), true);
      case 'record.open':
        return void navigate(valid.name, `${base}/observe?${new URLSearchParams({ trace: input.trace_id as string, ...(input.span_id ? { span: input.span_id as string } : {}) })}`, true);
      case 'execution.cancel':
        return void cancel();
      case 'approval.decide':
        return void decide(input as Parameters<typeof decide>[0]);
      case 'publication.download': {
        const producer = input.producer as NonNullable<ExecutionInput['producer']>;
        const query = new URLSearchParams({ step: input.step_id as string, name: input.name as string, attempt: String(producer.attempt) });
        if (producer.item_index != null) query.set('item', String(producer.item_index));
        if (producer.iteration_index != null) query.set('iteration', String(producer.iteration_index));
        if (typeof input.path === 'string') query.set('path', input.path);
        // An attachment response leaves this page in place.
        return void window.open(`${here}/download?${query}`, '_self');
      }
    }
  }
</script>

<svelte:head><title>{data.executionId} · Verify · Wyrd</title></svelte:head>
<header class="bar">
  <span>Verify</span><span>/</span><span>Executions</span><span>/</span><span class="here">{data.executionId}</span>
  <span class="tools">
    <ActionButton variant="link" label="Refresh" state={actions.refresh} onclick={refresh} />
    <ActionButton variant="link" lead="⧉" label="Copy link" state={actions.copy} onclick={copy} />
  </span>
</header>
<div class="content">
  {#if data.problem}
    <EmptyState state="error" problem={data.problem} />
  {:else if data.execution}
    <VerificationExecution {...data.execution} {actions} {onaction} />
  {/if}
</div>

<style>
  .bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    min-height: 55px;
    padding: 10px 24px;
    border-bottom: 1px solid var(--border);
    background: var(--nav-surface);
    color: var(--muted);
    font: 12px var(--font-mono);
  }
  .here {
    color: var(--text);
    overflow-wrap: anywhere;
  }
  .tools {
    display: inline-flex;
    gap: 12px;
    margin-left: auto;
  }
  .content {
    display: flex;
    flex-direction: column;
    gap: 20px;
    padding: 20px 24px 32px;
  }
</style>
