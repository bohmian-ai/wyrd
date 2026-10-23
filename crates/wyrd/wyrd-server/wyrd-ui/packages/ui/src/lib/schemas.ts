/**
 * The catalog components' input and action schemas, declared once.
 *
 * `scripts/catalog.ts` emits these as `catalog.json` with `z.toJSONSchema`;
 * view validation, the BFF load endpoint, and hosts parse with the same
 * values, so only these schemas define what a document or intent may carry.
 */
import { z } from 'zod';
import type { CardKind, CardRef } from 'wyrd-ai';

/** Every Card kind the registry serves, checked against the generated union both ways. */
const CARD_KINDS = [
  'Data',
  'Model',
  'Experiment',
  'Prompt',
  'Agent',
  'Verifier',
  'Service',
  'Policy',
  'Mcp',
  'Audit',
  'Artifact',
  'Trigger',
  'Operator',
  'Source'
] as const satisfies readonly CardKind[];
const everyKind: CardKind extends (typeof CARD_KINDS)[number] ? true : never = true;
void everyKind;

/** An exact Card reference: a full version pin, never a range or "latest". */
export const CardRefSchema = z
  .object({
    kind: z.enum(CARD_KINDS),
    name: z.string().min(1),
    version: z
      .string()
      .regex(/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/, 'an exact version such as 1.4.2'),
    uid: z.string().min(1).nullish()
  })
  .strict() satisfies z.ZodType<CardRef>;

/** An RFC 3339 instant with an explicit offset. */
const instant = z.iso.datetime({ offset: true });

/** An absolute time range; `from` precedes `to`. */
export const AbsoluteRangeSchema = z
  .object({ from: instant, to: instant })
  .strict()
  .refine((range) => Date.parse(range.from) < Date.parse(range.to), {
    message: 'from must precede to',
    path: ['to']
  });

const UNIT_MS = { m: 60_000, h: 3_600_000, d: 86_400_000 } as const;

/** The milliseconds a relative range's `last` spans. */
function span(last: string): number {
  return Number(last.slice(0, -1)) * UNIT_MS[last.slice(-1) as keyof typeof UNIT_MS];
}

/**
 * The bounds `last` reaches back to from now, or `undefined` when they are not
 * an `AbsoluteRangeSchema` range: a start before year 0000, or one no `Date`
 * can hold. A later clock only moves the start forward, so a span admitted now
 * still resolves when the view loads.
 */
function bounds(last: string): AbsoluteRange | undefined {
  const now = Date.now();
  const from = new Date(now - span(last));
  if (Number.isNaN(from.getTime())) return undefined;
  const range = { from: from.toISOString(), to: new Date(now).toISOString() };
  return AbsoluteRangeSchema.safeParse(range).success ? range : undefined;
}

/**
 * A range relative to when the view loads, such as `{"last": "6h"}`. Its span
 * must resolve to RFC 3339 bounds, so it reaches back no further than year 0000.
 */
export const RelativeRangeSchema = z
  .object({
    last: z
      .string()
      .regex(/^[1-9]\d*[mhd]$/, 'minutes, hours, or days, such as 6h')
      .refine((last) => bounds(last) !== undefined, 'a span reaching back no further than year 0000, such as 6h')
  })
  .strict();

/** A view or panel time range. */
export const RangeSchema = z.union([AbsoluteRangeSchema, RelativeRangeSchema]);

/** A result column named by a chart definition. */
const column = z.string().min(1);

/** Fields every chart type shares. */
const chartFields = {
  title: z.string().min(1).optional(),
  sql: z.string().min(1).max(65_536),
  unit: z.string().min(1).optional(),
  range: RangeSchema.optional()
};

/**
 * A chart definition: REQ-025 fields plus an optional range.
 *
 * The SQL runs unchanged. A range-aware chart reads `$1`/`$2` as the range's
 * RFC 3339 bounds and `$3` as the bucket interval, casting them in SQL.
 */
export const ChartInputSchema = z.discriminatedUnion('type', [
  z.object({ type: z.literal('timeseries'), ...chartFields, x: column, y: z.array(column).min(1), series: column.optional() }).strict(),
  z.object({ type: z.literal('bar'), ...chartFields, x: column, y: z.array(column).min(1) }).strict(),
  z.object({ type: z.literal('stat'), ...chartFields, y: z.array(column).length(1).optional() }).strict(),
  z.object({ type: z.literal('table'), ...chartFields }).strict()
]);

/** The input of `CardHeader`: one exact Card plus the host's tabs. */
export const CardHeaderInputSchema = z
  .object({
    cardRef: CardRefSchema,
    tabs: z
      .array(z.object({ id: z.string().min(1), label: z.string().min(1) }).strict())
      .max(12)
      .optional(),
    tab: z.string().min(1).optional()
  })
  .strict();

/** A bind value: JSON null, boolean, number, or string. */
const queryParam = z.union([z.null(), z.boolean(), z.number(), z.string()]);

