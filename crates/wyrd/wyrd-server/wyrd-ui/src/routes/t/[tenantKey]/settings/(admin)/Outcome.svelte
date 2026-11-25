<!--
  The result of one access action, from the action's own response: a refusal
  (nothing changed), a confirmation, a new key or new invitation links. Keys
  and links exist only in this result; dismissing or reloading discards them.
-->
<script lang="ts">
  import { fmtInstant } from 'wyrd-ai-ui';
  import { guidance } from '$lib/Entry.svelte';
  import type { WyrdProblem } from '$lib/server/problem';
  type Sent = { recipient: string; link: string; delivery: 'not_emailed' | 'accepted' | 'failed'; expiresAt: string };
  type Form = { problem?: WyrdProblem; done?: string; issued?: { id: string; credential: string }; invitations?: Sent[] } | null | undefined;
  let { form }: { form: Form } = $props();
  let dismissed = $state<string | null>(null);
  let copied = $state(false);
  const issued = $derived(form?.issued && form.issued.id !== dismissed ? form.issued : null);
  const sent = $derived(form?.invitations && form.invitations[0]?.link !== dismissed ? form.invitations : null);
  const DELIVERY = { not_emailed: 'Not emailed', accepted: 'Emailed', failed: 'Not delivered' } as const;
  async function copy(text: string) {
    await navigator.clipboard.writeText(text);
    copied = true;
  }
</script>

{#if form?.problem}
  <div class="app-problem" role="alert" data-state={form.problem.status === 403 ? 'unauthorized' : 'error'}>
    <strong>{form.problem.status === 403 ? '⊘' : '✕'} {form.problem.title}</strong>
    <p>{guidance(form.problem)} Nothing was changed.</p>
    <code>{form.problem.code}</code>
  </div>
{:else if sent}
  <div class="note ok" role="status" aria-label="Invitation links">
    <span class="g" aria-hidden="true">✓</span>
    <div class="grow">
      <b>{sent.length === 1 ? '1 invitation sent.' : `${sent.length} invitations sent.`}</b>
      <span class="muted">Each link works once and expires {fmtInstant(Date.parse(sent[0].expiresAt))}. Copy them now if email is off; they are not shown again.</span>
      {#if sent.some((one) => one.delivery === 'not_emailed')}<span class="muted">Wyrd does not email invitations until an operator configures a mail relay.</span>{/if}
      <ul class="links">
        {#each sent as one (one.link)}
          <li><span>{one.recipient}</span> <span class="muted">{DELIVERY[one.delivery]}</span> <code class="mono" data-secret>{one.link}</code></li>
        {/each}
      </ul>
    </div>
    <button class="app-control" type="button" onclick={() => copy(sent.map((one) => `${one.recipient}\t${one.link}`).join('\n'))}>{copied ? 'Copied' : 'Copy links'}</button>
    <button class="app-control" type="button" onclick={() => ((dismissed = sent[0].link), (copied = false))}>Dismiss</button>
  </div>
{:else if issued}
  <div class="note ok" role="status">
    <span class="g" aria-hidden="true">✓</span>
    <div class="grow">
      <b>Key created.</b> <span class="muted">Copy it now; it is not shown again.</span>
      <code class="mono secret" data-secret>{issued.credential}</code>
    </div>
    <button class="app-control" type="button" onclick={() => copy(issued.credential)}>{copied ? 'Copied' : 'Copy'}</button>
    <button class="app-control" type="button" onclick={() => ((dismissed = issued.id), (copied = false))}>Dismiss</button>
  </div>
{:else if form?.done}
  <div class="note ok" role="status"><span class="g" aria-hidden="true">✓</span><div class="grow">{form.done}</div></div>
{/if}

<style>
  .links {
    display: grid;
    gap: 4px;
    margin-top: 8px;
  }
  .links li {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  code {
    overflow-wrap: anywhere;
  }
  .secret {
    display: block;
    margin-top: 6px;
  }
  .app-problem {
    margin-bottom: 20px;
  }
</style>
