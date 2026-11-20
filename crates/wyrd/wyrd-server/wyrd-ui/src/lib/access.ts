/**
 * Presentation of the server's access contracts: labels for scopes, operations
 * and sources, the operation choices of a custom role, and the text of a copied
 * access request. The server alone decides what a role grants at a scope; this
 * module only words what it answered.
 */
import type { CardTypes } from 'wyrd-ai';

type Scope = CardTypes.PermissionScope;

/**
 * The tenant operations a custom role may select, by resource. The compiler
 * holds each name to the server's vocabulary; the platform plane's resources
 * are not tenant role material and are left out.
 */
export const OPERATIONS = {
  cards: ['read', 'write', 'delete'],
  artifacts: ['read', 'write'],
  executions: ['read'],
  verifier: ['run', 'approve'],
  triggers: ['write'],
  policy: ['lock'],
  services: ['install'],
  operators: ['read', 'write', 'invoke'],
  gateway: ['read', 'write', 'delete', 'invoke'],
  gateway_payload: ['read'],
  bifrost_query: ['read'],
  bifrost_table: ['read', 'write', 'install'],
  bifrost_record: ['write'],
  bifrost_peer: ['invoke'],
  tool: ['invoke'],
  audit: ['read'],
  github_connections: ['read', 'write'],
  identity_connections: ['write'],
  service_accounts: ['write'],
  users: ['write'],
  personal_credentials: ['read', 'write']
} as const satisfies Partial<Record<CardTypes.Resource & string, readonly (CardTypes.Action2 & string)[]>>;

/** Every selectable operation as its `resource:action` name. */
export const OPERATION_NAMES: readonly string[] = Object.entries(OPERATIONS).flatMap(([resource, actions]) =>
  actions.map((action) => `${resource}:${action}`)
);

/** The Card kinds a scope may name, in the server's spelling. */
export const CARD_KINDS = ['Repository', 'Service', 'Model', 'Agent', 'Verifier', 'Data', 'Artifact', 'Experiment', 'Prompt', 'Mcp', 'Audit', 'Trigger', 'Operator'] as const satisfies readonly CardTypes.CardKind[];

/** The scope shapes the grant form offers; the value travels as the form's `scopeType`. */
export const SCOPE_TYPES = [
  ['all', 'Whole tenant'],
  ['card_family', 'Card family (every version)'],
  ['card_version', 'Card exact version'],
  ['bifrost_schema', 'Bifrost schema'],
  ['bifrost_table', 'Bifrost table'],
  ['gateway_deployment', 'Gateway deployment'],
  ['gateway_model', 'Gateway model'],
  ['github_connection', 'GitHub connection'],
  ['tool', 'Built-in tool']
] as const;

/** `resource:action` of one operation or permission; the tenant-wide bundle reads `*`. */
export function operationName(operation: { readonly resource: unknown; readonly action: unknown }): string {
  const name = (part: unknown) => (typeof part === 'string' ? part : 'any_of');
  return operation.resource === 'wildcard' && operation.action === 'wildcard' ? '*' : `${name(operation.resource)}:${name(operation.action)}`;
}

/** What `scope` covers, in words; a family says that it includes future versions. */
export function scopeLabel(scope: Scope): string {
  if (scope === 'all') return 'Whole tenant';
  if ('card' in scope) {
    if ('family' in scope.card) return `${scope.card.family.kind}/${scope.card.family.name} · every version, including future ones`;
    return `${scope.card.version.kind}/${scope.card.version.name} · exact version ${scope.card.version.uid}`;
  }
  if ('bifrost' in scope) {
    if ('schema' in scope.bifrost) return `Bifrost schema ${scope.bifrost.schema.catalog}.${scope.bifrost.schema.schema}`;
    return `Bifrost table ${scope.bifrost.table.catalog}.${scope.bifrost.table.schema} · ${scope.bifrost.table.table_uid}`;
  }
  if ('gateway' in scope) {
    if ('deployment' in scope.gateway) return `Gateway deployment ${scope.gateway.deployment.deployment}`;
    return `Gateway model ${scope.gateway.model.deployment}/${scope.gateway.model.model}`;
  }
  if ('github_connection' in scope) return `GitHub connection ${scope.github_connection}`;
  return `Built-in tool ${scope.tool}`;
}

/** What a copied access request names: who asks, where, for what, and the link or identifier they supplied. */
export interface AccessRequest {
  /** The signed-in account's display name. */
  readonly identity: string;
  /** The tenant's route key. */
  readonly tenant: string;
  /** The action the person wanted, in plain words. */
  readonly action: string;
  /** The link or identifier the person supplied, verbatim. */
  readonly target: string;
}

/**
 * The text of an access request for the person's own channel. It carries only
 * what the person already holds: nothing is looked up, so an unknown target
 * and an inaccessible one read the same.
 */
export function accessRequest(request: AccessRequest): string {
  return [
    'Wyrd access request',
    `Account: ${request.identity}`,
    `Tenant: ${request.tenant}`,
    `Requested action: ${request.action}`,
    `Target: ${request.target}`,
    'Please grant this in Settings → Access, or tell me if the target is wrong.'
  ].join('\n');
}
