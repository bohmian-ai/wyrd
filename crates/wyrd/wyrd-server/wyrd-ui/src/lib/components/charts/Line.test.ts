import { fireEvent, render } from '@testing-library/svelte';
import { expect, test } from 'vitest';
import Line from './Line.svelte';
import ModeProvider from '../ModeProvider.svelte';

const series = [
  { label: 'p50', points: [120, 118, 131] },
  { label: 'p95', points: [310, 402, 388] },
  { label: 'p99', points: [520, 610, 588] }
];

test('draws one polyline per series', () => {
  const { container } = render(Line, { props: { series, labels: ['a', '', 'c'] } });
  expect(container.querySelectorAll('.wy-line svg > polyline')).toHaveLength(3);
});

test('renders only non-empty x labels', () => {
  const { container } = render(Line, { props: { series: [series[0]], labels: ['a', '', 'c'] } });
  expect(container.querySelectorAll('.wy-line .xl')).toHaveLength(2);
});

test('every series carries a distinct dash and marker, not just a distinct hue', () => {
  const { container } = render(Line, { props: { series } });
  const lines = [...container.querySelectorAll('.wy-line svg > polyline')];
  const dashes = lines.map((l) => l.getAttribute('stroke-dasharray') ?? 'solid');
  expect(new Set(dashes).size).toBe(3);

  // Markers: one path per point per series, and the shapes differ between series.
  const shapes = [...container.querySelectorAll('.wy-line svg > path.node')].map((p) =>
    (p.getAttribute('d') ?? '').replace(/[\d.-]+/g, '')
  );
  expect(new Set(shapes).size).toBe(3);
});

test('the legend mirrors the dash and the marker, not a colour swatch', () => {
  const { container } = render(Line, { props: { series } });
  const samples = [...container.querySelectorAll('.legend .sample')];
  expect(samples).toHaveLength(3);
  expect(new Set(samples.map((s) => s.querySelector('line')?.getAttribute('stroke-dasharray') ?? 'solid')).size).toBe(3);
  expect(container.querySelector('.legend')?.textContent).toContain('p95');
});

test('a threshold is a labeled rule with a positional tick', () => {
  const { container } = render(Line, {
    props: { series, threshold: { value: 750, label: 'SLO 750ms' } }
  });
  const rule = container.querySelector('.thr');
  const tick = container.querySelector('.tick');
  expect(rule).toHaveAttribute('stroke-dasharray', '5 3');
  // Same y as the rule: the tick is the positional read that survives colour blindness.
  expect(tick?.getAttribute('y1')).toBe(rule?.getAttribute('y1'));
  expect(container.querySelector('.thrl')?.textContent).toBe('SLO 750ms');
});

test('light and dark render the same structure and the same non-colour separation', () => {
  const strip = (html: string) => html.replace(/\sstyle="[^"]*"/g, '');
  const light = render(ModeProvider, { props: { mode: 'light' } });
  const dark = render(ModeProvider, { props: { mode: 'dark' } });
  // The modes differ only in tokens, so with the token-bearing style attributes removed
  // the two trees must be identical — geometry, dashes, markers, labels and legend.
  const draw = (target: HTMLElement) => {
    render(Line, { props: { series, threshold: { value: 750, label: 'SLO' } }, target });
    const drawn = target.querySelector('.wy-line');
    expect(drawn, 'the chart must render beneath the mode root').not.toBeNull();
    return strip((drawn as HTMLElement).innerHTML);
  };
  expect(draw(light.container.querySelector('.wy-root') as HTMLElement)).toBe(
    draw(dark.container.querySelector('.wy-root') as HTMLElement)
  );
});

test('the readout is populated without a pointer and reports the most recent point', () => {
  const { container } = render(Line, { props: { series, labels: ['a', 'b', 'c'], unit: 'ms' } });
  // Never hover-gated: a keyboard or touch reader gets the latest values on first paint.
  expect(container.querySelector('.readout .at')?.textContent).toBe('c');
  expect([...container.querySelectorAll('.readout .sv')].map((v) => v.textContent)).toEqual([
    '131ms',
    '388ms',
    '588ms'
  ]);
});

test('arrow keys step the readout and Home jumps to the start of the range', async () => {
  const { container } = render(Line, { props: { series, labels: ['a', 'b', 'c'] } });
  const plot = container.querySelector('.wy-line > svg') as SVGSVGElement;
  await fireEvent.keyDown(plot, { key: 'ArrowLeft' });
  expect(container.querySelector('.readout .at')?.textContent).toBe('b');
  await fireEvent.keyDown(plot, { key: 'Home' });
  expect(container.querySelector('.readout .at')?.textContent).toBe('a');
  expect(container.querySelector('.readout .sv')?.textContent).toBe('120');
});
