<script lang="ts">
  import { highlight } from '$lib/shiki';

  // Embeds a source file from the repo (crates / examples / python) into the
  // docs, highlighted with the same Shiki themes as Markdown fences. The `file`
  // prop is a repo-relative path; any number of leading `../` or `/` segments
  // are tolerated, so the legacy Astro call sites
  // (`../../../examples/python/foo.py`) and clean Phase D paths
  // (`examples/python/foo.py`) both resolve.
  let {
    file,
    lang,
    title,
    range
  }: { file: string; lang: string; title?: string; range?: string } = $props();

  // Eager raw glob keyed by repo path. `/` is the Vite root (docs/); `/../`
  // climbs to the repo root so examples/ and python/ are reachable.
  const files = import.meta.glob('/../{crates,examples,python}/**/*.{rs,py,yaml,toml}', {
    eager: true,
    import: 'default',
    query: '?raw'
  }) as Record<string, string>;

  // Reduce any path (glob key or author prop) to its repo-relative tail starting
  // at the first top-level dir, so the two sides match regardless of `../` depth.
  function repoTail(path: string): string {
    const m = path.match(/(?:crates|examples|python)\/.*$/);
    return m ? m[0] : path;
  }

  const byTail = new Map(Object.entries(files).map(([k, v]) => [repoTail(k), v]));

  function sliceRange(src: string, r: string): string {
    const [a, b] = r.split('-');
    const start = Number.parseInt(a, 10);
    const end = Number.parseInt(b ?? a, 10);
    if (!Number.isFinite(start) || !Number.isFinite(end) || start < 1 || end < start) {
      throw new Error(`CodeFromFile: invalid range "${r}"`);
    }
    return src.split('\n').slice(start - 1, end).join('\n');
  }

  const resolved = $derived.by(() => {
    const raw = byTail.get(repoTail(file));
    if (raw === undefined) {
      throw new Error(`CodeFromFile: file not found: ${file}`);
    }
    const code = range ? sliceRange(raw, range) : raw;
    return {
      code,
      displayTitle: title ?? file.split('/').pop() ?? file,
      html: highlight(code, lang)
    };
  });

  let copied = $state(false);
  async function copy(): Promise<void> {
    try {
      await navigator.clipboard.writeText(resolved.code);
      copied = true;
      setTimeout(() => (copied = false), 1500);
    } catch {
      /* clipboard unavailable */
    }
  }
</script>

<figure class="cff">
  <figcaption class="cff-bar">
    <span class="cff-title">{resolved.displayTitle}</span>
    <span class="cff-lang">{lang}</span>
    <button class="cff-copy" type="button" onclick={copy}>{copied ? 'copied' : 'copy'}</button>
  </figcaption>
  <div class="cff-body">{@html resolved.html}</div>
</figure>

<style>
  .cff {
    margin: 1.5rem 0;
    border: 2px solid var(--border);
    border-radius: var(--r);
    box-shadow: 3px 3px 0 0 var(--shadow);
    background: var(--surface);
    overflow: hidden;
  }
  .cff-bar {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 7px 11px;
    border-bottom: 2px solid var(--border);
    background: var(--surface);
    font-family: var(--font-mono);
  }
  .cff-title {
    font-size: 0.74rem;
    font-weight: 700;
    color: var(--text);
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .cff-lang {
    margin-left: auto;
    font-size: 0.6rem;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .cff-copy {
    font-family: var(--font-mono);
    font-size: 0.6rem;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--muted);
    background: var(--surface-2);
    border: 2px solid var(--border);
    border-radius: 4px;
    padding: 2px 7px;
    cursor: pointer;
  }
  .cff-copy:hover {
    color: var(--text);
  }
  .cff-body :global(pre.shiki) {
    margin: 0;
    padding: 12px 14px;
    overflow-x: auto;
    font-family: var(--font-mono);
    font-size: 0.82rem;
    line-height: 1.55;
  }
</style>
