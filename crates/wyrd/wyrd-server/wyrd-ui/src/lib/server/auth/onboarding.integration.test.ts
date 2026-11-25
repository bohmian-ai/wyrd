// @vitest-environment node
/**
 * Production onboarding journeys in a real Chromium. `identity_ui_e2e.rs`
 * (`onboarding_ui_journey`) starts one bound Wyrd server over repository
 * Postgres and one production `node build` BFF at a public origin, and runs
 * this file with the `WYRD_UI_ONBOARDING` fixture. The repository Mailpit is
 * the deployment's relay: every link and code a page asks for is read from
 * the mailbox it was sent to, as a person would. Nothing here mocks the BFF,
 * the server or the mail. The deployment's Google is this file's fake Google,
 * and its Turnstile Siteverify this file's fake, at the endpoints the server
 * was started with; the browser loads a stand-in Turnstile widget.
 *
 * The platform API, called with the operator token, stands in for the
 * operator's console; every visitor step goes through the pages.
 */
import { createServer } from 'node:http';
import { chromium, type Browser, type BrowserContext, type Page } from 'playwright';
import { afterAll, beforeAll, expect, test } from 'vitest';
import { startFakeGoogle, type GoogleAccount } from '$lib/fake-google.integration';

type Onboarding = {
  origin: string;
  bff: string;
  server: string;
  operator: string;
  mail: { host: string; port: number; api: string };
  google: string;
  siteverify: string;
};

const journey = JSON.parse(process.env.WYRD_UI_ONBOARDING ?? 'null') as Onboarding;
const journeyTimeout = 120_000;
let chrome: Browser;
let google: { issuer: string; account: GoogleAccount };
/** Every `(secret, token)` the fake Siteverify was asked about, in order. */
const siteverified: { secret: string; token: string }[] = [];

