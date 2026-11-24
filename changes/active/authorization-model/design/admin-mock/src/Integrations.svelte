<script>
  import Pill from './Pill.svelte';
</script>
<header class="pagehead"><div><h1>Integrations</h1><p>Services gtest reads from.</p></div></header>
<section class="card">
  <div class="row">
    <div class="label"><b>GitHub</b><span>Read repositories and report Verifier results on pull requests.</span></div>
    <div class="ctl"><Pill tone="off">Not connected</Pill><button class="app-control primary">Connect GitHub</button></div>
  </div>
</section>
