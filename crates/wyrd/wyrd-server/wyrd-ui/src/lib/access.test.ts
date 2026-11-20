import { expect, test } from 'vitest';
import { accessRequest, operationName, requestable, scopeLabel } from './access';

test('a copied access request holds the account, tenant, action and supplied link, and nothing else', () => {
  expect(accessRequest({ identity: 'alice', tenant: 'acme', action: 'Read this Repository', target: 'https://wyrd.example/t/acme/repositories/ledger?version=1.0.0' })).toBe(
    [
      'Wyrd access request',
      'Account: alice',
      'Tenant: acme',
      'Requested action: Read this Repository',
      'Target: https://wyrd.example/t/acme/repositories/ledger?version=1.0.0',
      'Please grant this in Settings → Access, or tell me if the target is wrong.'
    ].join('\n')
  );
});

test('a family scope says it covers future versions and an exact version names its UID', () => {
  expect(scopeLabel({ card: { family: { kind: 'Repository', name: 'payments' } } })).toBe('Repository/payments · every version, including future ones');
  expect(scopeLabel({ card: { version: { kind: 'Prompt', name: 'ask', uid: '0190-uid' } } })).toBe('Prompt/ask · exact version 0190-uid');
  expect(scopeLabel('all')).toBe('Whole tenant');
  expect(scopeLabel({ bifrost: { schema: { catalog: 'bifrost', schema: 'drift' } } })).toBe('Bifrost schema bifrost.drift');
});

test('an operation reads resource:action and the tenant-wide bundle reads *', () => {
  expect(operationName({ resource: 'cards', action: 'read' })).toBe('cards:read');
  expect(operationName({ resource: 'wildcard', action: 'wildcard' })).toBe('*');
});

test('only a refusal or a not-found offers an access request, at the top level or inside a panel result', () => {
  expect(requestable({ code: 'WYRD_PERMISSION_403_DENIED_RBAC' })).toBe(true);
  expect(requestable({ result: { state: 'error', problem: { code: 'WYRD_REGISTRY_404_CARD_NOT_FOUND' } } })).toBe(true);
  expect(requestable({ code: 'WYRD_SPEC_400_VALIDATION' })).toBe(false);
  expect(requestable({ code: 'WYRD_UPSTREAM_503_UNAVAILABLE' })).toBe(false);
  expect(requestable({ result: { state: 'populated', data: {} } })).toBe(false);
});
