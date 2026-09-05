// @vitest-environment jsdom
import { spawn, type ChildProcess } from 'node:child_process';
import { once } from 'node:events';
import { fixtureChange, summaries } from '$lib/server/changes/fixtures';
import { beforeAll, afterAll, expect, test } from 'vitest';

let server: ChildProcess;
let origin: string;
beforeAll(async () => {
  server = spawn(
    process.execPath,
    ['node_modules/vite/bin/vite.js', '--host', '127.0.0.1', '--port', '0'],
    {
      env: {
        ...process.env,
        WYRD_UI_LOCAL_AUTH: 'true',
        WYRD_UI_MOCK_DATA: 'true',
        NO_COLOR: '1'
      },
      stdio: ['ignore', 'pipe', 'pipe']
    }
  );
  origin = await new Promise<string>((resolve, reject) => {
    let output = '';
    const timer = setTimeout(
      () => reject(new Error(`UI did not start: ${output}`)),
      20_000
    );
    server.stdout!.on('data', (chunk) => {
      output += chunk.toString();
      const url = output.match(/http:\/\/127\.0\.0\.1:\d+/)?.[0];
      if (url) {
        clearTimeout(timer);
        resolve(url);
      }
    });
    server.stderr!.on('data', (chunk) => {
      output += chunk.toString();
    });
    server.once('error', (error) => {
      clearTimeout(timer);
      reject(error);
    });
    server.once('exit', () => {
      clearTimeout(timer);
      reject(new Error(output));
    });
  });
}, 25_000);
afterAll(async () => {
  if (server && server.exitCode === null) {
    const closed = once(server, 'exit');
    server.kill();
    await closed;
  }
});

