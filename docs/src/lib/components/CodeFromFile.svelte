<script lang="ts">
  import { highlight } from '$lib/shiki';
  import CodeBlock from '$lib/mdsvex/CodeBlock.svelte';

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
      displayTitle: title ?? file.split('/').pop() ?? file,
      html: highlight(code, lang)
    };
  });
</script>

<CodeBlock html={resolved.html} {lang} title={resolved.displayTitle} />
