<script module lang="ts">
  // Per-instance id source. Mermaid needs a unique, CSS-valid id per render and
  // rejects ids that collide within a page, so we hand out a monotonic counter.
  let nextId = 0;
</script>

<script lang="ts">
  import { onMount } from 'svelte';

  // Themed Mermaid wrapper. Mermaid ships its own palette; we override it with
  // the site design tokens (wyrd-tokens.css) read live off :root, so diagrams
  // match the brand and flip with the light/dark toggle instead of rendering in
  // default mermaid purple. Mermaid is import()-ed inside onMount because it
  // touches the DOM and must never run during SSR.
  let { chart, title }: { chart: string; title?: string } = $props();

  const id = `wyrd-mermaid-${nextId++}`;

  let svg = $state('');
  let failed = $state(false);

  function readTokens() {
    const cs = getComputedStyle(document.documentElement);
    const token = (name: string) => cs.getPropertyValue(name).trim();
    return {
      surface: token('--surface'),
      surface2: token('--surface-2'),
      border: token('--border'),
      text: token('--text'),
      muted: token('--muted'),
      mono: token('--font-mono') || 'monospace'
    };
  }

  async function render() {
    const mermaid = (await import('mermaid')).default;
    const t = readTokens();
    mermaid.initialize({
      startOnLoad: false,
      theme: 'base',
      securityLevel: 'strict',
      fontFamily: t.mono,
      themeVariables: {
        background: t.surface,
        primaryColor: t.surface2,
        primaryBorderColor: t.border,
        primaryTextColor: t.text,
        secondaryColor: t.surface,
        tertiaryColor: t.surface,
        mainBkg: t.surface2,
        nodeBorder: t.border,
        lineColor: t.muted,
        textColor: t.text,
        clusterBkg: t.surface,
        clusterBorder: t.border,
        edgeLabelBackground: t.surface,
        fontSize: '13px'
      }
    });
    try {
      const { svg: out } = await mermaid.render(`${id}-svg`, chart);
      svg = out;
      failed = false;
    } catch {
      // A malformed chart should degrade to the readable source, not a blank box.
      failed = true;
    }
  }

  onMount(() => {
    render();
    // Re-render on theme flip: ThemeToggle writes documentElement.dataset.theme.
    const observer = new MutationObserver((records) => {
      if (records.some((r) => r.attributeName === 'data-theme')) render();
    });
    observer.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ['data-theme']
    });
    return () => observer.disconnect();
  });
</script>

<figure class="mmd">
  {#if failed}
    <pre class="mmd-src">{chart}</pre>
  {:else}
    <!-- eslint-disable-next-line svelte/no-at-html-tags -->
    <div class="mmd-canvas">{@html svg}</div>
  {/if}
  {#if title}<figcaption>{title}</figcaption>{/if}
</figure>

<style>
  .mmd {
    margin: 1.5rem 0;
    padding: 1rem;
    background: var(--surface);
    border: 2px solid var(--border);
    border-radius: var(--r);
    box-shadow: 3px 3px 0 0 var(--shadow);
    overflow-x: auto;
  }
  .mmd-canvas {
    display: flex;
    justify-content: center;
  }
  .mmd-canvas :global(svg) {
    max-width: 100%;
    height: auto;
  }
  .mmd-src {
    font-family: var(--font-mono);
    font-size: 0.8rem;
    color: var(--muted);
    white-space: pre-wrap;
    margin: 0;
  }
  figcaption {
    margin-top: 0.75rem;
    font-family: var(--font-mono);
    font-size: 0.72rem;
    letter-spacing: 0.04em;
    color: var(--muted);
    text-align: center;
  }
</style>
