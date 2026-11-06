<!--
  An exact Prompt as authored, which is exactly what the provider receives:
  provider, model and response format, then portable system text and
  numbered turns, or the native format's body unchanged, extension fields
  included.
-->
<script lang="ts">
  import type { PromptMessagesProps } from '../cards.js';
  import { fmtYaml } from '../format.js';
  import EmptyState from './EmptyState.svelte';
  import Panel from './Panel.svelte';

  let { input, result }: PromptMessagesProps = $props();

  const spec = $derived(result.state === 'populated' ? result.data : undefined);
  const responseFormat = $derived(
    spec?.response_format == null ? undefined : typeof spec.response_format === 'string' ? spec.response_format : JSON.stringify(spec.response_format)
  );
  /** Splits text so odd parts are `{{placeholder}}` spans; display only, the server derives the variables. */
  const PLACEHOLDER = /(\{\{[^{}]*\}\})/;
  /** One content block as text; media keeps its literal source or `${media:name}` binding. */
  const block = (part: { type: string; text?: string; source?: unknown }) =>
    part.type === 'text' ? (part.text ?? '') : `[${part.type}] ${typeof part.source === 'string' ? part.source : JSON.stringify(part.source)}`;
</script>

{#if spec}
  <Panel title="Prompt" note="exactly what the provider receives">
    <p class="chips">
      <span>provider {spec.provider}</span>
      <span>model {spec.model}</span>
      {#if spec.format}<span>native {spec.format}</span>{/if}
      <span>response · {responseFormat ?? 'text'}</span>
    </p>
    {#if spec.body}
      <section aria-label="Native body">
        <h4>Native body · unchanged</h4>
        <pre>{fmtYaml(spec.body)}</pre>
      </section>
    {:else}
      {#if spec.system}
        {@render turn('System', 'System', spec.system)}
      {/if}
      {#each spec.messages ?? [] as message, index (index)}
        {@render turn(`Message ${index + 1}`, `${index + 1} · ${message.role}`, typeof message.content === 'string' ? message.content : message.content.map(block).join('\n\n'))}
      {/each}
      {#if !spec.system}<p class="none">No system message.</p>{/if}
    {/if}
    {#if spec.settings && Object.keys(spec.settings).length > 0}
      <section aria-label="Settings">
        <h4>Settings</h4>
        <pre>{fmtYaml(spec.settings)}</pre>
      </section>
    {/if}
  </Panel>
{:else if result.state === 'loading'}
  <EmptyState state="loading" title="Loading {input.cardRef.name}@{input.cardRef.version}" />
{:else if result.state === 'denied' || result.state === 'error'}
  <EmptyState state={result.state} problem={result.problem} />
{/if}

<!-- One turn as the provider receives it, its `{{variable}}` placeholders marked. -->
{#snippet turn(label: string, heading: string, text: string)}
  <section class="turn" aria-label={label}>
    <h4>{heading}</h4>
    <div class="bubble">
      {#each text.split(PLACEHOLDER) as part, index (index)}{#if index % 2}<span class="var">{part}</span>{:else}{part}{/if}{/each}
    </div>
  </section>
{/snippet}

<style>
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin: 0;
  }
  .chips span {
    border: 1px solid var(--border);
    border-radius: var(--r);
    padding: 3px 9px;
    background: var(--surface-2);
    font: 12px var(--font-mono);
    white-space: nowrap;
  }
  section {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .turn {
    display: grid;
    grid-template-columns: 84px minmax(0, 1fr);
    gap: 10px;
    align-items: start;
  }
  h4 {
    margin: 0;
    color: var(--muted);
    font: 400 11px var(--font-mono);
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }
  .turn h4 {
    padding-top: 10px;
  }
  .bubble {
    min-width: 0;
    border: 1px solid var(--border);
    border-left: 3px solid var(--declare);
    border-radius: var(--r);
    padding: 12px 14px;
    background: var(--surface-2);
    font: 15px/1.5 var(--font-sans);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .var {
    padding: 1px 5px;
    background: var(--declare-soft);
    color: color-mix(in srgb, var(--declare) 45%, var(--text));
    font-family: var(--font-mono);
  }
  .none {
    margin: 0;
    border-top: 1px dashed var(--border);
    padding-top: 10px;
    color: var(--muted);
    font: 11.5px var(--font-mono);
  }
  @media (max-width: 560px) {
    .turn {
      grid-template-columns: minmax(0, 1fr);
    }
    .turn h4 {
      padding-top: 0;
    }
  }
</style>
