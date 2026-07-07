// Barrel of every component an `.svx`/`.md` page can use without a per-page
// import block. The `injectMdsvexComponents` preprocessor in svelte.config.js
// imports exactly the subset each page references (including the highlighter's
// auto-injected <CodeBlock>) as bare identifiers, so content authors just drop
// the tag in. mdsvex 0.12.7's own `Components.*` layout rewrite only fires on
// parsed hast elements, never on raw/injected HTML, so it cannot be relied on.
export { default as CodeBlock } from './CodeBlock.svelte';
export { default as Aside } from '$lib/components/Aside.svelte';
export { default as LangTabs } from '$lib/components/LangTabs.svelte';
export { default as LangTab } from '$lib/components/LangTab.svelte';
export { default as CodeFromFile } from '$lib/components/CodeFromFile.svelte';
export { default as CardTileGrid } from '$lib/components/CardTileGrid.svelte';
export { default as CardTile } from '$lib/components/CardTile.svelte';
export { default as DataTable } from '$lib/components/DataTable.svelte';
export { default as CardSummary } from '$lib/components/CardSummary.svelte';
export { default as Pagination } from '$lib/components/Pagination.svelte';
export { default as Toc } from '$lib/components/Toc.svelte';
export { default as Tabs } from '$lib/components/Tabs.svelte';
export { default as Tab } from '$lib/components/Tab.svelte';
export { default as Steps } from '$lib/components/Steps.svelte';
export { default as WyrdFlowDiagram } from '$lib/components/WyrdFlowDiagram.svelte';
export { default as WyrdSequenceDiagram } from '$lib/components/WyrdSequenceDiagram.svelte';
export { default as WyrdServerGraph } from '$lib/components/WyrdServerGraph.svelte';
export { default as WyrdShutdownGraph } from '$lib/components/WyrdShutdownGraph.svelte';
