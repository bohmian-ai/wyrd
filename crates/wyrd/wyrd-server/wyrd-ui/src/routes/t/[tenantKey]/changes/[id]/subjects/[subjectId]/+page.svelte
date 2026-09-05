<script lang="ts">
  import { MediaQuery } from 'svelte/reactivity';
  const narrow = new MediaQuery('(max-width: 600px)');
  let navigationOpen = $state(true);
  let commitsOpen = $state(true);
  $effect(() => {
    navigationOpen = !narrow.current;
    commitsOpen = !narrow.current;
  });
  import '$lib/features/changes/changes.css';
  import ChangeHeader from '$lib/features/changes/ChangeHeader.svelte';
  import Composer from '$lib/features/changes/Composer.svelte';
  import Table from '$lib/components/Table.svelte';
  import Panel from '$lib/components/Panel.svelte';
  import Badge from '$lib/components/Badge.svelte';
  import StateBlock from '$lib/components/StateBlock.svelte';
  let { data, form } = $props();
  let selection = $state<{ key: string; line: number; side: 'old' | 'new' } | null>(null);
  let selectionKey = $derived(
    JSON.stringify([data.change.id, data.change.revision, data.subject.id, data.file?.path])
  );
  let line = $derived(
    selection?.key === selectionKey
      ? selection.line
      : (data.file?.lines.at(-1)?.number ?? null)
  );
  let side = $derived(
    selection?.key === selectionKey
      ? selection.side
      : (data.file?.lines.at(-1)?.side ?? 'new')
  );
  let base = $derived(`/t/${data.tenant.key}/changes/${data.change.id}`);
  let fileIndex = $derived(
    data.subject.files.findIndex((file) => file.path === data.file?.path)
  );
  let subjectIndex = $derived(
    data.change.subjects.findIndex((subject) => subject.id === data.subject.id)
  );
  let threads = $derived(
    data.change.threads.filter((thread) => thread.anchor.target === data.subject.id)
  );
  function fileLink(index: number) {
    return `?revision=${data.change.revision}&file=${encodeURIComponent(data.subject.files[index].path)}`;
  }
</script>
<div class="changes source-page"><ChangeHeader view={data} {base} active="Subjects" /><div class="stack">
<details class="subject-navigation" bind:open={navigationOpen}><summary class="source-identity mono">{data.subject.base} → {data.subject.candidate} · {data.subject.repository} · PR #{data.subject.pr}</summary><div class="row between"><a href={`${base}?revision=${data.change.revision}#subjects`}>← All subjects</a><span class="mono">{data.subject.id} — subject {subjectIndex + 1} of {data.change.subjects.length}</span>{#if data.change.subjects[subjectIndex + 1]}<a href={`${base}/subjects/${data.change.subjects[subjectIndex + 1].id}?revision=${data.change.revision}`}>Next subject →</a>{/if}</div><div class="notice mono source-identity">{data.subject.base} → {data.subject.candidate} · {data.subject.repository} · {data.subject.provider} · PR #{data.subject.pr} · {data.subject.relationship}</div></details>

