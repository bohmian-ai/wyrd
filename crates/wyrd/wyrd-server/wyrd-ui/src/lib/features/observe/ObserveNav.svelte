<script lang="ts">
  // Observe local navigation. Every link carries the shared URL scope so
  // moving between signals preserves service and range; the current signal is
  // marked for assistive tech and the visual state together.
  import { scopeQuery, type Scope } from './core/filter-state';

  let {
    base,
    current,
    scope
  }: {
    /** Tenant-rooted observe path, e.g. `/t/acme/observe`. */
    base: string;
    /** The active entry label. */
    current: string;
    scope: Scope;
  } = $props();

  const entries = [
    ['Overview', ''],
    ['Logs', '/logs'],
    ['Metrics', '/metrics'],
    ['Traces', '/traces'],
    ['GenAI', '/genai'],
    ['Dashboards', '/dashboards'],
    ['Evaluations', '/evaluations'],
    ['Drift', '/drift']
  ];
</script>

<nav class="nav" aria-label="Observe signals">
  {#each entries as [label, suffix] (label)}
    <a
      href={`${base}${suffix}${scopeQuery(scope)}`}
      aria-current={current === label ? 'page' : undefined}>{label}</a
    >
  {/each}
</nav>