let cookie: string;
let csrf: string;
async function get(path: string) {
  return fetch(origin + path, { headers: { cookie } });
}
async function post(
  path: string,
  values: Record<string, string>,
  requestOrigin = origin
) {
  return fetch(origin + path, {
    method: 'POST',
    headers: { cookie, origin: requestOrigin, accept: 'text/html' },
    body: new URLSearchParams({ csrf, ...values }),
    redirect: 'manual'
  });
}
beforeAll(async () => {
  const login = await fetch(origin + '/?/login', {
    method: 'POST',
    headers: { origin, accept: 'text/html' },
    body: new URLSearchParams(),
    redirect: 'manual'
  });
  cookie = login.headers.get('set-cookie')!.split(';')[0];
  const html = await (await get('/t/acme')).text();
  csrf = html.match(/name="csrf" value="([a-f0-9]+)"/)![1];
});
test('list search and view filters restore through the real tenant load', async () => {
  const response = await get('/t/acme/changes?view=needs-attention');
  expect(response.status).toBe(200);
  const html = await response.text();
  expect(html).toContain('Your review requested');
  expect(html).toContain('Split ledger write path');
  expect(html).not.toContain('Add groundedness gate to agent');
  const noMatch = await (
    await get('/t/acme/changes?view=needs-attention&amp;q=absent'.replace('&amp;', '&'))
  ).text();
  expect(noMatch).toContain('No matching Change Requests');
  expect(noMatch).toContain('value="absent"');
  expect(noMatch).not.toContain('Split ledger write path');
  expect(await (await get('/t/acme/changes?view=verified')).text()).toContain(
    'Add groundedness gate to agent'
  );
  expect(await (await get('/t/acme/changes?view=closed')).text()).toContain(
    'Rollback fraud threshold'
  );
  expect((await get('/t/research/changes')).status).toBe(403);
}, 30000);
let draftPath = '';
test('incomplete draft saves, resumes, and accepts multiple subjects and requirements', async () => {
  const response = await post('/t/acme/changes/new?/save', {
    title: 'Safer checkout',
    intent: 'Hold borderline carts',
    impact: '',
    owner: 'j.reyes',
    teams: 'Product, Data Science, Engineering',
    subjects: '[]',
    claims: '[]',
    requestKey: 'draft-create'
  });
  expect(response.status).toBe(303);
  draftPath = response.headers.get('location')!;
  const html = await (await get(draftPath)).text();
  expect(html).toContain('Safer checkout');
  expect(html).toContain('Resume draft');
  expect(await (await get(draftPath + '/timeline')).text()).toContain(
    'Draft revision 1 saved'
  );
  const ownTeam = await (
    await get(
      '/t/acme/changes?view=needs-attention&team=Product%2C%20Data%20Science%2C%20Engineering'
    )
  ).text();
  expect(ownTeam).toContain('Safer checkout');
  const resumed = await (await get(draftPath + '?edit=draft')).text();
  expect(resumed).toContain('Save draft');
  expect(resumed).toContain('On new evidence can start a paid run');
  const subjects = [
    {
      id: 'first',
      repository: 'acme/checkout',
      provider: 'github',
      pr: '4412',
      url: 'https://github.com/acme/checkout/pull/4412',
      base: '9f2c1ad4',
      candidate: '6b81e0c7'
    },
    {
      id: 'second',
      repository: 'acme/ranking',
      provider: 'github',
      pr: '221',
      url: '',
      base: 'c40b7e15',
      candidate: '88ff3a02'
    }
  ];
  const claims = [
    {
      id: 'claim-1',
      title: 'No ranking regression',
      checks: [
        { name: 'Eval gate', required: true, mode: 'on-new-evidence', billable: true }
      ]
    }
  ];
  const saved = await post(draftPath + '?/save', {
    title: 'Safer checkout',
    intent: 'Hold borderline carts',
    impact: 'Fewer capture errors',
    owner: 'j.reyes',
    teams: 'Product, Data Science, Engineering',
    subjects: JSON.stringify(subjects),
    claims: JSON.stringify(claims),
    revision: 'rev_01',
    requestKey: 'draft-update'
  });
  expect(saved.status).toBe(303);
  const updated = await (await get(draftPath)).text();
  expect(updated).toContain('acme/ranking');
  expect(updated).toContain('No ranking regression');
  expect(
    (await post(draftPath + '?/save', { csrf: '', requestKey: 'invalid' })).status
  ).toBe(403);
}, 30000);
test('Overview leads with intent and impact, exact subjects, and separate decisions', async () => {
  const html = await (await get('/t/acme/changes/change_01')).text();
  for (const text of [
    'Action summary',
    'Intent and impact',
    'r.okafor (Product)',
    'm.linden (Data Science)',
    'j.reyes (Engineering)',
    'subject_worker',
    'c40b7e15',
    '88ff3a02',
    'Lifecycle',
    'Verification',
    'Approval',
    'Override',
    'does not verify',
    'Review changes'
  ])
    expect(html).toContain(text);
  expect(html).toContain('id="subjects"');
  const prior = await get('/t/acme/changes/change_01?revision=rev_06');
  expect(prior.status).toBe(200);
  expect(await prior.text()).toContain('revision 6');
});
test('verification exposes prerequisites, provenance and confirmed run actions', async () => {
  const path = '/t/acme/changes/change_01/verification';
  const html = await (await get(path)).text();
  for (const text of [
    'View missing Evidence',
    'carried forward',
    'stale',
    'timed out',
    'inconclusive',
    'Rerun',
    'billable'
  ])
    expect(html.includes(text), text).toBe(true);
  const values = { revision: 'rev_07', checkId: 'pii', requestKey: 'run-pii' };
  expect((await post(path + '?/run', values)).status).toBe(400);
  expect((await post(path + '?/run', { ...values, confirmed: 'true' })).status).toBe(200);
  expect((await post(path + '?/run', { ...values, confirmed: 'true' })).status).toBe(200);
  expect(
    (
      await post(path + '?/run', {
        ...values,
        requestKey: 'repeat-pii',
        confirmed: 'true'
      })
    ).status
  ).toBe(409);
  expect(
    (
      await post(path + '?/run', {
        ...values,
        requestKey: 'missing-evidence',
        checkId: 'eval-gate',
        confirmed: 'true'
      })
    ).status
  ).toBe(409);
});
test('review writes anchored mentions, rejects stale edits, preserves history, resolves and reopens', async () => {
  const path = '/t/acme/changes/change_01/review';
  const html = await (await get(path)).text();
  expect(html.includes('Submit review')).toBe(true);
  const values = {
    operation: 'edit',
    threadId: 'thread_source',
    commentId: 'comment_source',
    expected: 'comment_source_v2',
    revision: 'rev_07',
    body: 'Use the Policy Card. @m.linden',
    requestKey: 'edit-source'
  };
  expect((await post(path + '?/review', values)).status).toBe(200);
  expect((await post(path + '?/review', values)).status).toBe(200);
  const stale = await post(path + '?/review', {
    ...values,
    body: 'Keep this draft',
    requestKey: 'stale-edit'
  });
  expect(stale.status).toBe(409);
  const conflict = await stale.text();
  expect(conflict.includes('Keep this draft')).toBe(true);
  expect(conflict.includes('WYRD_SPEC_409_CONFLICT')).toBe(true);
  expect(
    (
      await post(path + '?/review', {
        operation: 'reopen',
        threadId: 'thread_source',
        revision: 'rev_07',
        requestKey: 'reopen'
      })
    ).status
  ).toBe(200);
  expect(
    (
      await post(path + '?/review', {
        operation: 'resolve',
        threadId: 'thread_source',
        revision: 'rev_07',
        requestKey: 'resolve'
      })
    ).status
  ).toBe(200);
  const reviewed = await (await get(path)).text();
  expect(reviewed.includes('Edit history')).toBe(true);
  expect(reviewed.includes('Use the Policy Card. @m.linden')).toBe(true);
  const approval = await post(path + '?/review', {
    operation: 'decision',
    decision: 'approve',
    revision: 'rev_07',
    requestKey: 'approval',
    body: 'Ready from Engineering.'
  });
  expect(approval.status).toBe(200);
  expect(
    (
      await post(path + '?/review', {
        operation: 'comment',
        revision: 'rev_06',
        body: 'Old revision',
        requestKey: 'stale-comment'
      })
    ).status
  ).toBe(409);
});
test('timeline and read-only subject drilldowns keep exact revisions and anchored discussion', async () => {
  const timeline = await get('/t/acme/changes/change_01/timeline');
  expect(timeline.status).toBe(200);
  const html = await timeline.text();
  for (const text of [
    'Audit',
    'Review activity',
    'Evidence',
    'Override',
    'Close Change Request'
  ])
    expect(html.includes(text), text).toBe(true);
  const path = '/t/acme/changes/change_01/subjects/subject_api';
  const subject = await (await get(path)).text();
  for (const text of [
    '9f2c1ad4',
    '6b81e0c7',
    'Read-only',
    'src/capture/rank.rs',
    'Start discussion',
    'View in provider'
  ])
    expect(subject.includes(text), text).toBe(true);
  const anchor = {
    kind: 'source',
    revision: 'rev_07',
    target: 'subject_api',
    file: 'src/capture/rank.rs',
    line: 121,
    side: 'new'
  };
  expect(
    (
      await post(path + '?/review', {
        operation: 'comment',
        revision: 'rev_07',
        requestKey: 'source-comment',
        body: 'Is the low band covered? @m.linden',
        anchor: JSON.stringify(anchor)
      })
    ).status
  ).toBe(200);
  const nextFile = await (await get(path + '?file=tests%2Fcapture.rs')).text();
  expect(nextFile.includes('assert_eq!')).toBe(true);
  expect((await get(path + '?file=not-a-file')).status).toBe(404);
});
test('global mock disable withholds Changes without dropping URL filters', async () => {
  const response = await fetch(
    origin + '/t/acme/changes?view=needs-attention&q=ranking',
    { headers: { cookie: cookie + '; wyrd_ui_mock_data=false' } }
  );
  const html = await response.text();
  expect(html.includes('WYRD_SPEC_502_UPSTREAM_FAILURE')).toBe(true);
  expect(html.includes('value="ranking"')).toBe(true);
  expect(html.includes('Raise checkout ranking cutoff')).toBe(false);
});

