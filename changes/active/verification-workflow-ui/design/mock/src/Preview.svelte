<script>
  import { micromark } from 'micromark';
  import { bytes, sizeOf } from './data.js';

  let { out, producer, live } = $props();
  // Source shows the exact stored text, escaped; Readable renders it safely.
  let source = $state(false);
  const raw = $derived(typeof out.content === 'string' ? out.content : JSON.stringify(out.content, null, 2));
  const renders = $derived(out.kind === 'directory' || out.name.endsWith('.md'));

  const esc = (s) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
  // Escapes first, then colours tokens; content can never become markup.
  const json = (v) => esc(JSON.stringify(v, null, 2)).replace(/("(?:\\.|[^"\\])*")(\s*:)?|\b(true|false|null)\b|(-?\d+(?:\.\d+)?)/g,
    (m, str, colon, kw, num) => str ? (colon ? `<span class="k">${str}</span>${colon}` : `<span class="s">${str}</span>`) : kw ? `<span class="n">${kw}</span>` : `<span class="n">${num}</span>`);
</script>

<div class="preview" data-testid="preview">
  <div class="preview-head">
    <span class="mono">{out.name}{out.kind === 'directory' ? '/' : ''}</span>
    <span class="tag">{out.kind}</span>
    <span class="small muted mono">{bytes(sizeOf(out))}</span>
    <span class="small muted mono">from {producer}</span>
    {#if renders}
      <span class="segs right" role="group" aria-label="Preview mode">
        <button class="seg" aria-pressed={!source} data-testid="readable" onclick={() => (source = false)}>Readable</button>
        <button class="seg" aria-pressed={source} data-testid="source" onclick={() => (source = true)}>Source</button>
      </span>
    {/if}
  </div>
  {#if live}
    <p class="note box small" style="margin: 10px 12px 0">Published by an invocation that is still running. This output stays pinned to its producer; more outputs may arrive.</p>
  {/if}
  <div class="preview-body">
    {#if source && renders}
      <pre class="code" data-testid="source-text">{raw}</pre>
    {:else if out.kind === 'json'}
      <pre class="code">{@html json(out.content)}</pre>
    {:else if out.kind === 'directory'}
      <div class="outs-wrap">
        <table class="outs manifest">
          <thead><tr><th>Path</th><th>Size</th><th>sha256</th></tr></thead>
          <tbody>
            {#each out.content.files as f (f.path)}
              <tr><td><span>{f.path}</span></td><td><span>{bytes(f.size_bytes)}</span></td><td><span class="muted">{f.sha256.slice(0, 16)}…</span></td></tr>
            {/each}
          </tbody>
        </table>
      </div>
    {:else if out.name.endsWith('.md')}
      <!-- micromark escapes raw HTML unless allowDangerousHtml is set; it is not. -->
      <div class="md">{@html micromark(out.content)}</div>
    {:else}
      <pre class="code">{out.content}</pre>
    {/if}
  </div>
</div>
