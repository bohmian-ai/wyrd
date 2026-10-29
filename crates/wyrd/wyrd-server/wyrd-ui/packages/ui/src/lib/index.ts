/**
 * Wyrd UI components. Every data component renders from typed props alone;
 * loaders (`wyrd-ai-ui/loaders`) produce those props through an executor.
 */
import './theme.css';

export { default as Chart } from './components/Chart.svelte';
export { default as CardHeader } from './components/CardHeader.svelte';
export { default as StatStrip } from './components/StatStrip.svelte';
export { default as FacetRail } from './components/FacetRail.svelte';
export { default as RecordTable } from './components/RecordTable.svelte';
export { default as TraceWaterfall } from './components/TraceWaterfall.svelte';
export { default as RecordDetail } from './components/RecordDetail.svelte';
export { default as EmptyState } from './components/EmptyState.svelte';
export { default as CardRefChip } from './components/CardRefChip.svelte';
export { default as VerdictPill } from './components/VerdictPill.svelte';
export { default as StatusPill, type Tone } from './components/StatusPill.svelte';
export { default as Sparkline } from './components/Sparkline.svelte';
export { default as DurationBar } from './components/DurationBar.svelte';
export { default as Drawer } from './components/Drawer.svelte';
export * from './format.js';
export type * from './contract.js';
export type {
  CardHeaderProps,
  ChartProps,
  ComponentProps,
  FacetRailProps,
  RecordDetailProps,
  RecordTableProps,
  StatStripProps,
  TraceWaterfallProps
} from './loaders.js';
