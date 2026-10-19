import { localAuthEnabled, loginScenario } from '$lib/server/development';
import type { LayoutServerLoad } from './$types';

export const load: LayoutServerLoad = ({ cookies }) => ({
  mode: cookies.get('wyrd-mode') === 'light' ? ('light' as const) : ('dark' as const),
  localAuth: localAuthEnabled(),
  loginScenario: localAuthEnabled() ? loginScenario(cookies) : undefined
});