test('Verifier identity stays inspectable without a fabricated Card destination', async () => {
  const html = await (await get('/t/acme/changes/change_01/verification')).text();
  expect(html).toContain('checkout-verifier v5');
  expect(html).not.toContain('/cards/card_verifier_01');
});

test('seeded ledger draft resumes and saves its own content unchanged', async () => {
  const path = '/t/acme/changes/change_03';
  const expected = fixtureChange(summaries[2]);
  const readForm = async (suffix = '') => {
    const html = await (await get(path + '?edit=draft' + suffix)).text();
    const doc = new DOMParser().parseFromString(html, 'text/html');
    const form = doc.querySelector<HTMLFormElement>('form.draft')!;
    return Object.fromEntries(new FormData(form).entries()) as Record<string, string>;
  };
  const before = await readForm();
  expect(before.intent).toBe(expected.intent);
  expect(before.impact).toBe(expected.impact);
  expect(before.owner).toBe(expected.owner);
  expect(before.teams).toBe(expected.owners);
  expect(JSON.parse(before.subjects)).toEqual(expected.subjects);
  expect(JSON.parse(before.claims)).toEqual(
    expected.claims.map((claim) => ({
      id: claim.id,
      title: claim.title,
      checks: claim.checks.map(({ name, required, mode, billable }) => ({
        name,
        required,
        mode,
        billable
      }))
    }))
  );
  expect((await post(path + '?/save', before)).status).toBe(303);
  expect((await post(path + '?/save', before)).status).toBe(303);
  const after = await readForm();
  for (const key of ['title', 'intent', 'impact', 'owner', 'teams'])
    expect(after[key], key).toBe(before[key]);
  for (const key of ['subjects', 'claims'])
    expect(JSON.parse(after[key])).toEqual(JSON.parse(before[key]));
  expect(after.revision).toBe('rev_08');
  const historical = await (await get(path + '?revision=rev_07')).text();
  expect(historical).toContain('acme/ledger');
  expect(historical).toContain(expected.intent);
});

