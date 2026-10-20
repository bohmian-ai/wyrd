import { dev } from '$app/environment';
import { error } from '@sveltejs/kit';

/** The component gallery exists only in development. */
export function load() {
  if (!dev) error(404);
}
