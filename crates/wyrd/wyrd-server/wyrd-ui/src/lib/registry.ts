import type { Component } from 'svelte';
import Card from './components/Card.svelte';
import Button from './components/Button.svelte';
import Badge from './components/Badge.svelte';
import Table from './components/Table.svelte';
import KpiTile from './components/KpiTile.svelte';
import Spark from './components/Spark.svelte';
import Bars from './components/Bars.svelte';
import Lines from './components/Lines.svelte';
import Histo from './components/Histo.svelte';
import Trend from './components/Trend.svelte';
import Heatmap from './components/Heatmap.svelte';
import Dist from './components/Dist.svelte';
import Tree from './components/Tree.svelte';
import Dropdown from './components/Dropdown.svelte';
import CodeBlock from './components/CodeBlock.svelte';
import TraceTable from './components/TraceTable.svelte';
import Waterfall from './components/Waterfall.svelte';
import SpanPanel from './components/SpanPanel.svelte';
import EvalPanel from './components/EvalPanel.svelte';
import DriftPanel from './components/DriftPanel.svelte';
import Shell from './components/Shell.svelte';
import Sidebar from './components/Sidebar.svelte';
import Topbar from './components/Topbar.svelte';
import Hero from './components/Hero.svelte';

// The shared bridge: maps a standardized component name to its implementation.
// The workbench imports components directly; a dynamic-layout/A2UI renderer resolves
// them by name from here. Every entry must have a matching `status: "built"` contract
// in brand/components.json (enforced by registry.test.ts). Infrastructure (ModeProvider,
// Drawer) is composed by other components rather than placed by name, so it is excluded.
export const registry = {
  Card,
  Button,
  Badge,
  Table,
  KpiTile,
  Spark,
  Bars,
  Lines,
  Histo,
  Trend,
  Heatmap,
  Dist,
  Tree,
  Dropdown,
  CodeBlock,
  TraceTable,
  Waterfall,
  SpanPanel,
  EvalPanel,
  DriftPanel,
  Shell,
  Sidebar,
  Topbar,
  Hero
} satisfies Record<string, Component<any>>;

export type ComponentName = keyof typeof registry;

export function resolve(name: string): Component<any> | undefined {
  return (registry as Record<string, Component<any>>)[name];
}
