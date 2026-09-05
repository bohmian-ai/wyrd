import { readFileSync } from 'node:fs';
import { fireEvent, render } from '@testing-library/svelte';
import { expect, test } from 'vitest';
import Line from './Line.svelte';
import ModeProvider from '../ModeProvider.svelte';

const stamps = (labels: string[]) => labels.map((label, i) => ({
  label, at: `2026-09-04T17:${i}0:00Z`
}));

const series = [
  { label: 'p50', points: [120, 118, 131] },
  { label: 'p95', points: [310, 402, 388] },
  { label: 'p99', points: [520, 610, 588] }
];

test('draws one polyline per series', () => {
  const { container } = render(Line, { props: { series, labels: stamps(['a', '', 'c']) } });
  expect(container.querySelectorAll('.plot > svg > polyline')).toHaveLength(3);
});

test('renders only non-empty x labels', () => {
  const { container } = render(Line, { props: { series: [series[0]], labels: stamps(['a', '', 'c']) } });
  expect(container.querySelectorAll('.wy-line .xl')).toHaveLength(2);
});

test('every series carries a distinct dash and marker, not just a distinct hue', () => {
  const { container } = render(Line, { props: { series } });
  const lines = [...container.querySelectorAll('.plot > svg > polyline')];
  const dashes = lines.map((l) => l.getAttribute('stroke-dasharray') ?? 'solid');
  expect(new Set(dashes).size).toBe(3);

  // Markers: one path per point per series, and the shapes differ between series.
  const shapes = [...container.querySelectorAll('.plot > svg > path.node')].map((p) =>
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

test('the tooltip is reachable by focus, not by pointer alone, and opens on the latest point', async () => {
  const { container } = render(Line, { props: { series, labels: stamps(['a', 'b', 'c']), unit: 'ms' } });
  expect(container.querySelector('.tip'), 'at rest the plot carries no tooltip').toBeNull();

  await fireEvent.focus(container.querySelector('.plot > svg') as SVGSVGElement);
  expect(container.querySelector('.tip .at')?.textContent).toBe('c');
  expect([...container.querySelectorAll('.tip .sv')].map((v) => v.textContent)).toEqual([
    '131ms',
    '388ms',
    '588ms'
  ]);
});

test('arrow keys move the tooltip and Home jumps to the start of the range', async () => {
  const { container } = render(Line, { props: { series, labels: stamps(['a', 'b', 'c']) } });
  const plot = container.querySelector('.plot > svg') as SVGSVGElement;
  await fireEvent.focus(plot);
  await fireEvent.keyDown(plot, { key: 'ArrowLeft' });
  expect(container.querySelector('.tip .at')?.textContent).toBe('b');
  await fireEvent.keyDown(plot, { key: 'Home' });
  expect(container.querySelector('.tip .at')?.textContent).toBe('a');
  expect(container.querySelector('.tip .sv')?.textContent).toBe('120');
});

test('the tooltip anchors inward at the ends of the range so it is not clipped', async () => {
  const { container } = render(Line, { props: { series, labels: stamps(['a', 'b', 'c']) } });
  const plot = container.querySelector('.plot > svg') as SVGSVGElement;
  await fireEvent.focus(plot);
  expect(container.querySelector('.tip')?.getAttribute('style')).toContain('translate(calc(-100% + 6px)');
  await fireEvent.keyDown(plot, { key: 'Home' });
  expect(container.querySelector('.tip')?.getAttribute('style')).toContain('translate(-6px');
});

test('exposed time-axis labels and keyboard tooltip carry machine-readable instants', async () => {
  const manifest = JSON.parse(readFileSync('brand/components.json', 'utf8'));
  expect(manifest.components.Line.props.labels).toBe('Array<{ label: string, at: string }>?');
  const labels = [
    { label: '17:00', at: '2026-09-04T17:00:00Z' },
    { label: '', at: '2026-09-04T17:10:00Z' },
    { label: '17:20', at: '2026-09-04T17:20:00Z' }
  ];
  const { container } = render(Line, { props: { series, labels } });
  const times = [...container.querySelectorAll('time')];
  expect(times.map((t) => [t.textContent, t.getAttribute('datetime')])).toEqual([
    ['17:00', labels[0].at], ['17:20', labels[2].at]
  ]);
  expect(times.every((t) => t.namespaceURI === 'http://www.w3.org/1999/xhtml')).toBe(true);
  const plot = container.querySelector('.plot > svg') as SVGSVGElement;
  await fireEvent.focus(plot);
  expect(container.querySelector('.tip time')).toHaveAttribute('datetime', labels[2].at);
  await fireEvent.keyDown(plot, { key: 'ArrowLeft' });
  expect(container.querySelector('.tip time')).toHaveAttribute('datetime', labels[1].at);
  expect(container.querySelector('.tip time')?.textContent).toBe('point 2 of 3');
});

test('four series have four distinct dash and marker identities in plot and legend', () => {
  const four = [...series, { label: 'p99.9', points: [700, 800, 900] }];
  const { container } = render(Line, { props: { series: four } });
  for (const selector of ['.plot > svg > polyline', '.legend .sample line']) {
    const lines = [...container.querySelectorAll(selector)];
    expect(lines).toHaveLength(4);
    expect(new Set(lines.map((l) => l.getAttribute('stroke-dasharray') ?? 'solid')).size).toBe(4);
  }
  const plotShapes = [...container.querySelectorAll('.plot > svg > path.node')]
    .filter((_, i) => i % 3 === 0)
    .map((p) => (p.getAttribute('d') ?? '').replace(/[\d.-]+/g, ''));
  const legendShapes = [...container.querySelectorAll('.legend path.node')]
    .map((p) => (p.getAttribute('d') ?? '').replace(/[\d.-]+/g, ''));
  expect(new Set(plotShapes).size).toBe(4);
  expect(legendShapes).toEqual(plotShapes);
});

test('rejects a fifth series rather than repeating a non-colour identity', () => {
  const five = Array.from({ length: 5 }, (_, i) => ({ label: `series ${i}`, points: [1, 2, 3] }));
  expect(() => render(Line, { props: { series: five } })).toThrow('Line supports at most four series');
});
