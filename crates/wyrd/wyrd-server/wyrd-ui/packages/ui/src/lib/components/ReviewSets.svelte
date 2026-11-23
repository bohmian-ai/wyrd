<!--
  One pull request's review sets: every retained set newest first, the pinned
  or latest one selected, and that set's frozen members. Each member keeps its
  own status, verdict and failed requirements; a set has no combined verdict.
  A member whose record is not retained is listed as missing, and a pinned set
  that is not retained is reported, never replaced by the latest. Nothing here
  requests a review or reruns one.
-->
<script lang="ts">
  import type { ActionProps } from '../contract.js';
  import { fmtInstant, fmtShortId, parseInstant } from '../format.js';
  import { EXECUTION_STATUS, EXECUTION_VERDICT, REVIEW_SET_ROWS, type ReviewSet, type ReviewSetsProps } from '../observe.js';
  import ActionNote from './ActionNote.svelte';
  import EmptyState from './EmptyState.svelte';
  import RepositorySheet from './RepositorySheet.svelte';

  /** `labels` names the Verifier Card UIDs the host read; an unnamed one shows its UID. */
  let { input, result, actions = {}, onaction, labels = {} }: ReviewSetsProps & ActionProps & { labels?: Readonly<Record<string, string>> } = $props();

  const disabled = $derived(!onaction);
  const pull = $derived(`${input.repository}#${input.number}`);
  const show = (generation?: string) => onaction?.({ name: 'review_set.open', input: generation ? { generation } : {} });
  /** What started a set: a review someone requested, or a pull request event GitHub delivered. */
  const occurrence = (set: ReviewSet) => (set.request_key ? 'requested review' : 'pull request event');
  const head = (set: ReviewSet) => `head ${set.head.sha.slice(0, 7)}`;
</script>

