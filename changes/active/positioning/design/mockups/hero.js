// Review mockup driver. Each stage declares its beat durations in data-beats;
// an element with data-on="0-2 5" carries the class `on` during those beats and
// CSS does the rest. Durations are reading pace, not performance.
const $ = (sel, root = document) => root.querySelector(sel);
const $$ = (sel, root = document) => [...root.querySelectorAll(sel)];

const reduce = matchMedia('(prefers-reduced-motion: reduce)');
const OUTCOME = {
  p: ['pass', '✓ passed', '→ proceeds', '✓'],
  f: ['fail', '✕ failed', '‖ held', '✕'],
  q: ['inc', '? inconclusive', '↺ review', '?']
};

// ── generated, purely repetitive content (all synthetic) ──────────
function buildServiceStream() {
  const kinds = ['finance report', 'invoice export', 'support reply', 'account summary', 'payout notice', 'contract draft'];
  const outcomes = 'pppfppqpppfpppqp';
  const rows = [...outcomes].map((o, i) => {
    const [cls, badge, action] = OUTCOME[o];
    return `<div class="row"><span class="mono">r-${String(413 + i).padStart(4, '0')}</span>` +
      `<span class="kind">${kinds[i % kinds.length]}</span>` +
      `<span class="b ${cls}">${badge}</span><span class="act">${action}</span></div>`;
  }).join('');
  $('#a-rows').innerHTML = rows + rows; // doubled so the -50% loop is seamless

  let marks = '';
  for (let i = 0; i < 360; i++) {
    const h = (i * 7 + (i >> 3) * 5) % 23;
    marks += `<i class="m ${h === 4 ? 'f' : h === 13 ? 'q' : 'p'}"></i>`;
  }
  $('#a-field').innerHTML = marks + marks;
}

function fillLane(id, count, rows, outcomes) {
  let html = '';
  for (let i = 0; i < count; i++) {
    const p = ((i * 100) / count + ((i * 37) % 9)) % 100;
    const o = outcomes && outcomes[i % outcomes.length];
    html += `<i class="tok${o ? ' ' + o : ''}" style="--p:${p.toFixed(1)};--row:${i % rows}"` +
      (o ? ` data-g="${OUTCOME[o][3]}"` : '') + '></i>';
  }
  $(id).insertAdjacentHTML('beforeend', html);
}

function buildEvidenceColumns() {
  const requirements = ['recipient', 'destination', 'sharing-policy'];
  const results = { 'r-0412': '✓✓✕', 'r-0413': '✓✓✓', 'r-0414': '✓✓?' };
  const records = Object.entries(results).flatMap(([request, marks]) =>
    requirements.map((requirement, i) => [request, requirement, 'report-sharing', 'v3', marks[i]]));
  const headers = ['request', 'requirement', 'verifier', 'ver', ''];
  const selected = 2; // r-0412 · sharing-policy · failed

  $('#c-cols').innerHTML = headers.map((header, c) => {
    const seen = new Set();
    const cells = records.map((record, r) => {
      const value = record[c];
      const repeated = seen.has(value);
      seen.add(value);
      // The selected record reads back from the first occurrence of each of its values.
      const first = records.findIndex((other) => other[c] === records[selected][c]);
      return `<span class="cell${repeated ? ' rep' : ''}${r === first ? ' sel' : ''}" style="--i:${r}">${value}</span>`;
    }).join('');
    return `<div class="col"><span class="ch">${header}</span>${cells}</div>`;
  }).join('');
}

// ── beat driver ───────────────────────────────────────────────────
let stage, beats, beat = 0, timer, playing = false, visible = true;

const inRange = (spec, n) => spec.split(' ').some((range) => {
  const [from, to = from] = range.split('-').map(Number);
  return n >= from && n <= to;
});

function sync() {
  $$('[data-on]', stage).forEach((el) => el.classList.toggle('on', inRange(el.dataset.on, beat)));
  $$('#pips i').forEach((pip, i) => pip.classList.toggle('cur', i === beat));
  $('#play').textContent = playing ? 'Pause' : 'Play';
  const running = playing && visible;
  document.body.classList.toggle('halt', !running);
  clearTimeout(timer);
  // shortcut: resuming restarts the current beat; track remaining time if precise seeking is added.
  if (running) timer = setTimeout(() => { beat = (beat + 1) % beats.length; sync(); }, beats[beat]);
}

// With reduced motion the stage opens on a complete static rendering and only
// steps (without transitions) once the reader presses Play or Replay.
function reset(play) {
  playing = play;
  stage.classList.toggle('static', !play && reduce.matches);
  beat = stage.classList.contains('static') ? Number(stage.dataset.static) : 0;
  sync();
}

function select(id) {
  $$('.stage').forEach((s) => { s.hidden = s.id !== `stage-${id}`; });
  stage = $(`#stage-${id}`);
  beats = stage.dataset.beats.split(',').map(Number);
  $('#pips').innerHTML = beats.map(() => '<i></i>').join('');
  history.replaceState(null, '', `#${id}`);
  reset(!reduce.matches);
}

// ── wiring ────────────────────────────────────────────────────────
buildServiceStream();
fillLane('#b-in', 18, 3);
fillLane('#b-as', 12, 3);
fillLane('#b-jd', 16, 3);
fillLane('#b-go', 12, 2, 'p');
fillLane('#b-hold', 5, 2, 'fqf');
buildEvidenceColumns();

$$('input[name="opt"]').forEach((radio) => radio.addEventListener('change', () => select(radio.value)));
$('#play').addEventListener('click', () => {
  if (stage.classList.contains('static')) return reset(true);
  playing = !playing;
  sync();
});
$('#replay').addEventListener('click', () => reset(true));

const themeButton = $('#theme');
const paintTheme = () => { themeButton.textContent = `Theme: ${document.documentElement.dataset.theme}`; };
themeButton.addEventListener('click', () => {
  const root = document.documentElement;
  root.dataset.theme = root.dataset.theme === 'dark' ? 'light' : 'dark';
  paintTheme();
});
paintTheme();

// Stop timers and CSS loops while the hero is offscreen or the tab is hidden.
let onscreen = true;
const updateVisible = () => { visible = onscreen && !document.hidden; sync(); };
new IntersectionObserver(([entry]) => { onscreen = entry.isIntersecting; updateVisible(); }).observe($('#stages'));
document.addEventListener('visibilitychange', updateVisible);

const initial = 'abc'.includes(location.hash.slice(1)) && location.hash.length === 2 ? location.hash.slice(1) : 'a';
$(`input[name="opt"][value="${initial}"]`).checked = true;
select(initial);
