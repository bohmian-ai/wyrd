// @vitest-environment node
/**
 * Production onboarding journeys in a real Chromium. `identity_ui_e2e.rs`
 * (`onboarding_ui_journey`) starts one bound Wyrd server over repository
 * Postgres and one production `node build` BFF at a public origin, and runs
 * this file with the `WYRD_UI_ONBOARDING` fixture. The repository Mailpit is
 * the deployment's relay: every link and code a page asks for is read from
 * the mailbox it was sent to, as a person would. Nothing here mocks the BFF,
 * the server or the mail.
 *
 * The platform API, called with the operator token, stands in for the
 * operator's console; every visitor step goes through the pages.
 */
import { chromium, type Browser, type BrowserContext, type Page } from 'playwright';
import { afterAll, beforeAll, expect, test } from 'vitest';

type Onboarding = {
  origin: string;
  bff: string;
  server: string;
  operator: string;
  mail: { host: string; port: number; api: string };
};

const journey = JSON.parse(process.env.WYRD_UI_ONBOARDING ?? 'null') as Onboarding;
const journeyTimeout = 120_000;
let chrome: Browser;

beforeAll(async () => {
  // The public origin is the load balancer: Chromium connects every request for it to the BFF.
  chrome = await chromium.launch({ args: [`--host-rules=MAP ${new URL(journey.origin).host} ${new URL(journey.bff).host}`] });
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

test('a verified address finds its workspaces by link or pasted code', async () => {
  const { slug, name, owner } = await signedUpWorkspace('discovery');
  const { context, page } = await visitor();
  try {
    await page.goto(`${journey.origin}/workspaces`);
    expect(await page.getByRole('heading', { level: 1 }).textContent()).toBe('Find your workspaces');
    await page.getByLabel('Email').fill(owner);
    await page.getByRole('button', { name: 'Email me a link' }).click();
    await page.getByRole('heading', { name: 'Check your email' }).waitFor();
    const before = await mailCount(owner);

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
    await admin.page.goto(`${journey.origin}/t/${slug}/settings/access?tab=users`);
    const invite = admin.page.getByRole('form', { name: 'Invite users' });
    await invite.getByLabel('Email addresses').fill(teammate);
    await invite.getByLabel('Role').selectOption({ index: 1 });
    await invite.getByLabel('Scope').selectOption('all');
    await invite.getByRole('button', { name: 'Create invitations' }).click();
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
