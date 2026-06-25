import type { Component } from 'svelte';
import Card from './components/Card.svelte';
import Button from './components/Button.svelte';
import Badge from './components/Badge.svelte';
import Table from './components/Table.svelte';
import KpiTile from './components/KpiTile.svelte';

// The shared bridge: maps a standardized component name to its implementation.
// The workbench imports components directly; a dynamic-layout/A2UI renderer resolves
// them by name from here. Every entry must have a matching `status: "built"` contract
// in brand/components.json (enforced by registry.test.ts). ModeProvider is infrastructure,
// not a placeable component, so it is intentionally excluded.
export const registry = {
  Card,
  Button,
  Badge,
  Table,
  KpiTile
} satisfies Record<string, Component<any>>;

export type ComponentName = keyof typeof registry;

export function resolve(name: string): Component<any> | undefined {
  return (registry as Record<string, Component<any>>)[name];
}