/** How an action's completion becomes observable. */
type Completion = 'host-navigation' | 'view-range';

/** One action a component emits, with its input schema and completion. */
interface ActionSpec {
  readonly description: string;
  readonly input: z.ZodType;
  readonly completion: Completion;
}

/** One catalog component's declaration. */
interface ComponentSpec {
  readonly description: string;
  readonly input: z.ZodType;
  readonly states: readonly string[];
  readonly emits: Readonly<Record<string, ActionSpec>>;
}

/** Every catalog component, by name. Primitives and shell components are not here. */
export const components = {
  Chart: {
    description:
      'One chart over read-only SQL: timeseries, bar, stat, or table, with title, source caption, ' +
      'Open in Query, and in-chart errors. $1/$2 bind the range as RFC 3339 strings and $3 the bucket.',
    input: ChartInputSchema,
    states: ['populated', 'empty', 'denied', 'error', 'loading'],
    emits: {
      'chart.range': {
        description: 'A time-series brush selected this absolute range for the whole view.',
        input: AbsoluteRangeSchema,
        completion: 'view-range'
      },
      'chart.open_in_query': {
        description: 'Open the exact executed SQL and its typed binds in Query.',
        input: z.object({ sql: z.string().min(1), params: z.array(queryParam) }).strict(),
        completion: 'host-navigation'
      }
    }
  },
  CardHeader: {
    description:
      'An exact Card version: name, kind, version select, status, uid copy, tabs, and the primary action.',
    input: CardHeaderInputSchema,
    states: ['populated', 'denied', 'error', 'loading'],
    emits: {
      'card.version': {
        description: 'Show another exact version of this Card.',
        input: z.object({ card_ref: CardRefSchema }).strict(),
        completion: 'host-navigation'
      },
      'card.tab': {
        description: 'Show one of the host-supplied tabs.',
        input: z.object({ tab: z.string().min(1) }).strict(),
        completion: 'host-navigation'
      },
      'card.open_in_observe': {
        description: "Open this Card's telemetry in Observe.",
        input: z.object({ card_ref: CardRefSchema }).strict(),
        completion: 'host-navigation'
      }
    }
  }
} as const satisfies Record<string, ComponentSpec>;

/** The name of a catalog component. */
export type ComponentName = keyof typeof components;

/** A validated chart definition. */
export type ChartInput = z.infer<typeof ChartInputSchema>;
/** A validated `CardHeader` input. */
export type CardHeaderInput = z.infer<typeof CardHeaderInputSchema>;
/** A view or panel range. */
export type Range = z.infer<typeof RangeSchema>;
/** An absolute range with RFC 3339 bounds. */
export type AbsoluteRange = z.infer<typeof AbsoluteRangeSchema>;

/**
 * Resolve `range` against the current clock; a view resolves its relative
 * range once per load so every panel reads the same bounds. An admitted range
 * always resolves to `AbsoluteRangeSchema` bounds.
 */
export function resolveRange(range: Range): AbsoluteRange {
  if ('from' in range) return range;
  const resolved = bounds(range.last);
  if (resolved === undefined) throw new RangeError(`range ${range.last} was not admitted by RangeSchema`);
  return resolved;
}

/**
 * A panel's place on the view's 12-column grid of 40px rows, in whole units.
 * Components never see it: they fill whatever box the view gives them.
 */
export const LayoutSchema = z
  .object({
    x: z.number().int().min(0).max(11),
    y: z.number().int().min(0),
    w: z.number().int().min(1).max(12),
    h: z.number().int().min(1).max(48)
  })
  .strict()
  .refine((layout) => layout.x + layout.w <= 12, { message: 'x + w must not exceed 12 columns', path: ['w'] });

/** A grid position and size; see `LayoutSchema`. */
export type Layout = z.infer<typeof LayoutSchema>;

/** One placed panel; its `input` is validated against its component's schema at load. */
export const PanelSchema = z
  .object({ id: z.string().min(1), component: z.string().min(1), input: z.unknown(), layout: LayoutSchema.optional() })
  .strict();

/** The catalog every view document is validated against. */
export const CATALOG_ID = 'wyrd.ui/v1';
/** The view-document format version. */
export const VIEW_VERSION = 'wyrd.view/v1';

/**
 * A view document: catalog panels under one range, resolved once per load so
 * every panel reads the same bounds. A panel that fails validation fails alone.
 */
export const ViewDocumentSchema = z
  .object({
    version: z.literal(VIEW_VERSION),
    catalogId: z.literal(CATALOG_ID),
    title: z.string().min(1).optional(),
    range: RangeSchema,
    panels: z
      .array(PanelSchema)
      .min(1)
      .max(24)
      .refine((panels) => new Set(panels.map((panel) => panel.id)).size === panels.length, 'panel ids must be unique')
  })
  .strict();

/** A document-valid view; panel inputs are still unvalidated. */
export type ViewDocument = z.infer<typeof ViewDocumentSchema>;
