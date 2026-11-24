<!--
  The page an email link opens. Opening it consumes nothing: on mount the
  page clears the link's `#proof=` fragment from the address bar, then POSTs
  the proof; without a fragment the person pastes the code. `verify` POSTs
  one proof and answers the problem to show, or null once it is accepted, so
  each purpose keeps its own endpoint and wording.
-->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import { onMount } from 'svelte';
  import Entry from '$lib/Entry.svelte';
  import CheckEmail from '$lib/CheckEmail.svelte';
  import type { WyrdProblem } from '$lib/server/problem';

  let {
    heading,
    doneHeading,
    verify,
    restart,
    done
  }: {
    heading: string;
    doneHeading: string;
    verify: (proof: string) => Promise<WyrdProblem | null>;
    restart: [href: string, label: string];
    done: Snippet;
  } = $props();

  let phase: 'checking' | 'paste' | 'refused' | 'done' = $state('checking');
  let issue: WyrdProblem | null = $state.raw(null);

  async function submit(proof: string) {
    phase = 'checking';
    issue = await verify(proof);
    phase = issue ? 'refused' : 'done';
  }

  onMount(() => {
    const proof = new URLSearchParams(location.hash.slice(1)).get('proof');
    if (!proof) {
      phase = 'paste';
      return;
    }
    history.replaceState(history.state, '', location.pathname + location.search);
    void submit(proof);
  });
</script>

<Entry heading={phase === 'done' ? doneHeading : heading} {issue}>
  {#if phase === 'checking'}
    <p class="center" role="status" aria-busy="true">Checking the link…</p>
  {:else if phase === 'paste'}
    <CheckEmail onverify={submit} again={restart[0]} againLabel={restart[1]} />
  {:else if phase === 'refused'}
    <a class="app-control primary wide" href={restart[0]}>{restart[1]}</a>
  {:else}
    {@render done()}
  {/if}
</Entry>

<style>
  .center {
    text-align: center;
  }
</style>
