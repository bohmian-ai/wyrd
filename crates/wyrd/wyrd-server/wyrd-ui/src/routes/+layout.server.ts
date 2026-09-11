import { loginScenario } from '$lib/server/development';
import { dev } from '$app/environment';
import type { LayoutServerLoad } from './$types';

export const load: LayoutServerLoad = ({ locals, cookies }) => ({
  mode: cookies.get('wyrd-mode') === 'light' ? ('light' as const) : ('dark' as const),
  navCollapsed: cookies.get('wyrd-nav') === 'collapsed',
  development: dev,
  loginScenario: dev ? loginScenario(cookies) : undefined,
  mockData: locals.mockData,
  devCsrf: dev ? locals.session?.csrf : undefined
});
