import {
  CardRefSchema,
  loadPanel,
  wyrdProblem,
  type ArtifactTableProps,
  type CardFactsProps,
  type CardHeaderProps,
  type ComponentName,
  type Problem,
  type PromptMessagesProps,
  type PromptVariablesProps,
  type RelationshipListProps,
  type UsedByProps,
  type VersionDiffProps
} from 'wyrd-ai-ui/loaders';
import { nodeExecutor } from 'wyrd-ai-ui/node';
import { reject } from '$lib/server/auth/session';
import { CARD_TABS, PROMPT_TABS } from '$lib/cards-host';
import type { PageServerLoad } from './$types';

/**
 * One exact Card version as this tenant's browser session. The URL must name
 * the exact kind, name and version; a missing or invalid one is a validation
 * problem with no read, never a latest-version lookup. The header's Card
 * read gates every other panel, so a denied or missing Card issues no
 * dependent read; only the selected tab's panels load.
 */
export const load: PageServerLoad = async ({ locals, url, request }) => {
  const query = Object.fromEntries(['kind', 'name', 'version', 'uid'].flatMap((key) => (url.searchParams.has(key) ? [[key, url.searchParams.get(key)]] : [])));
  const parsed = CardRefSchema.safeParse(query);
  if (!parsed.success) {
    const [issue] = parsed.error.issues;
    return { problem: wyrdProblem('validation', `The Card URL ${issue.path.join('.') || 'reference'}: ${issue.message}`) };
  }
  const cardRef = parsed.data;
  const tabs = cardRef.kind === 'Prompt' ? PROMPT_TABS : CARD_TABS;
  const tab = tabs.find((each) => each.id === url.searchParams.get('tab'))?.id ?? 'overview';
  const compare = url.searchParams.get('compare') ?? undefined;
  const session = locals.browserSession ?? reject('unauthenticated');
  const context = { executor: nodeExecutor(session.client()), signal: request.signal };
  const panel = (component: ComponentName, input: object) => loadPanel(component, input, context);

  const header = (await panel('CardHeader', { cardRef, tabs, tab })) as CardHeaderProps;
  if (header.result.state !== 'populated') return { cardRef, tab, compare, header };
  const one = { cardRef };
  if (tab === 'files') return { cardRef, tab, compare, header, files: (await panel('ArtifactTable', one)) as ArtifactTableProps };
  if (tab === 'yaml') return { cardRef, tab, compare, header, yaml: (await panel('CardFacts', { cardRef, yaml: true })) as CardFactsProps };
  if (tab === 'versions')
    return { cardRef, tab, compare, header, diff: (await panel('VersionDiff', compare ? { cardRef, otherVersion: compare } : one)) as VersionDiffProps | Problem };
  const [facts, relationships] = (await Promise.all([panel('CardFacts', one), panel('RelationshipList', one)])) as [CardFactsProps, RelationshipListProps];
  if (cardRef.kind !== 'Prompt') return { cardRef, tab, compare, header, facts, relationships };
  const [messages, variables, usedBy, diff] = (await Promise.all([
    panel('PromptMessages', one),
    panel('PromptVariables', one),
    panel('UsedBy', one),
    panel('VersionDiff', one)
  ])) as [PromptMessagesProps, PromptVariablesProps, UsedByProps, VersionDiffProps];
  return { cardRef, tab, compare, header, facts, relationships, prompt: { messages, variables, usedBy, diff } };
};
