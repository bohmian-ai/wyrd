<!-- Components used directly with server-loaded props. -->
<script lang="ts">
  import { CardHeader, Chart } from 'wyrd-ai-ui';

  let { data } = $props();
</script>

<main>
  <CardHeader {...data.header} />
  <Chart {...data.chart} />
  <a href="/view">The same panels as a view document</a>
</main>

<style>
  main {
    display: grid;
    gap: 16px;
    max-width: 960px;
    margin: 0 auto;
    padding: 16px;
  }
</style>