<div class="source-grid"><section class="files"><Panel title="Changed files"><p>File {fileIndex + 1} of {data.subject.files.length}</p><nav class="file-list" aria-label="Changed files">{#each data.subject.files as file, index}<a href={fileLink(index)} aria-current={data.file?.path === file.path ? 'page' : undefined} title={file.path}><span class="full-path">{file.path}</span><span class="short-path">{file.path.split("/").at(-1)}</span><small> +{file.additions} −{file.deletions}</small></a>{/each}</nav></Panel></section>
<div class="stack diff-content">{#if data.file}{@const file = data.file}<Panel title={`${data.file.path} — unified diff · Read-only`}>{#snippet head()}<div class="row between file-controls"><span class="mono">+{file.additions} −{file.deletions}</span><div class="row">{#if fileIndex > 0}<a class="control" href={fileLink(fileIndex-1)}>‹ Prev file</a>{/if}{#if fileIndex + 1 < data.subject.files.length}<a class="control" href={fileLink(fileIndex+1)}>Next file ›</a>{/if}</div></div>{/snippet}
<Table label="Read-only unified diff"><div class="diff">{#each data.file.lines as row}<div id={`line-${row.side}-${row.number}`} class="diff-line" class:added={row.kind === '+'} class:removed={row.kind === '-'}><button class="line-action" type="button" aria-label={`Discuss ${row.side} line ${row.number}`} onclick={() => selection = { key: selectionKey, line: row.number, side: row.side }} disabled={!data.capabilities.review}>+</button><span class="line-number">{row.number}</span><code>{row.kind} {row.text}</code>{#each threads.filter(thread => thread.anchor.file === data.file?.path && thread.anchor.line === row.number && thread.anchor.side === row.side) as thread}<a href={`${base}/review#${thread.id}`}>{thread.resolved ? '✓ Resolved thread' : '● Open thread'}</a>{/each}</div>{/each}</div></Table>
</Panel>{:else}<StateBlock state="absent" title="No source diff received" detail="The exact subject is saved; source inspection is not available yet." />{/if}
<Panel title={`Start discussion${line ? ' — line ' + line : ''}`}><p class="mono">{data.file?.path} · revision {data.change.revisionNumber}</p>{#if data.file}{#key selectionKey}<Composer view={data} csrf={data.session.csrf} label="Start discussion" anchor={{ kind: 'source', revision: data.change.revision, target: data.subject.id, file: data.file.path, line: line ?? data.file.lines.at(-1)!.number, side }} result={form} />{/key}{/if}</Panel>
{#if data.subject.url}<a href={data.subject.url} target="_blank" rel="noreferrer">View in provider ↗ — PR #{data.subject.pr}</a>{/if}
</div><section class="source-threads"><Panel title="Anchored threads">{#each threads as thread}<p class="mono">{thread.anchor.file}:{thread.anchor.line}</p><Badge tone={thread.resolved ? 'ok' : 'running'}>{thread.resolved ? 'Resolved' : 'Open'}</Badge><p><a href={`${base}/review#${thread.id}`}>Open thread →</a></p>{/each}</Panel></section><details class="source-commits" bind:open={commitsOpen}><summary>Commit history · {data.subject.commits.length}</summary><Panel title="Commits">{#each data.subject.commits as commit}<p class="mono"><strong>{commit.sha}</strong><br />{commit.title}</p>{/each}</Panel></details></div><div class="footer row"><a class="control primary" href={`${base}/review?revision=${data.change.revision}#submit-review`}>Review changes</a></div>
</div></div>
<style>
  .source-grid {
    display: grid;
    grid-template-columns: minmax(190px, 270px) minmax(0, 1fr);
    gap: 20px;
    align-items: start;
  }
  .short-path {
    display: none;
  }
  .file-list {
    display: grid;
    gap: 12px;
    font: 10px var(--font-mono);
    overflow-wrap: anywhere;
  }
  .file-list a[aria-current] {
    background: var(--brand-soft);
    padding: 6px;
    border: 2px solid var(--border);
    border-radius: var(--r);
  }
  .diff {
    width: max-content;
    min-width: 100%;
    margin-top: 16px;
  }
  .diff-line {
    display: flex;
    align-items: center;
    gap: 8px;
    width: max-content;
    min-width: 100%;
    font: 11px/2 var(--font-mono);
  }
  .diff-line code {
    white-space: pre;
  }
  .diff-line.added {
    background: color-mix(in srgb, var(--ok) 10%, var(--surface));
  }
  .diff-line.removed {
    background: color-mix(in srgb, var(--danger) 10%, var(--surface));
  }
  .line-number {
    color: var(--muted);
  }
  .line-action {
    background: var(--surface);
    color: var(--brand-strong);
    border: 2px solid var(--border);
    border-radius: var(--r);
    cursor: pointer;
  }
  .files {
    grid-column: 1;
    grid-row: 1;
  }
  .diff-content {
    grid-column: 2;
    grid-row: 1 / span 3;
  }
  .source-threads {
    grid-column: 1;
    grid-row: 2;
  }
  .source-commits {
    grid-column: 1;
    grid-row: 3;
  }
  .subject-navigation > summary,
  .source-commits > summary {
    display: none;
  }
  @media (max-width: 1000px) {
    .source-grid {
      display: flex;
      flex-direction: column;
      gap: 10px;
    }
    .source-grid > * {
      width: 100%;
      min-width: 0;
    }
    .full-path {
      display: none;
    }
    .short-path {
      display: inline;
    }
    .file-list {
      gap: 6px;
      display: flex;
      flex-wrap: wrap;
    }
  }
  @media (max-width: 600px) {
    .subject-navigation > summary,
    .source-commits > summary {
      display: block;
      font-size: 9px;
    }
    .source-identity {
      padding: 0;
      border: 0;
      line-height: 1.4;
    }
    .files :global(.wy-panel) {
      border: 0;
      box-shadow: none;
      background: transparent;
    }
    .files :global(.wy-panel-head),
    .files p {
      display: none;
    }
    .source-page .files :global(.wy-panel-body) {
      padding: 0;
    }
    .file-list {
      font-size: 9px;
      align-items: center;
      flex-wrap: nowrap;
      overflow-x: auto;
      white-space: nowrap;
    }
    .file-list a[aria-current] {
      padding: 3px;
    }
    .diff {
      margin-top: 0;
    }
    .diff-line {
      font-size: 10px;
      line-height: 2;
      gap: 5px;
    }
    .file-controls {
      font-size: 8.5px;
    }
    .file-controls .control {
      font-size: 8.5px;
      padding: 2px 4px;
    }
    .diff-content :global(.wy-panel-head) {
      flex-wrap: nowrap;
      padding: 5px 7px;
    }
    .diff-content :global(.wy-panel-head .t) {
      min-width: 0;
      flex: 1;
      white-space: nowrap;
      overflow: hidden;
      text-overflow: ellipsis;
    }
    .source-threads :global(.wy-panel-body) {
      display: grid;
      grid-template-columns: 1fr auto auto;
      gap: 4px;
      align-items: center;
    }
    .source-threads p {
      margin: 3px 0;
    }
    .source-page :global(.composer textarea) {
      height: 32px;
      min-height: 32px;
      font-size: 10px;
    }
    .source-page :global(.wy-panel-body) {
      padding: 6px 8px;
    }
  }
</style>