test('equal-number old and new diff rows keep distinct validated discussions', async () => {
  const path = '/t/acme/changes/change_01/subjects/subject_model';
  const documentAt = async (route: string) =>
    new DOMParser().parseFromString(await (await get(route)).text(), 'text/html');
  const initial = await documentAt(path);
  expect(initial.querySelectorAll('#line-old-1')).toHaveLength(1);
  expect(initial.querySelectorAll('#line-new-1')).toHaveLength(1);
  const anchor = {
    kind: 'source',
    revision: 'rev_07',
    target: 'subject_model',
    file: 'eval/threshold.yaml',
    line: 1,
    side: 'old'
  };
  for (const side of ['old', 'new']) {
    const input = {
      operation: 'comment',
      revision: 'rev_07',
      requestKey: 'diff-' + side,
      body: 'Discussion on ' + side,
      anchor: JSON.stringify({ ...anchor, side })
    };
    expect((await post(path + '?/review', input)).status).toBe(200);
    expect((await post(path + '?/review', input)).status).toBe(200);
  }
  const source = await documentAt(path);
  const oldLinks = source.querySelectorAll('#line-old-1 a');
  const newLinks = source.querySelectorAll('#line-new-1 a');
  expect(oldLinks).toHaveLength(1);
  expect(newLinks).toHaveLength(1);
  expect(oldLinks[0].getAttribute('href')).not.toBe(newLinks[0].getAttribute('href'));
  const review = await documentAt('/t/acme/changes/change_01/review');
  for (const side of ['old', 'new'])
    expect(review.querySelector(`a[href$="#line-${side}-1"]`)).not.toBeNull();
  for (const patch of [
    { side: undefined },
    { side: 'invalid' },
    { line: 999 },
    { target: 'subject_api' },
    { file: 'other.yaml' },
    { revision: 'rev_06' }
  ]) {
    const result = await post(path + '?/review', {
      operation: 'comment',
      revision: 'rev_07',
      requestKey: JSON.stringify(patch),
      body: 'Invalid coordinate',
      anchor: JSON.stringify({ ...anchor, ...patch })
    });
    expect(result.status).toBe(patch.revision ? 409 : 400);
  }
  expect((await documentAt(path)).querySelectorAll('.diff-line a')).toHaveLength(2);
});
