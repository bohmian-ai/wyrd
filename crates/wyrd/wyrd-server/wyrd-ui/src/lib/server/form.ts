import { reject } from './auth/session';

/** One submitted form value, untrimmed; a missing, empty or oversized value is a validation problem. */
export function field(form: FormData, name: string, max = 4096): string {
  const value = form.get(name);
  if (typeof value !== 'string' || !value || value.length > max) reject('validation');
  return value;
}
