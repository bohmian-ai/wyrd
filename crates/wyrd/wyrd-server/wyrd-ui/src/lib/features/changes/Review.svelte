<script lang="ts">
  import { MediaQuery } from 'svelte/reactivity';
  const narrow = new MediaQuery('(max-width: 600px)');
  let summaryOpen = $state(true);
  $effect(() => {
    summaryOpen = !narrow.current;
  });
  import { enhance } from '$app/forms';
  import Panel from '$lib/components/Panel.svelte';
  import Badge from '$lib/components/Badge.svelte';
  import Composer from './Composer.svelte';
  import StatusIcon from './StatusIcon.svelte';
  import Markdown from './Markdown.svelte';
  import type { ChangeView, ActionResult, Anchor, Thread, TimelineEvent } from './types';
  import { claimState } from './status';
  let {
    view,
    csrf,
    base,
    actor,
    result
  }: {
    view: ChangeView;
    csrf: string;
    base: string;
    actor: string;
    result?: ActionResult | null;
  } = $props();
  let change = $derived(view.change);
  let approved = $derived.by(() => {
    if (change.approval.startsWith('Approved')) return true;
    const match = change.approval.match(/(\d+) of (\d+)/);
    return !!match && match[1] === match[2];
  });
  let feed = $derived(
    [
      ...change.threads.map((thread) => ({
        at: thread.comments[0]?.revisions[0]?.at ?? '',
        thread,
        event: null as TimelineEvent | null
      })),
      ...change.timeline
        .filter((event) => ['revision', 'decision', 'lifecycle'].includes(event.kind))
        .map((event) => ({ at: event.at, thread: null as Thread | null, event }))
    ].sort((a, b) => a.at.localeCompare(b.at))
  );
  function anchorLink(anchor: Anchor): string {
    if (anchor.kind === 'source' || anchor.kind === 'subject')
      return `${base}/subjects/${anchor.target}?revision=${anchor.revision}${anchor.file ? `&file=${encodeURIComponent(anchor.file)}` : ''}${anchor.line ? `#line-${anchor.side}-${anchor.line}` : ''}`;
    if (['claim', 'evidence', 'result'].includes(anchor.kind))
      return `${base}/verification?revision=${anchor.revision}#${anchor.kind === 'result' ? 'result-' : ''}${anchor.target}`;
    return `${base}?revision=${anchor.revision}`;
  }
