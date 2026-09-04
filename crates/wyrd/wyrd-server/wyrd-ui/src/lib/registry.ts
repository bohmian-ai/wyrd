import type { Component } from 'svelte';
import Badge from './components/Badge.svelte';
import Button from './components/Button.svelte';
import Chip from './components/Chip.svelte';
import CodeBlock from './components/CodeBlock.svelte';
import Disclosure from './components/Disclosure.svelte';
import KpiTile from './components/KpiTile.svelte';
import Panel from './components/Panel.svelte';
import Select from './components/Select.svelte';
import StateBlock from './components/StateBlock.svelte';
import Table from './components/Table.svelte';
import Bars from './components/charts/Bars.svelte';
import ChartPanel from './components/charts/ChartPanel.svelte';
import Line from './components/charts/Line.svelte';
import Spark from './components/charts/Spark.svelte';

// The view component catalog: the components a future composed view is allowed to place
// by name. Membership is deliberate, not incidental — every entry takes typed, JSON-safe,
// semantic props and owns no credential, tenant, route or data access.
//
// Application chrome (Shell, Sidebar, Topbar, ModeProvider) is intentionally absent. It
// is trusted application code: it resolves tenant identity, session state and navigation,
// and an authored view must never be able to replace or impersonate it.
export const registry = {
  Badge,
  Button,
  Chip,
  CodeBlock,
  Disclosure,
  KpiTile,
  Panel,
  Select,
  StateBlock,
  Table,
  Bars,
  ChartPanel,
  Line,
  Spark
} satisfies Record<string, Component<any>>;

export type ComponentName = keyof typeof registry;

export function resolve(name: string): Component<any> | undefined {
  return (registry as Record<string, Component<any>>)[name];
}
