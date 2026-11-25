import type { LayoutServerLoad } from './$types';

// The hook resolved this session from Rust on this very request; only its safe metadata reaches the page.
export const load: LayoutServerLoad = ({ locals }) => ({ session: locals.platformSession!.view });
