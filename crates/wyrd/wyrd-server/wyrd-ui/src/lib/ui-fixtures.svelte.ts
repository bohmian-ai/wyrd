/** A deeply reactive copy of `value`, as a direct-Svelte host holds its view document. */
export function reactive<T>(value: T): T {
  const held = $state(value);
  return held;
}
