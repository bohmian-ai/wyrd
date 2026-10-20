<script lang="ts">
  import type { PageProps } from './$types';
  let { data }: PageProps = $props();
</script>

<svelte:head><title>{data.tenant.name} · Wyrd</title></svelte:head>
<h1>{data.tenant.name}</h1>
<p>Signed in as <strong>{data.session.subject.name}</strong> to tenant <code>{data.tenant.key}</code>.</p>

<style>
  h1 {
    font: var(--weight-strong) 24px var(--font-sans);
    margin-bottom: 8px;
  }
  p {
    font: 13px var(--font-sans);
    color: var(--muted);
  }
  strong {
    color: var(--text);
    font-weight: var(--weight-strong);
  }
  code {
    font: 12px var(--font-mono);
  }
</style>
