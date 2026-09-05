<script lang="ts">
  import '$lib/features/changes/changes.css';
  import ChangeHeader from '$lib/features/changes/ChangeHeader.svelte';
  import Composer from '$lib/features/changes/Composer.svelte';
  import Table from '$lib/components/Table.svelte';
  import Panel from '$lib/components/Panel.svelte';
  import Badge from '$lib/components/Badge.svelte';
  import StateBlock from '$lib/components/StateBlock.svelte';
  let { data, form } = $props();
  let selection = $state<{ key: string; line: number } | null>(null);
  let selectionKey = $derived(
    JSON.stringify([data.change.id, data.change.revision, data.subject.id, data.file?.path])
  );
  let line = $derived(
    selection?.key === selectionKey
      ? selection.line
      : (data.file?.lines.at(-1)?.number ?? null)
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
<div class="changes"><ChangeHeader view={data} {base} active="Subjects" /><div class="stack">
<div class="row between"><a href={`${base}?revision=${data.change.revision}#subjects`}>← All subjects</a><span class="mono">{data.subject.id} — subject {subjectIndex + 1} of {data.change.subjects.length}</span>{#if data.change.subjects[subjectIndex + 1]}<a href={`${base}/subjects/${data.change.subjects[subjectIndex + 1].id}?revision=${data.change.revision}`}>Next subject →</a>{/if}</div>
<div class="notice mono">{data.subject.base} → {data.subject.candidate} · {data.subject.repository} · {data.subject.provider} · PR #{data.subject.pr} · {data.subject.relationship}</div>
<div class="source-grid"><aside class="stack"><Panel title="Changed files"><p>File {fileIndex + 1} of {data.subject.files.length}</p><nav class="file-list" aria-label="Changed files">{#each data.subject.files as file, index}<a href={fileLink(index)} aria-current={data.file?.path === file.path ? 'page' : undefined} title={file.path}><span class="full-path">{file.path}</span><span class="short-path">{file.path.split("/").at(-1)}</span><small> +{file.additions} −{file.deletions}</small></a>{/each}</nav></Panel><Panel title="Anchored threads">{#each threads as thread}<p class="mono">{thread.anchor.file}:{thread.anchor.line}</p><Badge tone={thread.resolved ? 'ok' : 'running'}>{thread.resolved ? 'Resolved' : 'Open'}</Badge><p><a href={`${base}/review#${thread.id}`}>Open thread →</a></p>{/each}</Panel><Panel title="Commits">{#each data.subject.commits as commit}<p class="mono"><strong>{commit.sha}</strong><br />{commit.title}</p>{/each}</Panel></aside>
<div class="stack">{#if data.file}<Panel title={`${data.file.path} — unified diff · Read-only`}><div class="row between"><span class="mono">+{data.file.additions} −{data.file.deletions}</span><div class="row">{#if fileIndex > 0}<a class="control" href={fileLink(fileIndex-1)}>‹ Prev file</a>{/if}{#if fileIndex + 1 < data.subject.files.length}<a class="control" href={fileLink(fileIndex+1)}>Next file ›</a>{/if}</div></div>
<Table label="Read-only unified diff"><div class="diff">{#each data.file.lines as row}<div id={`line-${row.number}`} class="diff-line" class:added={row.kind === '+'} class:removed={row.kind === '-'}><button class="line-action" type="button" aria-label={`Discuss line ${row.number}`} onclick={() => selection = { key: selectionKey, line: row.number }} disabled={!data.capabilities.review}>+</button><span class="line-number">{row.number}</span><code>{row.kind} {row.text}</code>{#each threads.filter(thread => thread.anchor.file === data.file?.path && thread.anchor.line === row.number) as thread}<a href={`${base}/review#${thread.id}`}>{thread.resolved ? '✓ Resolved thread' : '● Open thread'}</a>{/each}</div>{/each}</div></Table>
</Panel>{:else}<StateBlock state="absent" title="No source diff received" detail="The exact subject is saved; source inspection is not available yet." />{/if}
<Panel title={`Start discussion${line ? ' — line ' + line : ''}`}><p class="mono">{data.file?.path} · revision {data.change.revisionNumber}</p>{#if data.file}{#key selectionKey}<Composer view={data} csrf={data.session.csrf} label="Start discussion" anchor={{ kind: 'source', revision: data.change.revision, target: data.subject.id, file: data.file.path, line: line ?? data.file.lines.at(-1)!.number }} result={form} />{/key}{/if}</Panel>
{#if data.subject.url}<a href={data.subject.url} target="_blank" rel="noreferrer">View in provider ↗ — PR #{data.subject.pr}</a>{/if}
</div></div><div class="footer row"><a class="control primary" href={`${base}/review?revision=${data.change.revision}#submit-review`}>Review changes</a></div>
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
  @media (max-width: 1000px) {
    .source-grid > aside {
      display: contents;
    }
    .source-grid > aside :global(.wy-panel:first-child) {
      order: 0;
    }
    .source-grid > div {
      order: 1;
    }
    .source-grid > aside :global(.wy-panel:nth-child(2)) {
      order: 2;
    }
    .source-grid > aside :global(.wy-panel:nth-child(3)) {
      order: 3;
    }

    .source-grid {
      grid-template-columns: minmax(0, 1fr);
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
</style>
