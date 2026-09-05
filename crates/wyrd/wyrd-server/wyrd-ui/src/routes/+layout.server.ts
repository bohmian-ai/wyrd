import { loginScenario } from '$lib/server/development';
import { dev } from '$app/environment';
import type { LayoutServerLoad } from './$types';

export const load: LayoutServerLoad = ({ locals, cookies }) => ({
  development: dev,
  loginScenario: dev ? loginScenario(cookies) : undefined,
  mockData: locals.mockData,
  devCsrf: dev ? locals.session?.csrf : undefined
});
