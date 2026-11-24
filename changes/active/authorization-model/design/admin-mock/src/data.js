// Sample data shaped after the wire views named in each comment. Every address is example.test.

export const TENANT = 'gtest';
export const ME = { email: 'steven@example.test', name: 'Steven F.', method: 'Google' };

// PrincipalSummary (+ proposed sign-in method and role summary; see notes open question 1).
export const USERS = [
  { id: 'u1', email: 'steven@example.test', name: 'Steven F.', method: 'Google', roles: ['admin'], status: 'active', last: '2 min ago' },
  { id: 'u2', email: 'priya@example.test', name: 'Priya N.', method: 'GitHub', roles: ['editor'], status: 'active', last: 'yesterday' },
  { id: 'u3', email: 'li@example.test', name: 'li.wei', method: 'Password', roles: ['viewer', 'approver'], status: 'active', last: '3 days ago' },
  { id: 'u4', email: 'old@example.test', name: 'Sam K.', method: 'GitHub', roles: ['viewer'], status: 'suspended', last: '41 days ago' },
];
// UserInvitationView
export const INVITES = [
  { id: 'i1', email: 'dana@example.test', roles: ['viewer'], delivery: 'emailed', expires: 'in 21 h' },
  { id: 'i2', email: 'omar@example.test', roles: ['editor'], delivery: 'failed', expires: 'in 6 h' },
];
export const SERVICES = [
  { id: 's1', name: 'ci-verifier', restriction: 'workload', keys: 2, lastUsed: '4 min ago', status: 'active' },
  { id: 's2', name: 'nightly-drift', restriction: 'workload', keys: 1, lastUsed: '9 h ago', status: 'active' },
];
export const ROLES = [
  { name: 'admin', builtIn: true, summary: 'Everything, including people, sign-in and secrets', ops: ['*'], members: 1 },
  { name: 'editor', builtIn: true, summary: 'Write Cards and tables, run Verifiers, invoke the gateway and Operators', ops: ['cards:read', 'cards:write', 'cards:delete', 'executions:read', 'artifacts:read', 'artifacts:write', 'operators:read', 'operators:invoke', 'gateway:read', 'gateway:invoke', 'bifrost_table:read', 'bifrost_table:write', 'bifrost_query:read', 'bifrost_record:write', 'verifier:run', 'triggers:write', 'policy:lock', 'services:install'], members: 1 },
  { name: 'viewer', builtIn: true, summary: 'Read Cards, executions, artifacts and tables', ops: ['cards:read', 'executions:read', 'artifacts:read', 'operators:read', 'gateway:read', 'bifrost_table:read', 'bifrost_query:read'], members: 2 },
  { name: 'approver', builtIn: true, summary: 'Read Cards and approve Verifiers', ops: ['cards:read', 'verifier:approve'], members: 1 },
  { name: 'auditor', builtIn: true, summary: 'Read the audit log', ops: ['audit:read'], members: 0 },
  { name: 'workload', builtIn: true, summary: 'What a service account needs to run verification', ops: ['cards:read', 'executions:read', 'verifier:run', 'bifrost_record:write', 'gateway:invoke', 'operators:invoke'], members: 2 },
];
// ProviderCredentialView, OperatorConnectionView, OIDC connection, TrustedIssuerView.
export const SECRETS = [
  { name: 'openai-prod', kind: 'Gateway provider key', usedBy: 'openai · 3 deployments', state: 'active', rotated: '12 days ago' },
  { name: 'anthropic-prod', kind: 'Gateway provider key', usedBy: 'anthropic · 2 deployments', state: 'active', rotated: '94 days ago' },
  { name: 'slack-alerts', kind: 'Operator connection', usedBy: '2 Operators', state: 'active', rotated: '30 days ago' },
  { name: 'Okta (company SSO)', kind: 'SSO client secret', usedBy: 'Company sign-in', state: 'active', rotated: '5 days ago' },
  { name: 'old-azure', kind: 'Gateway provider key', usedBy: 'nothing', state: 'revoked', rotated: '200 days ago' },
];
export const WORKSPACES = [
  { slug: 'acme', name: 'Acme', users: 14, status: 'active', created: 'Oct 2' },
  { slug: 'gtest', name: 'gtest', users: 4, status: 'active', created: 'today' },
  { slug: 'ghtest', name: 'ghtest', users: 1, status: 'active', created: 'today' },
  { slug: 'old-co', name: 'Old Co', users: 3, status: 'suspended', created: 'Jun 9' },
];

// Every preview state, for the strip menu and the self-check.
export const STATES = [
  ['Sign-in page · provider buttons', '#/t/gtest/login'],
  ['Invitation · join buttons', '#/t/gtest/signup'],
  ['People', '#/t/gtest/settings/people'],
  ['People · pending invitations', '#/t/gtest/settings/people?filter=invited'],
  ['People · invite drawer', '#/t/gtest/settings/people?drawer=invite'],
  ['People · invitations created', '#/t/gtest/settings/people?sent=1'],
  ['People · person drawer', '#/t/gtest/settings/people?drawer=u3'],
  ['Service accounts', '#/t/gtest/settings/service-accounts'],
  ['Roles', '#/t/gtest/settings/roles'],
  ['Roles · editor expanded', '#/t/gtest/settings/roles?open=editor'],
  ['Sign-in · company SSO not set up', '#/t/gtest/settings/sign-in'],
  ['Sign-in · setup step 2', '#/t/gtest/settings/sign-in?setup=2'],
  ['Sign-in · company SSO active', '#/t/gtest/settings/sign-in?sso=active'],
  ['Sign-in · require confirmation', '#/t/gtest/settings/sign-in?sso=active&confirm=require'],
  ['Sign-in · required', '#/t/gtest/settings/sign-in?sso=active&required=1'],
  ['Secrets', '#/t/gtest/settings/secrets'],
  ['Secrets · add key drawer', '#/t/gtest/settings/secrets?drawer=add'],
  ['Secrets · revoke confirmation', '#/t/gtest/settings/secrets?confirm=openai-prod'],
  ['Integrations', '#/t/gtest/settings/integrations'],
  ['Your account', '#/t/gtest/account'],
  ['Platform · overview', '#/platform/ui'],
  ['Platform · workspaces', '#/platform/ui/workspaces'],
  ['Platform · sign-in providers', '#/platform/ui/providers'],
  ['Platform · mail', '#/platform/ui/mail'],
  ['Platform · admins', '#/platform/ui/admins'],
  ['Platform · stored secrets', '#/platform/ui/secrets'],
];
