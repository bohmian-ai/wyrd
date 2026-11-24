<script lang="ts">
  import Proof from '$lib/Proof.svelte';
  import { verifySignup } from '$lib/onboarding';
</script>

<svelte:head><title>Confirm your email · Wyrd</title></svelte:head>
<Proof heading="Confirm your email" doneHeading="Email confirmed" verify={verifySignup} restart={['/signup', 'Start again']}>
  {#snippet done()}
    <p class="done center" role="status"><span class="ok">✓</span> This browser can now create your workspace for the next five minutes.</p>
    <a class="app-control primary wide" href="/signup/workspace">Name your workspace</a>
  {/snippet}
</Proof>

<style>
  .center {
    text-align: center;
  }
</style>
