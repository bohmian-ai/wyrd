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
  chrome = await chromium.launch();
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

/**
 * A fresh browser profile at the public origin. The origin is the load
 * balancer: every request Chromium sends to it reaches the BFF.
 */
async function visitor(): Promise<{ context: BrowserContext; page: Page }> {
  const context = await chrome.newContext();
  await context.route(`${journey.origin}/**`, async (route) => {
    const url = new URL(route.request().url());
    await route.fulfill({ response: await route.fetch({ url: journey.bff + url.pathname + url.search }) });
  });
  const page = await context.newPage();
  page.setDefaultTimeout(10_000);
  return { context, page };
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

    await enableMailAndSignup();
    await page.goto(`${journey.origin}/`);
    expect(await page.getByRole('link', { name: 'Create a workspace' }).getAttribute('href')).toBe('/signup');
    expect(await page.getByRole('link', { name: 'Find my workspaces' }).getAttribute('href')).toBe('/workspaces');
  } finally {
    await context.close();
  }
}, journeyTimeout);
