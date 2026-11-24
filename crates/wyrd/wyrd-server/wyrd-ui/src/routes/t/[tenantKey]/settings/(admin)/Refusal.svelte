<!--
  A Settings section the server would not load: a refusal names who may do
  this and offers an access request; anything else is the server's safe problem.
-->
<script lang="ts">
  import type { WyrdProblem } from '$lib/server/problem';
  import AccessRequest from '$lib/AccessRequest.svelte';
  interface Props {
    problem: WyrdProblem | null;
    /** What the section lets an administrator do, in plain words. */
    action: string;
  }
  let { problem, action }: Props = $props();
</script>

{#if problem?.status === 403}
  <div class="app-problem" data-state="unauthorized">
    <strong>⊘ You can't {action.toLowerCase()} in this workspace</strong>
    <p>This is for workspace administrators. Ask one to grant what you need.</p>
    <code>{problem.code}</code>
  </div>
  <AccessRequest {action} />
{:else}
  <div class="app-problem" data-state="error">
    <strong>✕ This section is unavailable</strong>
    <p>{problem?.remediation ?? 'Please try again later.'}</p>
    <code>{problem?.code}</code>
  </div>
{/if}
