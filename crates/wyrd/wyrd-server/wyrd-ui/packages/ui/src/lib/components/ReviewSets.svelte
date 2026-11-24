<!--
  One pull request's review sets: every retained set newest first, the pinned
  or latest one selected, and that set's frozen members. Each member keeps its
  own status, verdict and failed requirements; a set has no combined verdict.
  A member whose record is not retained is listed as missing, and a pinned set
  that is not retained is reported, never replaced by the latest. Nothing here
  requests a review or reruns one.
-->
<script lang="ts">
  import type { ActionIntent, ActionProps } from '../contract.js';
  import { fmtInstant, fmtShortId, parseInstant } from '../format.js';
  import { REVIEW_SET_ROWS, type ReviewSet, type ReviewSetsProps } from '../observe.js';
  import ActionNote from './ActionNote.svelte';
  import EmptyState from './EmptyState.svelte';
  import StatusMark from './StatusMark.svelte';
  import RepositorySheet from './RepositorySheet.svelte';

  /**
   * `labels` names the Verifier Card UIDs the host read; an unnamed one shows its UID.
   * `link` is the URL a routing host gives an intent: each navigation is then a real link. Without one, it is a button.
   */
  let { input, result, actions = {}, onaction, labels = {}, link }: ReviewSetsProps & ActionProps & { labels?: Readonly<Record<string, string>>; link?: (intent: ActionIntent) => string | undefined } = $props();

  const disabled = $derived(!onaction);
  const pull = $derived(`${input.repository}#${input.number}`);
  const openSet = (generation?: string): ActionIntent => ({ name: 'review_set.open', input: generation ? { generation } : {} });
  /** A link to `intent` where the host routes it, else a button that emits it: the element and its attributes. */
  const go = (intent: ActionIntent) => {
    const href = link?.(intent);
    return href ? { tag: 'a', attrs: { href } } : { tag: 'button', attrs: { type: 'button', disabled, onclick: () => onaction?.(intent) } };
  };
  /** The exact Verifier definition a named member links to. */
  const verifierRef = (label?: string) => {
    const at = label?.lastIndexOf('@') ?? -1;
    return label && at > 0 ? { kind: 'Verifier', name: label.slice(0, at), version: label.slice(at + 1) } : undefined;
  };
  /**
   * What started a set, from what it recorded: a requested review is a rerequest, a head that differs from the set
   * before it is a new head, and the first retained set of a fully read history is the opening. Anything else is a
   * pull request event; GitHub's action is not retained.
   */
  function occurrence(sets: readonly ReviewSet[], more: boolean, set: ReviewSet, at: number) {
    if (set.request_key) return 'rerequested';
    const before = at < 0 ? undefined : sets[at + 1];
    if (before) return before.head.sha === set.head.sha ? 'pull request event' : 'new head';
    return at === sets.length - 1 && !more ? 'opened' : 'pull request event';
  }
  const head = (set: ReviewSet) => `head ${set.head.sha.slice(0, 7)}`;
</script>

