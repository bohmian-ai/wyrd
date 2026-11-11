<!--
  An exact Verifier as declared: its purpose, the graph of inputs, steps and
  requirements, and the authored declaration of whichever node is selected.
  Definition only: no execution state, and nothing here edits the Verifier.
-->
<script lang="ts">
  import type { VerifierDefinitionData, VerifierDefinitionProps } from '../verifier.js';
  import { fmtYaml } from '../format.js';
  import EmptyState from './EmptyState.svelte';
  import Panel from './Panel.svelte';
  import VerifierGraph from './VerifierGraph.svelte';

  let { input, result }: VerifierDefinitionProps = $props();

  let selected = $state('');
  const WORDS = { input: 'Input', step: 'Step', req: 'Requirement' } as const;

  /** The authored block a node key names, exactly as the spec carries it. */
  function declaration({ spec }: VerifierDefinitionData, key: string) {
    const [type, id] = key.split(/:(.*)/s) as [keyof typeof WORDS, string];
    const steps = spec.steps ?? [];
    const owner = steps.find((step) => step.requirements?.some((each) => each.id === id));
    const block =
      type === 'input'
        ? spec.inputs?.[id]
        : type === 'step'
          ? steps.find((step) => step.id === id)
          : [...(spec.requirements ?? []), ...(owner?.requirements ?? [])].find((each) => each.id === id);
    if (block === undefined) return undefined;
    const where = type === 'req' ? (owner ? `judged after step ${owner.id}` : 'judged after every step') : undefined;
    return { title: `${WORDS[type]} ${id}`, where, yaml: fmtYaml(type === 'input' ? { [id]: block } : block) };
  }
  const shown = $derived(result.state === 'populated' && selected ? declaration(result.data, selected) : undefined);
</script>

{#if result.state === 'populated'}
  <div class="definition">
    {#if result.data.spec.description}<p class="description">{result.data.spec.description}</p>{/if}
    {#key result.data.graph}
      <VerifierGraph graph={result.data.graph} bind:selected />
    {/key}
    <Panel title={shown?.title ?? 'Declaration'} note={shown?.where}>
      {#if shown}
        <pre>{shown.yaml}</pre>
      {:else}
        <p class="hint">Select an input, step or requirement in the graph to read its declaration.</p>
      {/if}
    </Panel>
  </div>
{:else if result.state === 'loading'}
  <EmptyState state="loading" title="Loading the definition of {input.cardRef.name}@{input.cardRef.version}" />
{:else if result.state !== 'empty'}
  <EmptyState state={result.state} problem={result.problem} />
{/if}

<style>
  .definition {
    display: flex;
    flex-direction: column;
    gap: 16px;
    min-width: 0;
  }
  .description {
    margin: 0;
    color: var(--text);
    font: 14px/1.5 var(--font-sans);
    overflow-wrap: anywhere;
  }
  .hint {
    margin: 0;
    color: var(--muted);
    font: 13px var(--font-sans);
  }
</style>
