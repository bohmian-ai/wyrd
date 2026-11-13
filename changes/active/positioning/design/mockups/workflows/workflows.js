// One shared loop drives all four diagrams. #fig declares beat durations in
// data-beats; an element with data-on="2-6" carries the class `on` during
// those beats and CSS does the rest. The last beat is the finished state.
// The pace is illustrative, not measured performance.
const fig = document.getElementById('fig');
const playButton = document.getElementById('play');
const beats = fig.dataset.beats.split(',').map(Number);
const last = beats.length - 1;
const reduce = matchMedia('(prefers-reduced-motion: reduce)');

let beat = 0;
let timer;
let playing = !reduce.matches;
let visible = true;

const inRange = (spec, n) => {
  const [from, to = from] = spec.split('-').map(Number);
  return n >= from && n <= to;
};

function sync() {
  fig.querySelectorAll('[data-on]').forEach((el) => el.classList.toggle('on', inRange(el.dataset.on, beat)));
  playButton.textContent = playing ? 'Pause' : 'Play';
  const running = playing && visible;
  document.body.classList.toggle('halt', !running);
  clearTimeout(timer);
  if (running) timer = setTimeout(() => { beat = (beat + 1) % beats.length; sync(); }, beats[beat]);
}

// With reduced motion the diagrams rest on the finished state until Play or Replay.
function start(play) {
  playing = play;
  beat = play ? 0 : last;
  sync();
}

playButton.addEventListener('click', () => {
  if (!playing && beat === last) return start(true);
  playing = !playing;
  sync();
});
document.getElementById('replay').addEventListener('click', () => start(true));

// Stop the timer and the pulses while the section is offscreen or the tab is hidden.
let onscreen = true;
const updateVisible = () => { visible = onscreen && !document.hidden; sync(); };
new IntersectionObserver(([entry]) => { onscreen = entry.isIntersecting; updateVisible(); }).observe(fig);
document.addEventListener('visibilitychange', updateVisible);

start(playing);