{#snippet nav(intent: ActionIntent, text: string, title?: string)}{@const to = go(intent)}<svelte:element this={to.tag} class={title ? 'link mono' : 'link'} {title} {...to.attrs}>{text}</svelte:element>{/snippet}

<RepositorySheet>
  {#if result.state === 'populated'}
    {@const data = result.data}
    {@const set = data.selected}
    {@const source = set ?? data.sets[0]}
    <div class="xhead">
      <div class="xtitle">
        <div class="xname"><span class="kt">Pull request</span>{#if source.title}<h1>{source.title}</h1>{:else}<h1 class="mono">#{input.number}</h1>{/if}</div>
        <div class="xmeta">
          {#if source.policy.version}{@render nav({ name: 'card.open', input: { card_ref: { kind: 'Repository', name: input.repository, version: source.policy.version } } }, input.repository)}{:else}{input.repository}{/if}<span class="sep">·</span>#{input.number}<span class="sep">·</span>Repository Card {input.repository}
        </div>
      </div>
      <div class="xactions">
        {#if source.full_name}<a class="btn btn-sm" href="https://{source.host}/{source.full_name}/pull/{input.number}" target="_blank" rel="noopener noreferrer">{source.full_name}#{input.number} ↗</a>{/if}
        <ActionNote state={actions['review_set.open'] ?? actions['record.open'] ?? actions['card.open']} />
      </div>
    </div>

    <section class="section" aria-labelledby="review-sets-heading">
      <div class="section-head"><h2 class="h" id="review-sets-heading">Review sets · {data.sets.length}{data.more ? '+' : ''}</h2></div>
      <nav class="sets" aria-label="Review sets of #{input.number}">
        {#each data.sets as each, at (each.generation)}
          {@const count = each.member_ids.length}
          {@const to = go(openSet(each.generation === data.current ? undefined : each.generation))}
          <svelte:element this={to.tag} class="setrow" aria-current={each.generation === set?.generation ? 'true' : undefined} {...to.attrs}>
            <span class="mono" title={each.generation}>{fmtShortId(each.generation)}</span>
            <span class="small">{occurrence(data.sets, data.more, each, at)}</span>
            <span class="mono small" title={each.head.sha}>{head(each)}</span>
            <span class="mono small muted">{fmtInstant(parseInstant(each.recorded_at))}</span>
            <span class="mono small muted">{each.policy.name}@{each.policy.version ?? '—'}</span>
            <span class="small muted push">{count} {count === 1 ? 'execution' : 'executions'}{each.generation === data.current ? ' · latest' : ''}</span>
          </svelte:element>
        {/each}
      </nav>
      {#if data.more}<p class="note">The newest {REVIEW_SET_ROWS} member records are listed. Open an earlier set by its link.</p>{/if}
    </section>

    {#if !set}
      <div class="banner warn" role="status">
        Review set {fmtShortId(input.generation ?? '')} is not retained for this pull request.
        {@render nav(openSet(), `Latest set ${fmtShortId(data.current)}`)}
      </div>
    {:else}
      {#if input.generation && set.generation !== data.current}
        <div class="banner" role="status">
          You are viewing earlier review set {fmtShortId(set.generation)}. This link stays on it.
          {@render nav(openSet(), `Latest set ${fmtShortId(data.current)}`)}
        </div>
      {:else if input.generation}
        <div class="banner" role="status">
          Pinned to {fmtShortId(set.generation)}, the latest set when this link was made.
          {@render nav(openSet(), 'Follow the latest set')}
        </div>
      {/if}
      <section class="section" aria-labelledby="review-members-heading">
        <div class="section-head"><h2 class="h" id="review-members-heading">{fmtShortId(set.generation)} · {occurrence(data.sets, data.more, set, data.sets.findIndex((each) => each.generation === set.generation))} · {head(set)}</h2></div>
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
                {@const verifier = member.verifier_card_uid}
                {@const definition = verifierRef(verifier ? labels[verifier] : undefined)}
                <tr>
                  <td><div class="cellbox">{@render nav({ name: 'record.open', input: { execution_id: member.execution_id } }, fmtShortId(member.execution_id), member.execution_id)}</div></td>
                  <td><div class="cellbox">{#if verifier && definition}{@render nav({ name: 'card.open', input: { card_ref: definition } }, labels[verifier], verifier)}{:else if verifier}<span class="mono" title={verifier}>{fmtShortId(verifier)}</span>{:else}<span class="muted mono">—</span>{/if}</div></td>
                  <td><div class="cellbox">{#if member.status}<StatusMark status={member.status} />{:else}<span class="muted">record not retained</span>{/if}</div></td>
                  <td><div class="cellbox">{#if member.verdict}<StatusMark verdict={member.verdict} />{:else}<span class="muted">none</span>{/if}</div></td>
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
    text-decoration: none;
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
