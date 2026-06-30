<script lang="ts">
  let {
    items = [
      ['Overview', 'overview', false],
      ['Register a card', 'register', false],
      ['From the CLI', 'cli', true],
      ['From Python', 'python', true],
      ['Idempotency', 'idempotency', false],
      ['Errors', 'errors', false]
    ],
    active = 'register'
  }: { items?: [string, string, boolean][]; active?: string } = $props();
</script>

<nav class="toc" aria-label="On this page">
  <div class="th">On this page</div>
  {#each items as [label, slug, sub]}
    <a href="#top" class={`${sub ? 'sub' : ''} ${slug === active ? 'on' : ''}`}>{label}</a>
  {/each}
</nav>