<RepositorySheet>
  {#if result.state === 'populated'}
    {@const data = result.data}
    {@const set = data.selected}
    {@const source = set ?? data.sets[0]}
    <div class="xhead">
      <div class="xtitle">
        <div class="xname"><span class="kt">Pull request</span>{#if source.title}<h1>{source.title}</h1>{:else}<h1 class="mono">#{input.number}</h1>{/if}</div>
        <div class="xmeta">
          <button type="button" class="link" {disabled} onclick={() => source.policy.version && onaction?.({ name: 'card.open', input: { card_ref: { kind: 'Repository', name: input.repository, version: source.policy.version } } })}>{input.repository}</button>
          <span class="sep">·</span>#{input.number}<span class="sep">·</span>Repository Card {input.repository}
        </div>
      </div>
      <div class="xactions"><ActionNote state={actions['review_set.open'] ?? actions['record.open'] ?? actions['card.open']} /></div>
    </div>

    <section class="section" aria-labelledby="review-sets-heading">
      <div class="section-head"><h2 class="h" id="review-sets-heading">Review sets · {data.sets.length}{data.more ? '+' : ''}</h2></div>
      <nav class="sets" aria-label="Review sets of #{input.number}">
        {#each data.sets as each (each.generation)}
          {@const count = each.member_ids.length}
          <button type="button" class="setrow" {disabled} aria-current={each.generation === set?.generation ? 'true' : undefined} onclick={() => show(each.generation === data.current ? undefined : each.generation)}>
            <span class="mono" title={each.generation}>{fmtShortId(each.generation)}</span>
            <span class="small">{occurrence(each)}</span>
            <span class="mono small" title={each.head.sha}>{head(each)}</span>
            <span class="mono small muted">{fmtInstant(parseInstant(each.recorded_at))}</span>
            <span class="mono small muted">{each.policy.name}@{each.policy.version ?? '—'}</span>
            <span class="small muted push">{count} {count === 1 ? 'execution' : 'executions'}{each.generation === data.current ? ' · latest' : ''}</span>
          </button>
        {/each}
      </nav>
      {#if data.more}<p class="note">The newest {REVIEW_SET_ROWS} member records are listed. Open an earlier set by its link.</p>{/if}
    </section>

    {#if !set}
      <div class="banner warn" role="status">
        Review set {fmtShortId(input.generation ?? '')} is not retained for this pull request.
        <button type="button" class="link" {disabled} onclick={() => show()}>Latest set {fmtShortId(data.current)}</button>
      </div>
    {:else}
      {#if input.generation && set.generation !== data.current}
        <div class="banner" role="status">
          You are viewing earlier review set {fmtShortId(set.generation)}. This link stays on it.
          <button type="button" class="link" {disabled} onclick={() => show()}>Latest set {fmtShortId(data.current)}</button>
        </div>
      {:else if input.generation}
        <div class="banner" role="status">
          Pinned to {fmtShortId(set.generation)}, the latest set when this link was made.
          <button type="button" class="link" {disabled} onclick={() => show()}>Follow the latest set</button>
        </div>
      {/if}
      <section class="section" aria-labelledby="review-members-heading">
        <div class="section-head"><h2 class="h" id="review-members-heading">{fmtShortId(set.generation)} · {occurrence(set)} · {head(set)}</h2></div>
        {#if !set.complete}
          <p class="note box">{set.members.filter((member) => member.retained).length} of {set.members.length} member records are retained. This set is incomplete.</p>
        {/if}
        <div class="outs-wrap" role="region" aria-label="Members of review set {fmtShortId(set.generation)}" tabindex="-1">
          <table class="outs">
            <thead>
              <tr><th scope="col">Execution</th><th scope="col">Verifier</th><th scope="col">Status</th><th scope="col">Verdict</th><th scope="col">Failed requirements</th></tr>
            </thead>
            <tbody>
              {#each set.members as member (member.execution_id)}
                {@const status = member.status ? (EXECUTION_STATUS[member.status] ?? ['', member.status, 'text']) : undefined}
                {@const verdict = member.verdict ? EXECUTION_VERDICT[member.verdict] : undefined}
                {@const verifier = member.verifier_card_uid}
                <tr>
                  <td><div class="cellbox"><button type="button" class="link mono" {disabled} title={member.execution_id} onclick={() => onaction?.({ name: 'record.open', input: { execution_id: member.execution_id } })}>{fmtShortId(member.execution_id)}</button></div></td>
                  <td><div class="cellbox">{#if verifier}<span class="mono" title={verifier}>{labels[verifier] ?? fmtShortId(verifier)}</span>{:else}<span class="muted mono">—</span>{/if}</div></td>
                  <td><div class="cellbox">{#if status}<span class="st c-{status[2]}"><span aria-hidden="true">{status[0]}</span>{status[1]}</span>{:else}<span class="muted">record not retained</span>{/if}</div></td>
                  <td><div class="cellbox">{#if verdict}<span class="badge c-{verdict[2]}"><span aria-hidden="true">{verdict[0]}</span>{verdict[1]}</span>{:else}<span class="muted">none</span>{/if}</div></td>
                  <td><div class="cellbox">{#if member.failed.length}<span class="mono c-danger">{member.failed.join(', ')}</span>{:else}<span class="muted mono">—</span>{/if}</div></td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
        <p class="note">Each Verifier records its own status and verdict. A review set has no combined verdict.</p>
      </section>
    {/if}
  {:else if result.state === 'loading'}
    <EmptyState state="loading" title="Loading review sets of {pull}" />
  {:else if result.state === 'empty'}
    <EmptyState state="empty" title={result.reason} />
  {:else}
    <EmptyState state={result.state} problem={result.problem} />
  {/if}
</RepositorySheet>

<style>
  .sets {
    display: flex;
    flex-direction: column;
    border: 1px solid var(--border);
    border-radius: var(--r);
    background: var(--surface);
  }
  .setrow {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 4px 14px;
    padding: 8px 12px;
    border: 0;
    border-bottom: 1px solid var(--border-soft);
    background: transparent;
    color: var(--text);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .setrow:last-child {
    border-bottom: 0;
  }
  .setrow:hover {
    background: var(--surface-hover);
  }
  .setrow[aria-current='true'] {
    background: var(--declare-soft);
    box-shadow: inset 2px 0 0 var(--declare);
  }
  .push {
    margin-left: auto;
  }
</style>
