import { render } from '@testing-library/svelte';
import { expect, test } from 'vitest';
import Histo from './Histo.svelte';

test('renders a reference and current bar per bin', () => {
  const { container } = render(Histo, { props: { reference: [4, 10, 6], current: [2, 8, 12] } });
  expect(container.querySelectorAll('.wy-histo .ref')).toHaveLength(3);
  expect(container.querySelectorAll('.wy-histo .cur')).toHaveLength(3);
});
