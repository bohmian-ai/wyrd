<!-- A refusal as a stable problem: title, guidance (retry time or remediation) and the catalog code. -->
<script>
  import { guidance } from './data.js';
  let { issue } = $props();
</script>

<div role="alert">
  <div class="app-problem" id="problem" data-testid="problem" data-state={issue.status >= 500 ? 'error' : 'unauthorized'}>
    <strong>⊘ {issue.title}</strong>
    <p>{guidance(issue)}</p>
    {#if issue.code}<code>{issue.code}</code>{/if}
  </div>
</div>
