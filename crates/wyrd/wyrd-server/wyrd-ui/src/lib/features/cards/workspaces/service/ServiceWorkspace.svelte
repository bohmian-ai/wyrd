<script lang="ts">
  // TASK-007 — the Service Card operational workspace host: the exact
  // Overview · Composition · Definition local navigation (REQ-123) plus the
  // URL-backed version and range scope every view shares (REQ-125). All
  // state lives in query parameters; the server projects every value.
  import './service.css';
  import { page } from '$app/state';
  import { RANGES } from '$lib/features/observe/core/filter-state';
  import type { CardDetail, ServicePresentation } from '../../core/types';
  import { SERVICE_VIEWS, readServiceScope, serviceHref } from './service-state';
  import ServiceOverview from './ServiceOverview.svelte';
  import ServiceComposition from './ServiceComposition.svelte';
  import ServiceDefinition from './ServiceDefinition.svelte';

  let { detail, base }: { detail: CardDetail; base: string } = $props();
  const spec = $derived(detail.presentation as ServicePresentation);
  const scope = $derived(readServiceScope(page.url.searchParams));
  const path = $derived(page.url.pathname);
  /** Titlecase view label for the subnav. */
  function label(view: string): string {
    return view[0].toUpperCase() + view.slice(1);
  }
</script>

<div class="svc">
  <nav class="subnav" aria-label="Service views">
    {#each SERVICE_VIEWS as view (view)}
      <a
        class="view"
        href={serviceHref(path, detail.version, scope, { view, sel: '' })}
        aria-current={scope.view === view ? 'true' : undefined}
        data-sveltekit-noscroll>{label(view)}</a
      >
    {/each}
    <span class="scopebar">
      <span>version</span>
      {#each detail.versions as row (row.version)}
        <a
          href={serviceHref(path, row.version, scope, {})}
          aria-current={row.version === detail.version ? 'true' : undefined}
          data-sveltekit-noscroll
          >{row.version}{row.version === detail.currentVersion
            ? ' (current)'
            : row.version === detail.version
              ? ' (historical)'
              : ''}</a
        >
      {/each}
      {#if scope.view === 'overview'}
        <span>· range</span>
        {#each RANGES as range (range)}
          <a
            href={serviceHref(path, detail.version, scope, { range })}
            aria-current={scope.range === range ? 'true' : undefined}
            data-sveltekit-noscroll>{range}</a
          >
        {/each}
      {/if}
    </span>
  </nav>
  {#if scope.view === 'composition'}
    <ServiceComposition {spec} {detail} {base} {scope} {path} />
  {:else if scope.view === 'definition'}
    <ServiceDefinition {spec} {detail} {base} {scope} {path} />
  {:else}
    <ServiceOverview {spec} {detail} {base} {scope} {path} />
  {/if}
</div>
