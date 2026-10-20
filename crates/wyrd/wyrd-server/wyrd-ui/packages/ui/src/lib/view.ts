/**
 * View documents: validation, the `WyrdView` renderer, `mountView` for
 * non-Svelte hosts, and `remoteLoader` for hosts that load through a BFF.
 * Browser-safe: no credential and no Node SDK import.
 */
import './theme.css';
import { mount, unmount } from 'svelte';
import type { ActionIntent, ActionState, Executor, Problem } from './contract.js';
import { inputProblem, type ComponentName, type ComponentProps } from './loaders.js';
import { ViewDocumentSchema, type AbsoluteRange, type ViewDocument } from './schemas.js';
import WyrdView from './WyrdView.svelte';

export { WyrdView, ViewDocumentSchema };
export type { ViewDocument };

/** Loads one validated panel under the view's resolved range. */
export type PanelLoader = (
  component: ComponentName,
  input: unknown,
  context: { readonly range: AbsoluteRange; readonly signal: AbortSignal }
) => Promise<ComponentProps[ComponentName] | Problem>;

/** Performs one validated host-navigation intent from the panel `panelId`; resolves to its outcome. */
export type ViewActionHandler = (intent: ActionIntent, panelId: string) => Promise<ActionState | void> | ActionState | void;

/** Options of {@link mountView}. */
export interface MountViewOptions {
  readonly doc: unknown;
  readonly executor?: Executor;
  readonly load?: PanelLoader;
  readonly onaction?: ViewActionHandler;
  readonly mode?: 'light' | 'dark';
}

/** Validate a view document's structure; panel inputs are validated per panel at load. */
export function validateView(doc: unknown): ViewDocument | Problem {
  const parsed = ViewDocumentSchema.safeParse(doc);
  return parsed.success ? parsed.data : inputProblem('View', parsed.error.issues[0]);
}

/** Render a view document into `element` from any DOM host; `destroy` aborts loads and removes it. */
export function mountView(element: Element, options: MountViewOptions): { destroy(): void } {
  const view = mount(WyrdView, { target: element, props: { ...options } });
  return { destroy: () => void unmount(view) };
}

/**
 * Load panels through a host endpoint such as the BFF's `POST /t/<tenant>/ui/load`.
 * The browser sends only the panel and range; the endpoint holds the session.
 */
export function remoteLoader(url: string, fetcher: typeof fetch = fetch): PanelLoader {
  return async (component, input, { range, signal }) => {
    const response = await fetcher(url, {
      method: 'POST',
      headers: { 'content-type': 'application/json', accept: 'application/json' },
      body: JSON.stringify({ component, input, range }),
      signal
    });
    const body = (await response.json()) as ComponentProps[ComponentName] | Problem;
    if (response.ok) return body;
    return 'code' in body ? body : { code: 'WYRD_UI_LOAD_FAILED', title: 'Panel failed to load', status: response.status };
  };
}
