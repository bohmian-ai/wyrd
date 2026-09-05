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
  import Markdown from './Markdown.svelte';
  import type { ChangeView, ActionResult, Anchor, Thread } from './types';
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
  function anchorLink(anchor: Anchor): string {
    if (anchor.kind === 'source' || anchor.kind === 'subject')
      return `${base}/subjects/${anchor.target}?revision=${anchor.revision}${anchor.file ? `&file=${encodeURIComponent(anchor.file)}` : ''}${anchor.line ? `#line-${anchor.side}-${anchor.line}` : ''}`;
    if (['claim', 'evidence', 'result'].includes(anchor.kind))
      return `${base}/verification?revision=${anchor.revision}#${anchor.kind === 'result' ? 'result-' : ''}${anchor.target}`;
    return `${base}?revision=${anchor.revision}`;
  }
</script>
{#snippet renderThread(thread: Thread)}<section id={thread.id}><Panel title={`Thread · ${thread.anchor.kind} · ${thread.anchor.file ?? thread.anchor.target}${thread.anchor.line ? ':' + thread.anchor.line : ''}`}>{#snippet head()}<a href={anchorLink(thread.anchor)}>Anchor →</a><Badge tone={thread.resolved ? 'ok' : 'running'}>{thread.resolved ? 'Resolved' : 'Open'}</Badge>{/snippet}
    {#each thread.comments as comment}{@const current = comment.revisions.at(-1)!}<article class="comment"><div class="row between"><strong>{view.mentions.find(person => person.id === comment.author)?.name ?? comment.author}</strong><time class="mono muted" datetime={current.at} title={current.at}>{current.at.slice(11,16)}Z</time></div><Markdown body={current.body} />
      <details class="history"><summary>Edit history · {comment.revisions.length} {comment.revisions.length === 1 ? 'revision' : 'revisions'}</summary>{#each comment.revisions as revision}<div class="notice"><p class="mono">{revision.editor} · <time datetime={revision.at}>{revision.at}</time></p><Markdown body={revision.body} /></div>{/each}</details>
      {#if comment.author === actor && view.capabilities.review}<details class="divider" open={result?.commentId === comment.id && !!result?.problem}><summary>Edit comment</summary><Composer {view} {csrf} operation="edit" threadId={thread.id} commentId={comment.id} expected={current.id} initial={current.body} label="Save edit" result={result?.commentId === comment.id ? result : null} /></details>{/if}
    </article>{/each}
    <details class="divider" open={thread === change.threads[0]}><summary>Reply · @mention</summary><Composer {view} {csrf} operation="reply" threadId={thread.id} commentId={thread.comments[0]?.id} label="Reply" result={result?.threadId === thread.id && !result?.commentId ? result : null} /></details>
    <form class="resolve" method="POST" action="?/review" use:enhance><input type="hidden" name="csrf" value={csrf} /><input type="hidden" name="revision" value={change.revision} /><input type="hidden" name="requestKey" value={`${view.requestKey}:${thread.id}`} /><input type="hidden" name="threadId" value={thread.id} /><input type="hidden" name="operation" value={thread.resolved ? 'reopen' : 'resolve'} /><button class="control" disabled={!view.capabilities.review}>{thread.resolved ? 'Reopen' : 'Resolve thread'}</button></form>
    {#if thread.transitions.length}<details class="divider"><summary>Resolution history</summary>{#each thread.transitions as transition}<p class="mono">{transition.resolved ? 'Resolved' : 'Reopened'} by {transition.actor} · {transition.at}</p>{/each}</details>{/if}
  </Panel></section>{/snippet}
<div class="columns review"><div class="stack">
  <Panel title={`Comment on this Change · revision ${change.revisionNumber}`}><Composer {view} {csrf} result={result?.threadId ? null : result} /></Panel>
  {#each change.threads.slice(0, narrow.current ? 2 : undefined) as thread (thread.id)}{@render renderThread(thread)}{/each}
</div><aside class="stack"><section id="submit-review" class="raised"><Panel title={`Submit review — revision ${change.revisionNumber}`} variant="raised"><form method="POST" action="?/review" class="stack review-form" use:enhance><input type="hidden" name="csrf" value={csrf} /><input type="hidden" name="revision" value={change.revision} /><input type="hidden" name="requestKey" value={`${view.requestKey}:decision`} /><input type="hidden" name="operation" value="decision" /><details bind:open={summaryOpen}><summary>Optional summary</summary><label>Review summary<textarea name="body" maxlength="10000"></textarea></label></details><fieldset class="stack"><legend>Review decision</legend>{#each [['comment','Comment','Feedback without a decision'],['approve','Approve','Records approval — never verifies or merges'],['request-changes','Request changes','Blocks until a new revision addresses feedback']] as [value,title,detail]}<label class="decision"><input type="radio" name="decision" {value} checked={value === 'comment'} /><span><strong>{title}</strong><p>{detail}</p></span></label>{/each}</fieldset><button class="control primary" disabled={!view.capabilities.review}>Submit review</button><small>Reviews exactly revision {change.revisionNumber}. A new revision requires re-review.</small></form></Panel></section>
  {#if result?.message}<p role="status">{result.message}</p>{/if}
  {#if result?.problem}<div role="alert" class="notice"><p>{result.problem.title}</p><p class="mono">{result.problem.code}</p><p>Your draft is preserved in its composer.</p></div>{/if}
  {#if narrow.current && change.threads.length > 2}<details class="more-threads"><summary>More threads · {change.threads.length - 2}</summary><div class="stack">{#each change.threads.slice(2) as thread (thread.id)}{@render renderThread(thread)}{/each}</div></details>{/if}
  <Panel title="Participants">{#each view.mentions.filter(person => person.kind === 'user') as person}<p>@{person.name} · {person.team}</p>{/each}</Panel>
</aside></div>
<style>
  .review-form {
    gap: 8px;
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
  .history {
    display: inline-block;
    margin-right: 12px;
    font-size: 11px;
  }
  .resolve {
    margin-top: 8px;
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