beforeAll(async () => {
  // The public origin is the load balancer: Chromium connects every request for it to the BFF.
  chrome = await chromium.launch({ args: [`--host-rules=MAP ${new URL(journey.origin).host} ${new URL(journey.bff).host}`] });
  google = await startFakeGoogle(Number(new URL(journey.google).port));
  // Siteverify passes only tokens the stand-in widget issued as passing, solved on the public host for signup.
  const siteverify = createServer((request, response) => {
    let form = '';
    request.on('data', (chunk: Buffer) => (form += chunk.toString()));
    request.on('end', () => {
      const fields = new URLSearchParams(form);
      const token = fields.get('response') ?? '';
      siteverified.push({ secret: fields.get('secret') ?? '', token });
      const success = token.startsWith('pass-');
      response.writeHead(200, { 'content-type': 'application/json' });
      response.end(JSON.stringify({ success, hostname: new URL(journey.origin).hostname, action: 'signup', 'error-codes': success ? [] : ['invalid-input-response'] }));
    });
  });
  await new Promise<void>((resolve) => siteverify.listen(Number(new URL(journey.siteverify).port), '127.0.0.1', resolve));
  siteverify.unref();
  // The pages treat a server that is not ready as down; Forge joins shortly after bind.
  for (let attempt = 0; (await fetch(`${journey.server}/readyz`)).status !== 200; attempt++) {
    if (attempt === 240) throw new Error('the server never became ready');
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
}, 90_000);
afterAll(async () => {
  await chrome?.close();
});

/** Call one platform management route as the operator; returns the JSON answer. */
async function platform(method: string, path: string, body?: unknown): Promise<unknown> {
  const response = await fetch(journey.server + path, {
    method,
    headers: {
      'x-wyrd-access-token': `Bearer ${journey.operator}`,
      ...(body === undefined ? {} : { 'content-type': 'application/json' })
    },
    body: body === undefined ? undefined : JSON.stringify(body)
  });
  const answer = await response.json().catch(() => null);
  expect(response.ok, `${method} ${path}: ${JSON.stringify(answer)}`).toBe(true);
  return answer;
}

/** The operator turns the Mailpit relay and public signup on. */
async function enableMailAndSignup(): Promise<void> {
  await platform('PUT', '/platform/mail', {
    host: journey.mail.host,
    port: journey.mail.port,
    tls: 'none',
    sender: 'onboarding@wyrd.test'
  });
  await platform('PUT', '/platform/login-settings', { password_enabled: true, signup_enabled: true });
}

/** A fresh browser profile at the public origin. */
async function visitor(): Promise<{ context: BrowserContext; page: Page }> {
  const context = await chrome.newContext();
  const page = await context.newPage();
  page.setDefaultTimeout(10_000);
  return { context, page };
}

/** The text of the newest message Mailpit holds for `address`, waiting for it to arrive. */
async function newestMail(address: string): Promise<string> {
  for (let attempt = 0; attempt < 100; attempt++) {
    const search = await fetch(`${journey.mail.api}/api/v1/search?query=${encodeURIComponent(`to:"${address}"`)}`);
    const { messages } = (await search.json()) as { messages: { ID: string }[] };
    if (messages.length) {
      const message = await fetch(`${journey.mail.api}/api/v1/message/${messages[0].ID}`);
      return ((await message.json()) as { Text: string }).Text;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error('no message arrived');
}

/** How many messages Mailpit holds for `address`. */
async function mailCount(address: string): Promise<number> {
  const search = await fetch(`${journey.mail.api}/api/v1/search?query=${encodeURIComponent(`to:"${address}"`)}`);
  return ((await search.json()) as { messages: unknown[] }).messages.length;
}

/** The link a proof message carries, its proof in the fragment. */
function mailedLink(text: string): string {
  const link = /(https?:\/\/\S+#proof=\S+)/.exec(text)?.[1];
  expect(link, 'the message links its proof in a fragment').toBeDefined();
  return link!;
}

/** Sign in to workspace `slug` with a password and wait for its home page. */
async function signIn(page: Page, slug: string, username: string, password: string): Promise<void> {
  await page.goto(`${journey.origin}/t/${slug}/login`);
  await page.getByLabel('Username').fill(username);
  await page.getByLabel('Password', { exact: true }).fill(password);
  await page.getByRole('button', { name: 'Sign in', exact: true }).click();
  await page.waitForURL(`${journey.origin}/t/${slug}`);
}

/** Post a form as the `wyrd-cli` client from the page's origin, as the CLI would; returns the JSON answer. */
async function cliForm(page: Page, path: string, fields: Record<string, string>): Promise<unknown> {
  return page.evaluate(
    async ([path, fields]) =>
      (await fetch(path, { method: 'POST', body: new URLSearchParams({ client_id: 'wyrd-cli', ...fields }) })).json(),
    [path, fields] as const
  );
}

/** Distinguishes this run's addresses and slugs from earlier runs against the same Mailpit. */
const run = Date.now().toString(36);

/** A workspace created through the email signup pages, and its administrator. */
type Workspace = { slug: string; name: string; owner: string; username: string; password: string };

/**
 * Set up a workspace the way a visitor does: email signup, the mailed link,
 * then naming it with an administrator password. The owner's address is the
 * workspace's verified recovery email, so discovery and reset reach it.
 */
async function signedUpWorkspace(label: string): Promise<Workspace> {
  await enableMailAndSignup();
  const ws = {
    slug: `${label}-${run}`,
    name: `Journey ${label}`,
    owner: `${label}-${run}@example.test`,
    username: 'owner',
    password: 'correct horse battery staple'
  };
  const { context, page } = await visitor();
  try {
    await page.goto(`${journey.origin}/signup`);
    await page.getByLabel('Email').fill(ws.owner);
    await page.getByRole('button', { name: 'Continue with email' }).click();
    await page.getByRole('heading', { name: 'Check your email' }).waitFor();
    await page.goto(mailedLink(await newestMail(ws.owner)));
    await page.getByRole('link', { name: 'Name your workspace' }).click();
    await page.getByLabel('Workspace name').fill(ws.name);
    await page.getByLabel('URL slug').fill(ws.slug);
    await page.getByLabel('Your username').fill(ws.username);
    await page.getByLabel('Your password', { exact: true }).fill(ws.password);
    await page.getByRole('button', { name: 'Create workspace' }).click();
    await page.waitForURL(`${journey.origin}/t/${ws.slug}`);
  } finally {
    await context.close();
  }
  return ws;
}

test('the root page offers creation and discovery only when they are usable', async () => {
  const { context, page } = await visitor();
  try {
    // Private defaults: no relay and signup off, so neither link is offered.
    await page.goto(`${journey.origin}/`);
    await expect.poll(() => page.getByRole('heading', { level: 1 }).textContent()).toBe('Welcome to Wyrd');
    expect(await page.getByRole('link', { name: 'Create a workspace' }).count()).toBe(0);
    expect(await page.getByRole('link', { name: 'Find my workspaces' }).count()).toBe(0);
    // The workspace URL entry stays the way to an existing workspace.
    expect(await page.getByLabel('Tenant').count()).toBe(1);
    // Signup is off by default, and its page says how to join instead.
    await page.goto(`${journey.origin}/signup`);
    await page.getByText('Creating a workspace is turned off on this deployment.').waitFor();
    expect(await page.getByLabel('Email').count()).toBe(0);

    await enableMailAndSignup();
    await page.goto(`${journey.origin}/`);
    expect(await page.getByRole('link', { name: 'Create a workspace' }).getAttribute('href')).toBe('/signup');
    expect(await page.getByRole('link', { name: 'Find my workspaces' }).getAttribute('href')).toBe('/workspaces');
  } finally {
    await context.close();
  }
}, journeyTimeout);

test('an emailed proof creates a workspace whose administrator signs in with a passphrase', async () => {
  await enableMailAndSignup();
  const owner = `owner-${run}@example.test`;
  const slug = `journey-${run}`;
  const { context, page } = await visitor();
  try {
    await page.goto(`${journey.origin}/signup`);
    expect(await page.getByRole('heading', { level: 1 }).textContent()).toBe('Create a workspace');
    await page.getByLabel('Email').fill(owner);
    await page.getByRole('button', { name: 'Continue with email' }).click();
    await page.getByRole('heading', { name: 'Check your email' }).waitFor();
    expect(await page.getByTestId('check-email').textContent()).toContain(owner);

    // The message links the verify page; opening it there POSTs the proof and clears the fragment.
    const link = mailedLink(await newestMail(owner));
    expect(new URL(link).pathname).toBe('/signup/verify');
    await page.goto(link);
    await page.getByRole('heading', { name: 'Email confirmed' }).waitFor();
    expect(page.url()).toBe(`${journey.origin}/signup/verify`);
    await page.getByRole('link', { name: 'Name your workspace' }).click();

    await page.getByRole('heading', { name: 'Name your workspace' }).waitFor();
    // GET /auth/signup/proof says the proof is a mailbox, so the form asks for the administrator's sign-in.
    await expect.poll(() => page.getByTestId('identity').textContent()).toContain('Email confirmed in this browser');
    await page.getByLabel('Workspace name').fill('Journey Research');
    expect(await page.getByLabel('URL slug').inputValue()).toBe('journey-research');
    // A reserved slug is refused as taken; the proof survives for a retry.
    await page.getByLabel('URL slug').fill('platform');
    await page.getByLabel('Your username').fill('owner');
    await page.getByLabel('Your password', { exact: true }).fill('correct horse battery staple');
    await page.getByRole('button', { name: 'Create workspace' }).click();
    await page.getByRole('alert').getByText('That URL is already taken').waitFor();

    await page.getByLabel('URL slug').fill(slug);
    await page.getByRole('button', { name: 'Create workspace' }).click();
    await page.waitForURL(`${journey.origin}/t/${slug}`);
    // The retry needed no second message, and the creator is signed in to the new workspace.
    expect(await mailCount(owner)).toBe(1);
    expect(new URL(page.url()).pathname).toBe(`/t/${slug}`);
  } finally {
    await context.close();
  }

  // The administrator signs in again with the passphrase alone.
  const returning = await visitor();
  try {
    await returning.page.goto(`${journey.origin}/t/${slug}/login`);
    await returning.page.getByLabel('Username').fill('owner');
    await returning.page.getByLabel('Password', { exact: true }).fill('correct horse battery staple');
    await returning.page.getByRole('button', { name: 'Sign in', exact: true }).click();
    await returning.page.waitForURL(`${journey.origin}/t/${slug}`);
  } finally {
    await returning.context.close();
  }
}, journeyTimeout);

test('at the owned-workspace limit the page replaces creation with the way back', async () => {
  const ws = await signedUpWorkspace('capped');
  await platform('PUT', '/platform/login-settings', { password_enabled: true, signup_enabled: true, signup_workspace_cap: 1 });
  const second = `capped-second-${run}`;
  const { context, page } = await visitor();
  try {
    await page.goto(`${journey.origin}/signup`);
    await page.getByLabel('Email').fill(ws.owner);
    await page.getByRole('button', { name: 'Continue with email' }).click();
    await page.getByRole('heading', { name: 'Check your email' }).waitFor();
    await expect.poll(() => mailCount(ws.owner)).toBe(2);
    await page.goto(mailedLink(await newestMail(ws.owner)));
    await page.getByRole('link', { name: 'Name your workspace' }).click();
    await page.getByLabel('Workspace name').fill('Capped second');
    await page.getByLabel('URL slug').fill(second);
    await page.getByLabel('Your username').fill(ws.username);
    await page.getByLabel('Your password', { exact: true }).fill(ws.password);
    await page.getByRole('button', { name: 'Create workspace' }).click();

    const alert = page.getByRole('alert');
    await alert.getByText('Workspace limit reached').waitFor();
    expect(await alert.textContent()).toContain('WYRD_AUTH_403_SIGNUP_WORKSPACE_LIMIT');
    // Nothing is left to submit: the way on is an existing workspace.
    expect(await page.locator('form').count()).toBe(0);
    expect(await page.getByRole('button', { name: 'Create workspace' }).count()).toBe(0);
    expect(await page.getByRole('link', { name: 'Sign in to an existing workspace' }).getAttribute('href')).toBe('/');
    const tenants = JSON.stringify(await platform('GET', '/platform/tenants'));
    expect(tenants).toContain(ws.slug);
    expect(tenants).not.toContain(second);
  } finally {
    await context.close();
  }
}, journeyTimeout);

test('a verified address finds its workspaces by link or pasted code', async () => {
  const { slug, name, owner } = await signedUpWorkspace('discovery');
  const { context, page } = await visitor();
  try {
    await page.goto(`${journey.origin}/workspaces`);
    expect(await page.getByRole('heading', { level: 1 }).textContent()).toBe('Find your workspaces');
    // The proof is mailed after the answer, so wait for the message beyond the signup one.
    let before = await mailCount(owner);
    await page.getByLabel('Email').fill(owner);
    await page.getByRole('button', { name: 'Email me a link' }).click();
    await page.getByRole('heading', { name: 'Check your email' }).waitFor();
    await expect.poll(() => mailCount(owner)).toBe(++before);

    // The mailed link lists the workspace as a link to its own sign-in page, then leaves no proof in the address bar.
    const link = mailedLink(await newestMail(owner));
    expect(new URL(link).pathname).toBe('/workspaces/verify');
    await page.goto(link);
    await page.getByRole('heading', { name: 'Your workspaces', exact: true }).waitFor();
    expect(page.url()).toBe(`${journey.origin}/workspaces/verify`);
    const listed = page.getByRole('list', { name: 'Workspaces for this address' }).getByRole('link');
    expect(await listed.count()).toBe(1);
    expect(await listed.getAttribute('href')).toBe(`${journey.origin}/t/${slug}/login`);
    expect(await listed.textContent()).toContain(name);

    // A second request in a new browser, with the code pasted instead of the link opened.
    const pasted = await visitor();
    try {
      await pasted.page.goto(`${journey.origin}/workspaces`);
      await pasted.page.getByLabel('Email').fill(owner);
      await pasted.page.getByRole('button', { name: 'Email me a link' }).click();
      await pasted.page.getByRole('heading', { name: 'Check your email' }).waitFor();
      await expect.poll(() => mailCount(owner)).toBe(before + 1);
      const code = new URL(mailedLink(await newestMail(owner))).hash.slice('#proof='.length);
      await pasted.page.getByLabel('Code from the email').fill(code);
      await pasted.page.getByRole('button', { name: 'Verify' }).click();
      await pasted.page.getByRole('heading', { name: 'Your workspaces', exact: true }).waitFor();
      await pasted.page.getByRole('link', { name }).click();
      await pasted.page.waitForURL(`${journey.origin}/t/${slug}/login`);
    } finally {
      await pasted.context.close();
    }
  } finally {
    await context.close();
  }
}, journeyTimeout);

test('a password account resets its password with a mailed code', async () => {
  const { slug, owner } = await signedUpWorkspace('reset');
  const { context, page } = await visitor();
  try {
    // A password-only workspace offers no company SSO, and points to reset.
    await page.goto(`${journey.origin}/t/${slug}/login`);
    await page.getByLabel('Username').waitFor();
    expect(await page.getByRole('button', { name: 'Sign in with SSO' }).count()).toBe(0);
    await page.getByRole('link', { name: 'Reset your password' }).click();
    await page.waitForURL(`${journey.origin}/t/${slug}/reset`);

    const before = await mailCount(owner);
    await page.getByLabel('Username').fill('owner');
    await page.getByRole('button', { name: 'Email me a code' }).click();
    await page.getByText(`If ${slug} has a password account named owner`).waitFor();
    await expect.poll(() => mailCount(owner)).toBe(before + 1);
    const code = /paste this code there:\s+(\S+)/.exec(await newestMail(owner))?.[1];
    expect(code, 'the message carries the reset code').toBeDefined();

    await page.getByLabel('Reset code').fill(code!);
    await page.getByLabel('New password', { exact: true }).fill('a brand new passphrase for owner');
    await page.getByRole('button', { name: 'Set password' }).click();
    await page.waitForURL(`${journey.origin}/t/${slug}/login?reset=done`);
    await page.getByText('Password set. Sign in with your new password.').waitFor();
    await page.getByLabel('Username').fill('owner');
    await page.getByLabel('Password', { exact: true }).fill('a brand new passphrase for owner');
    await page.getByRole('button', { name: 'Sign in', exact: true }).click();
    await page.waitForURL(`${journey.origin}/t/${slug}`);
  } finally {
    await context.close();
  }
}, journeyTimeout);

test('a signed-in administrator verifies a recovery email from the account page', async () => {
  const ws = await signedUpWorkspace('recovery');
  const slug = ws.slug;
  const { context, page } = await visitor();
  const recovery = `recovery-${run}@example.test`;
  try {
    await signIn(page, slug, ws.username, ws.password);

    await page.goto(`${journey.origin}/t/${slug}/settings/account`);
    const section = page.getByRole('region', { name: 'Recovery email' });
    await section.getByLabel('Email').fill(recovery);
    await section.getByRole('button', { name: 'Send verification link' }).click();
    await section.getByText(`If mail reaches ${recovery}, it holds a link.`).waitFor();

    const link = mailedLink(await newestMail(recovery));
    expect(new URL(link).pathname).toBe(`/t/${slug}/contact/verify`);
    await page.goto(link);
    await page.getByRole('heading', { name: 'Recovery email verified', exact: true }).waitFor();
    expect(page.url()).toBe(`${journey.origin}/t/${slug}/contact/verify`);
    await page.getByText(`Password reset codes for your ${slug} account now go to this address.`).waitFor();
  } finally {
    await context.close();
  }
}, journeyTimeout);

test('a signed-out browser opening a recovery email link is told to sign in', async () => {
  const { slug } = await signedUpWorkspace('signed-out');
  const { context, page } = await visitor();
  try {
    await page.goto(`${journey.origin}/t/${slug}/contact/verify#proof=not-a-proof`);
    await page.getByRole('alert').getByText('You are signed out').waitFor();
    expect(await page.getByRole('link', { name: `Sign in to ${slug}` }).getAttribute('href')).toBe(`/t/${slug}/login`);
  } finally {
    await context.close();
  }
}, journeyTimeout);

test('the administrator invites a teammate who enrolls and approves a CLI sign-in', async () => {
  const ws = await signedUpWorkspace('invite');
  const slug = ws.slug;
  const teammate = `teammate-${run}@example.test`;
  const admin = await visitor();
  try {
    await signIn(admin.page, slug, ws.username, ws.password);
    await admin.page.goto(`${journey.origin}/t/${slug}/settings/people`);
    await admin.page.getByRole('link', { name: 'Invite people' }).click();
    const invite = admin.page.getByRole('form', { name: 'Invite people' });
    await invite.getByLabel('Email addresses').fill(teammate);
    await invite.getByRole('radio').nth(1).check();
    await invite.getByLabel('Applies to').selectOption('all');
    await admin.page.getByRole('button', { name: 'Send invitations' }).click();
    await admin.page.goto(`${journey.origin}/t/${slug}/settings/people?filter=invited`);
    await admin.page.getByRole('region', { name: 'Pending invitations' }).getByText(teammate).waitFor();
  } finally {
    await admin.context.close();
  }

  const { context, page } = await visitor();
  try {
    const link = /(https?:\/\/\S+#invitation=\S+)/.exec(await newestMail(teammate))?.[1] ?? '';
    expect(new URL(link).pathname).toBe(`/t/${slug}/signup`);
    await page.goto(link);
    await page.getByLabel('Username').fill('teammate');
    await page.getByLabel('Password', { exact: true }).fill('a teammate passphrase of length');
    await page.getByRole('button', { name: 'Create account' }).click();
    await page.waitForURL(`${journey.origin}/t/${slug}`);

    // The CLI starts a device sign-in; the freshly enrolled teammate approves it in this browser.
    const device = (await cliForm(page, '/auth/device_authorization', { tenant: slug })) as {
      device_code: string;
      verification_uri_complete: string;
    };
    await page.goto(device.verification_uri_complete);
    await page.getByRole('button', { name: 'Approve and sign in' }).click();
    await page.getByRole('heading', { name: 'Login approved' }).waitFor();

    const token = (await cliForm(page, '/auth/token', {
      grant_type: 'urn:ietf:params:oauth:grant-type:device_code',
      device_code: device.device_code
    })) as { access_token: string };
    const me = await fetch(`${journey.server}/v1/principals/me`, {
      headers: { 'x-wyrd-access-token': `Bearer ${token.access_token}` }
    });
    expect(me.status).toBe(200);
    expect(((await me.json()) as { username: string }).username).toBe('teammate');
  } finally {
    await context.close();
  }
}, journeyTimeout);

let googleEnabled = false;

/** The operator configures, tests and enables the deployment's Google sign-in, once. */
async function enableGoogle(): Promise<void> {
  if (googleEnabled) return;
  const { revision } = (await platform('PUT', '/platform/social/providers/google', { client_id: 'wyrd-google', client_secret: 'google-secret' })) as { revision: number };
  const { authorization_url: authorize } = (await platform('POST', '/platform/social/providers/google/test', { expected_revision: revision })) as { authorization_url: string };
  const signedIn = await fetch(authorize, { redirect: 'manual' });
  const tested = await fetch(signedIn.headers.get('location') ?? '');
  expect(tested.status, 'the provider test passes').toBe(200);
  await platform('PUT', '/platform/social/providers/google/enabled', { enabled: true, expected_revision: revision });
  googleEnabled = true;
}

/** The slugs of every workspace the deployment holds. */
async function workspaceSlugs(): Promise<string[]> {
  const { tenants } = (await platform('GET', '/platform/tenants')) as { tenants: { slug: string }[] };
  return tenants.map((tenant) => tenant.slug);
}

/** Sign `account` in with Google from the signup page, landing on the workspace form. */
async function googleSignup(page: Page, account: GoogleAccount): Promise<void> {
  google.account = account;
  await page.goto(`${journey.origin}/signup`);
  await page.getByRole('button', { name: 'Continue with Google' }).click();
  await page.waitForURL(`${journey.origin}/signup/workspace`);
  await expect.poll(() => page.getByTestId('identity').textContent()).toContain('Signed in with Google');
}

test('a Google proof creates a workspace its creator signs in to with Google', async () => {
  await enableMailAndSignup();
  await enableGoogle();
  const slug = `google-${run}`;
  const account = { sub: `google-${run}`, email: `google-${run}@gmail.test` };
  const { context, page } = await visitor();
  try {
    await googleSignup(page, account);
    // A provider proof sets no password.
    expect(await page.getByLabel('Your username').count()).toBe(0);
    expect(await page.getByLabel('Your password', { exact: true }).count()).toBe(0);
    await page.getByLabel('Workspace name').fill('Google journey');
    await page.getByLabel('URL slug').fill(slug);
    await page.getByRole('button', { name: 'Create workspace' }).click();
    await page.waitForURL(`${journey.origin}/t/${slug}`);
  } finally {
    await context.close();
  }
  expect((await workspaceSlugs()).filter((each) => each === slug)).toHaveLength(1);

  // The creator returns with Google alone.
  const returning = await visitor();
  try {
    google.account = account;
    await returning.page.goto(`${journey.origin}/t/${slug}/login`);
    await returning.page.getByRole('button', { name: 'Continue with Google' }).click();
    await returning.page.waitForURL(`${journey.origin}/t/${slug}`);
  } finally {
    await returning.context.close();
  }
}, journeyTimeout);

test('a Google proof without a verified email confirms one by mail and creates the same attempt', async () => {
  await enableMailAndSignup();
  await enableGoogle();
  const slug = `unverified-${run}`;
  const contact = `unverified-${run}@example.test`;
  const { context, page } = await visitor();
  try {
    await googleSignup(page, { sub: `unverified-${run}`, email: `unverified-${run}@gmail.test`, email_verified: false });
    await page.getByLabel('Workspace name').fill('Unverified journey');
    await page.getByLabel('URL slug').fill(slug);
    await page.getByRole('button', { name: 'Create workspace' }).click();
    await page.getByRole('alert').getByText('Confirm an email address to continue').waitFor();
    expect(await workspaceSlugs()).not.toContain(slug);

    await page.getByLabel('Email to confirm').fill(contact);
    await page.getByRole('button', { name: 'Send a link' }).click();
    await page.getByTestId('check-email').waitFor();
    const code = new URL(mailedLink(await newestMail(contact))).hash.slice('#proof='.length);
    await page.getByLabel('Code from the email').fill(code);
    await page.getByRole('button', { name: 'Verify' }).click();
    await page.getByText('Email confirmed. Create the workspace again.').waitFor();
    // The same attempt and form create the workspace, still with no password.
    expect(await page.getByLabel('URL slug').inputValue()).toBe(slug);
    await page.getByRole('button', { name: 'Create workspace' }).click();
    await page.waitForURL(`${journey.origin}/t/${slug}`);
  } finally {
    await context.close();
  }
  expect((await workspaceSlugs()).filter((each) => each === slug)).toHaveLength(1);
  expect(await mailCount(contact)).toBe(1);
}, journeyTimeout);

/**
 * A stand-in for Cloudflare's Turnstile script: each widget's first token is
 * one the fake Siteverify refuses, and every token after a reset passes.
 */
const STAND_IN_TURNSTILE = `window.turnstile = (() => {
  let issued = 0;
  const widgets = {};
  const issue = (id) => setTimeout(() => {
    const widget = widgets[id];
    if (!widget) return;
    widget.callback((issued++ ? 'pass-' : 'fail-') + issued + '-' + Math.random().toString(36).slice(2));
    widget.element.dataset.issued = String(issued);
  });
  return {
    render(element, options) {
      const id = 'widget-' + Object.keys(widgets).length;
      element.dataset.sitekey = options.sitekey;
      element.dataset.action = options.action;
      widgets[id] = { callback: options.callback, element };
      issue(id);
      return id;
    },
    reset: issue,
    remove(id) { delete widgets[id]; }
  };
})();`;

test('a configured challenge refuses a failed token, and the fresh token creates one workspace', async () => {
  await enableMailAndSignup();
  await platform('PUT', '/platform/login-settings', {
    password_enabled: true,
    signup_enabled: true,
    signup_challenge: { site_key: 'journey-site-key', secret: 'journey-turnstile-secret' }
  });
  const owner = `challenged-${run}@example.test`;
  const slug = `challenged-${run}`;
  const { context, page } = await visitor();
  await context.route('https://challenges.cloudflare.com/turnstile/**', (route) =>
    route.fulfill({ contentType: 'text/javascript', body: STAND_IN_TURNSTILE })
  );
  const asked = siteverified.length;
  try {
    await page.goto(`${journey.origin}/signup`);
    await page.getByLabel('Email').fill(owner);
    await page.getByRole('button', { name: 'Continue with email' }).click();
    await page.getByRole('heading', { name: 'Check your email' }).waitFor();
    await page.goto(mailedLink(await newestMail(owner)));
    await page.getByRole('link', { name: 'Name your workspace' }).click();
    const widget = page.getByTestId('turnstile');
    await expect.poll(() => widget.getAttribute('data-issued')).toBe('1');
    expect(await widget.getAttribute('data-sitekey')).toBe('journey-site-key');
    expect(await widget.getAttribute('data-action')).toBe('signup');
    await page.getByLabel('Workspace name').fill('Challenged journey');
    await page.getByLabel('URL slug').fill(slug);
    await page.getByLabel('Your username').fill('owner');
    await page.getByLabel('Your password', { exact: true }).fill('correct horse battery staple');

    // Rust asks Siteverify about the widget's token and refuses it; the page resets the widget.
    await page.getByRole('button', { name: 'Create workspace' }).click();
    await page.getByRole('alert').getByText('Challenge not passed').waitFor();
    expect(await workspaceSlugs()).not.toContain(slug);
    // The reset issues a fresh token; the same form then creates the workspace.
    await expect.poll(() => widget.getAttribute('data-issued')).toBe('2');
    await page.getByRole('button', { name: 'Create workspace' }).click();
    await page.waitForURL(`${journey.origin}/t/${slug}`);
  } finally {
    await context.close();
  }
  const tokens = siteverified.slice(asked);
  expect(tokens.map(({ token }) => token.split('-')[0])).toEqual(['fail', 'pass']);
  expect(new Set(tokens.map(({ token }) => token)).size).toBe(2);
  expect(tokens.every(({ secret }) => secret === 'journey-turnstile-secret')).toBe(true);
  expect((await workspaceSlugs()).filter((each) => each === slug)).toHaveLength(1);
}, journeyTimeout);
