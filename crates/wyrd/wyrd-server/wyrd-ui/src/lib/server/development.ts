import { dev } from '$app/environment';
import { env } from '$env/dynamic/private';
import type { Cookies } from '@sveltejs/kit';

export const mockCookie = 'wyrd_ui_mock_data';

export function mockDataEnabled(cookies: Pick<Cookies, 'get'>): boolean {
  return dev && (cookies.get(mockCookie) ?? env.WYRD_UI_MOCK_DATA) === 'true';
}

export function localAuthEnabled(): boolean {
  return dev && env.WYRD_UI_LOCAL_AUTH === 'true';
}

export const loginScenarioCookie = 'wyrd_ui_login_scenario';
export function loginScenario(cookies: Pick<Cookies, 'get'>): 'single' | 'multiple' | 'none' {
  const value = dev ? cookies.get(loginScenarioCookie) : undefined;
  return value === 'multiple' || value === 'none' ? value : 'single';
}
export function mockTenantKey(): string {
  return env.WYRD_UI_MOCK_TENANT || 'acme';
}