</script>
{#snippet renderThread(thread: Thread)}<section id={thread.id}><Panel title={thread.anchor.kind === 'change' ? `${view.mentions.find(person => person.id === thread.comments[0]?.author)?.name ?? thread.comments[0]?.author} commented` : `${thread.anchor.file ? thread.anchor.file.split('/').at(-1) + (thread.anchor.line ? ':' + thread.anchor.line : '') : thread.anchor.target} · ${thread.anchor.kind}`}>{#snippet head()}{#if thread.anchor.kind !== 'change'}<a href={anchorLink(thread.anchor)}>Anchor →</a>{/if}<Badge tone={thread.resolved ? 'ok' : 'running'}>{thread.resolved ? 'Resolved' : 'Open'}</Badge>{/snippet}
    {#each thread.comments as comment}{@const current = comment.revisions.at(-1)!}<article class="comment"><div class="row between"><strong>{view.mentions.find(person => person.id === comment.author)?.name ?? comment.author}</strong><time class="mono muted" datetime={current.at} title={current.at}>{current.at.slice(5,16).replace('T',' ')}Z</time></div><Markdown body={current.body} />
      <div class="comment-actions">{#if comment.revisions.length > 1}<details class="history"><summary>Edit history · {comment.revisions.length} revisions</summary>{#each comment.revisions as revision}<div class="notice"><p class="mono">{revision.editor} · <time datetime={revision.at}>{revision.at}</time></p><Markdown body={revision.body} /></div>{/each}</details>{/if}
      {#if comment.author === actor && view.capabilities.review}<details class="edit" open={result?.commentId === comment.id && !!result?.problem}><summary>Edit</summary><Composer {view} {csrf} operation="edit" threadId={thread.id} commentId={comment.id} expected={current.id} initial={current.body} label="Save edit" result={result?.commentId === comment.id ? result : null} /></details>{/if}</div>
    </article>{/each}
    <div class="thread-actions"><details class="reply"><summary>Reply · @mention</summary><Composer {view} {csrf} operation="reply" threadId={thread.id} commentId={thread.comments[0]?.id} label="Reply" result={result?.threadId === thread.id && !result?.commentId ? result : null} /></details>
    <form class="resolve" method="POST" action="?/review" use:enhance><input type="hidden" name="csrf" value={csrf} /><input type="hidden" name="revision" value={change.revision} /><input type="hidden" name="requestKey" value={`${view.requestKey}:${thread.id}`} /><input type="hidden" name="threadId" value={thread.id} /><input type="hidden" name="operation" value={thread.resolved ? 'reopen' : 'resolve'} /><button class="text-control" disabled={!view.capabilities.review}>{thread.resolved ? 'Reopen' : 'Resolve thread'}</button></form>
    {#if thread.transitions.length}<details class="transitions"><summary>Resolution history</summary>{#each thread.transitions as transition}<p class="mono">{transition.resolved ? 'Resolved' : 'Reopened'} by {transition.actor} · {transition.at}</p>{/each}</details>{/if}</div>
  </Panel></section>{/snippet}
{#snippet renderEvent(event: TimelineEvent)}<p class="feed-event mono"><span class="event-kind">{event.kind}</span> {event.text} — {event.actor} · <time datetime={event.at}>{event.at.slice(5, 16).replace('T', ' ')}Z</time> · <a href={base + event.destination}>Open →</a></p>{/snippet}
<div class="columns review"><div class="stack">
  {#each feed.slice(0, narrow.current ? 2 : undefined) as item, i (item.thread?.id ?? item.event?.id ?? i)}{#if item.thread}{@render renderThread(item.thread)}{:else if item.event}{@render renderEvent(item.event)}{/if}{/each}
  {#if narrow.current && feed.length > 2}<details class="more-threads"><summary>More activity · {feed.length - 2}</summary><div class="stack">{#each feed.slice(2) as item, i (item.thread?.id ?? item.event?.id ?? i)}{#if item.thread}{@render renderThread(item.thread)}{:else if item.event}{@render renderEvent(item.event)}{/if}{/each}</div></details>{/if}
  {#if change.lifecycle !== 'closed'}<Panel title={`Comment on this Change · revision ${change.revisionNumber}`}><Composer {view} {csrf} result={result?.threadId ? null : result} /></Panel>{/if}
  <section id="submit-review"><Panel title={`Submit review — revision ${change.revisionNumber}`} variant="accent">
    <div class="review-status">
      <div class="rs-row"><StatusIcon state={change.verification.tone === 'ok' ? 'pass' : 'fail'} size={20} /><div><strong>{change.verification.label === 'Needs attention' ? 'Not verified' : change.verification.label}</strong><p>{change.satisfied} of {change.total} required Claims satisfied · <a href={`${base}/verification?revision=${encodeURIComponent(change.revision)}`}>Open Verification →</a></p></div></div>
      <ul class="claim-rows">{#each change.claims as claim}<li><StatusIcon state={claimState(claim)} /><span class="claim-name"><strong>{claim.title}</strong><span class="muted">{` — ${claim.resolution.replaceAll('_', ' ')}`}</span></span><a href={`${base}/verification?revision=${encodeURIComponent(change.revision)}#${claim.id}`}>Details</a></li>{/each}</ul>
      <div class="rs-row"><StatusIcon state={approved ? 'pass' : 'attention'} size={20} /><div><strong>Approval — {change.approval.toLowerCase()}</strong><p>Approving records judgment on revision {change.revisionNumber} — it never runs Verifiers.</p></div></div>
    </div>
    {#if result?.message && !result.threadId && !result.commentId}<p role="status" class="flash">✓ {result.message}</p>{/if}
    {#if change.lifecycle === 'closed'}<p class="muted divider">Closed — {change.closure ?? 'completed'}. This Change Request no longer accepts reviews or comments.</p>{:else}<form method="POST" action="?/review" class="stack review-form divider" use:enhance><input type="hidden" name="csrf" value={csrf} /><input type="hidden" name="revision" value={change.revision} /><input type="hidden" name="requestKey" value={`${view.requestKey}:decision`} /><input type="hidden" name="operation" value="decision" /><details bind:open={summaryOpen}><summary>Optional summary</summary><textarea name="body" maxlength="10000" aria-label="Review summary"></textarea></details><fieldset class="stack"><legend>Review decision</legend>{#each [['comment','Comment','Feedback without a decision'],['approve','Approve','Records approval — never verifies or merges'],['request-changes','Request changes','Blocks until a new revision addresses feedback']] as [value,title,detail]}<label class="decision"><input type="radio" name="decision" {value} checked={value === 'comment'} /><span><strong>{title}</strong><p>{detail}</p></span></label>{/each}</fieldset><button class="control primary" disabled={!view.capabilities.review}>Submit review</button><small>Reviews exactly revision {change.revisionNumber}. A new revision requires re-review.</small></form>{/if}{#if change.lifecycle === 'open' && view.capabilities.write}<form id="close" method="POST" action="?/review" class="close-form divider" use:enhance><input type="hidden" name="csrf" value={csrf} /><input type="hidden" name="revision" value={change.revision} /><input type="hidden" name="requestKey" value={`${view.requestKey}:close`} /><input type="hidden" name="operation" value="close" /><select name="decision" aria-label="Closure reason"><option value="completed">Completed</option><option value="cancelled">Cancelled</option></select><button class="control" class:primary={approved}>Close Change Request</button><small class="muted">Records closure on the server. Provider pull requests stay unchanged.</small></form>{/if}</Panel></section>
  {#if result?.problem}<div role="alert" class="notice"><p>{result.problem.title}</p><p class="mono">{result.problem.code}</p><p>Your draft is preserved in its composer.</p></div>{/if}
</div><aside class="stack">
  <Panel title="Participants">{#each view.mentions.filter(person => person.kind === 'user') as person}<p>@{person.name} · {person.team}</p>{/each}</Panel>
</aside></div>
<style>
  .review-form {
    gap: 8px;
  }
  .flash {
    margin: 10px 0 0;
    padding: 8px 10px;
    border: 2px solid var(--ok);
    border-radius: var(--r);
    color: var(--ok-text);
    font-weight: 600;
  }
  .close-form {
    display: grid;
    gap: 6px;
    justify-items: start;
  }
  .review-status {
    display: grid;
    gap: 12px;
  }
  .claim-rows {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
  }
  .claim-rows li {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 0 8px 30px;
    border-top: 1px solid var(--sep);
    font-size: 12px;
  }
  .claim-rows .claim-name {
    flex: 1;
    min-width: 0;
  }
  .claim-rows a {
    font: 700 10px var(--font-mono);
  }
  .rs-row {
    display: flex;
    align-items: flex-start;
    gap: 10px;
  }
  .rs-row strong {
    font-size: 13px;
  }
  .rs-row p {
    margin: 2px 0 0;
    font-size: 12px;
    color: var(--muted);
  }
  .review-form fieldset {
    gap: 8px;
  }
  .review-form .decision {
    font-size: 12px;
  }
  .review-form .decision p {
    margin: 2px 0;
  }
  .review :global(.divider) {
    margin-top: 8px;
    padding-top: 8px;
  }
  .comment {
    margin-top: 10px;
  }
  .feed-event {
    font-size: 11px;
    color: var(--muted);
    padding-left: 4px;
    border-left: 2px dashed var(--border);
  }
  .feed-event .event-kind {
    color: var(--brand-strong);
    font-weight: 700;
  }
  .comment-actions,
  .thread-actions {
    display: flex;
    align-items: baseline;
    gap: 16px;
    flex-wrap: wrap;
    font-size: 11px;
  }
  .thread-actions {
    margin-top: 10px;
    padding-top: 8px;
    border-top: 1px solid var(--sep);
  }
  .comment-actions:empty {
    display: none;
  }
  .comment-actions details[open],
  .thread-actions details[open] {
    flex-basis: 100%;
  }
  .comment-actions summary,
  .thread-actions summary,
  .text-control {
    font: 700 10px var(--font-mono);
    color: var(--brand-strong);
    border: 0;
    padding: 0;
    background: transparent;
    cursor: pointer;
  }
  .text-control:disabled {
    color: var(--muted);
    cursor: not-allowed;
  }
  .review textarea {
    min-height: 48px;
  }
  fieldset {
    border: 0;
    padding: 0;
    margin: 0;
  }
  legend {
    margin-bottom: 12px;
    font-weight: 600;
  }
  .decision {
    display: flex;
    align-items: flex-start;
    gap: 10px;
  }
  .decision input {
    margin-top: 3px;
  }
  .decision p {
    margin: 4px 0;
    color: var(--muted);
  }
  @media (max-width: 600px) {
    .comment {
      margin: 0;
      font-size: 10px;
    }
    .comment :global(.markdown p) {
      margin: 4px 0;
      line-height: 1.4;
    }
    .review :global(.divider) {
      margin-top: 4px;
      padding-top: 0;
      border: 0;
    }
    .history {
      font-size: 9px;
    }
    .resolve {
      margin-top: 3px;
    }
    .resolve button {
      border: 0;
      padding: 2px;
      background: transparent;
      color: var(--brand-strong);
    }
    .review-form,
    .review-form fieldset {
      gap: 6px;
    }
    .review-form .decision {
      font-size: 10px;
      gap: 6px;
    }
    .decision strong,
    .decision p {
      display: inline;
    }
    .decision p::before {
      content: ' — ';
    }
    legend {
      margin-bottom: 4px;
    }
  }
</style>
