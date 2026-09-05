<script lang="ts">
  import { untrack } from 'svelte';
  import { enhance } from '$app/forms';
  import type { ChangeView, Anchor, ActionResult } from './types';
  import Markdown from './Markdown.svelte';
  let {
    view,
    csrf,
    operation = 'comment',
    threadId = '',
    commentId = '',
    expected = '',
    initial = '',
    anchor,
    label = 'Comment',
    result
  }: {
    view: ChangeView;
    csrf: string;
    operation?: string;
    threadId?: string;
    commentId?: string;
    expected?: string;
    initial?: string;
    anchor?: Anchor;
    label?: string;
    result?: ActionResult | null;
  } = $props();
  let body = $state(untrack(() => initial));
  let preview = $state(false);
  let revision = $state(untrack(() => view.change.revision));
  let expectedRevision = $state(untrack(() => expected));
  let busy = $state(false);
  let query = $derived(body.match(/@([a-zA-Z0-9_.-]*)$/)?.[1]);
  let matches = $derived(
    query === undefined
      ? []
      : view.mentions.filter((person) => person.name.startsWith(query))
  );
</script>
<form method="POST" action="?/review" class="stack composer" use:enhance={() => { busy = true; return async ({ result: response, update }) => { await update({ reset: false }); if (response.type === 'success') body = ''; busy = false; }; }}>
  <input type="hidden" name="csrf" value={csrf} /><input type="hidden" name="operation" value={operation} /><input type="hidden" name="revision" value={revision} /><input type="hidden" name="requestKey" value={`${view.requestKey}:${operation}:${threadId}`} /><input type="hidden" name="threadId" value={threadId} /><input type="hidden" name="commentId" value={commentId} /><input type="hidden" name="expected" value={expectedRevision} />{#if anchor}<input type="hidden" name="anchor" value={JSON.stringify({ ...anchor, revision })} />{/if}
  <div class="composer-field"><textarea aria-label={`${label} · GitHub Flavored Markdown`} name="body" bind:value={body} maxlength="10000" required placeholder="Write a comment… @ mentions people and teams" rows="2"></textarea><button class="control primary" disabled={busy || !view.capabilities.review}>{busy ? 'Posting…' : label}</button></div>
  {#if matches.length}<div aria-label="Mention suggestions" class="row">{#each matches as person}<button type="button" class="control" onclick={() => body = body.replace(/@[a-zA-Z0-9_.-]*$/, `@${person.name} `)}>@{person.name} — {person.team}</button>{/each}</div>{/if}
  {#if preview}<Markdown {body} />{/if}
  {#if result?.problem && result.body !== undefined}<div class="notice" role="alert"><strong>{result.problem.title}</strong><p>{result.body}</p><p class="mono">{result.problem.code}</p>{#if result.problem.status === 409}<button type="button" class="control" onclick={() => { revision = result?.currentRevision ?? revision; expectedRevision = result?.currentCommentRevision ?? expectedRevision; if (!body) body = result?.body ?? ''; }}>Reload, keep draft</button>{/if}</div>{/if}
  <div class="row"><button type="button" class="text-action" onclick={() => preview = !preview}>{preview ? 'Hide preview' : 'Preview'}</button><button type="button" class="text-action" onclick={() => body = ''}>Cancel</button></div>
</form>

<style>
  .composer {
    gap: 6px;
  }
  .composer-field {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    align-items: start;
    gap: 8px;
  }
  .composer textarea {
    min-height: 36px;
    font-size: 11px;
    padding: 6px;
  }
  .text-action {
    border: 0;
    background: transparent;
    color: var(--brand-strong);
    font: 11px var(--font-mono);
    padding: 2px;
    cursor: pointer;
  }
  @media (max-width: 600px) {
    .composer-field {
      grid-template-columns: minmax(0, 1fr) auto;
    }
    .composer textarea {
      min-height: 36px;
    }
  }
</style>
