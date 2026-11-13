// Runs before ../simple/hero.js, which owns the beat timer, the case switch,
// pause/replay, and the offscreen/hidden pause. This file only fills in the
// parts every variant shares and wires the direction selector.
const figure = document.getElementById('fig');

// Syntax tokens for the static code lines, in the docs site's code colours:
// keyword, string, function (and decorator), punctuation, Wyrd name.
const token = (cls) => (text) => `<tspan class="${cls}">${text}</tspan>`;
const [k, s, f, p, w] = ['k', 's', 'f', 'p', 'w'].map(token);

// The handler, identical in all three variants: a twelve-line integration
// sketch in conventional Python layout (one blank line after the import, two
// before the decorated handler). Line n sits at baseline 38 + 11(n - 1);
// lines 2, 4 and 5 are blank and keep their numbers. Indents are four and
// eight 9.5px mono advances (22.8 and 45.6 units). Line 9 is the Wyrd call;
// its mid-height (y123) is where each variant's link attaches.
const HANDLER = `
  <rect class="frame" x="10" y="4" width="380" height="166" rx="2" />
  <text class="cap mid" x="200" y="19">YOUR FINANCE API</text>
  <path class="rule" d="M10 26 H390 M36 32 V164" />
  <rect class="stripe" x="37" y="118" width="2" height="11" />
  <rect class="band" x="37" y="118" width="352" height="11" data-on="0-3" />
  <rect class="band" x="37" y="140" width="352" height="11" data-on="4" data-case="pass" />
  <rect class="band" x="37" y="151" width="352" height="11" data-on="4" data-case="fail" />
  <g class="ln">${Array.from({ length: 12 }, (_, i) => `<text x="30" y="${38 + 11 * i}">${i + 1}</text>`).join('')}</g>
  <g class="code">
    <text x="44" y="38">${k('from')} wyrd ${k('import')} ${w('WyrdState')}</text>
    <text x="44" y="60">state ${p('=')} ${w('WyrdState')}${p('.')}${f('declare')}${p('(')}service${p(')')}</text>
    <text x="44" y="93">${f('@api.post')}${p('(')}${s('"/reports/send"')}${p(')')}</text>
    <text x="44" y="104">${k('def')} ${f('send_report')}${p('(')}report${p(':')} Report${p('):')}</text>
    <text x="66.8" y="115">${k('with')} state${p('.')}${f('run')}${p('()')} ${k('as')} run${p(':')}</text>
    <text x="89.6" y="126">r ${p('=')} run${p('.')}observe${p('.')}${w('verify')}${p('(')}name${p(',')} ${p('**')}data${p(')')}</text>
    <text x="66.8" y="137">${k('if')} r${p('.')}verdict ${p('==')} ${s('"passed"')}${p(':')}</text>
    <text x="89.6" y="148">${k('return')} ${f('release')}${p('(')}report${p(')')}</text>
    <text x="66.8" y="159">${k('return')} ${f('hold')}${p('(')}report${p(')')}</text>
  </g>
  <rect class="fill call" x="338" y="120" width="26" height="6" rx="2" data-on="0-4" />
  <rect class="slot" x="338" y="120" width="26" height="6" rx="2" />
  <text class="mark" x="370" y="126" data-on="4" data-case="pass">✓</text>
  <text class="mark" x="370" y="126" data-on="4" data-case="fail">✕</text>
  <text class="mark out end" x="380" y="148" data-on="4" data-case="pass">→ Release</text>
  <text class="mark out end" x="380" y="159" data-on="4" data-case="fail">‖ Hold</text>`;

// The three checks as rows, for the two horizontal variants.
const row = (y, kind, name, fills, marks) => `
  <text class="name" x="12" y="${y}">${name}</text>
  <text class="cap" x="12" y="${y + 13}">${kind}</text>
  ${fills(`x="150" y="${y - 9}" width="100" height="8" rx="2"`)}
  <rect class="slot" x="150" y="${y - 9}" width="100" height="8" rx="2" />
  ${marks(`x="258" y="${y - 1}"`)}`;
const assertFill = (box) => `<rect class="fill" ${box} data-on="1-4" />`;
const assertMark = (at) => `<text class="mark" ${at} data-on="1-4">✓</text>`;
const CHECK_ROWS = `
  <rect class="frame wyrd" x="0" y="0" width="280" height="146" rx="2" />
  <text class="cap w" x="12" y="17">WYRD · REAL-TIME VERIFICATION</text>
  <path class="rule w" d="M0 24 H280" />
  ${row(48, 'ASSERT', 'Recipient authorized', assertFill, assertMark)}
  ${row(84, 'AGENT', 'Sharing policy · PDF',
    (box) => `<rect class="fill agent" ${box} data-on="1-4" data-case="pass" />
              <rect class="fill agent bad" ${box} data-on="1-4" data-case="fail" />`,
    (at) => `<text class="mark" ${at} data-on="3-4" data-case="pass">✓</text>
             <text class="mark" ${at} data-on="3-4" data-case="fail">✕</text>`)}
  ${row(120, 'ASSERT', 'Destination private', assertFill, assertMark)}`;

figure.querySelectorAll('.handler').forEach((g) => { g.innerHTML = HANDLER; });
figure.querySelectorAll('.checks').forEach((g) => { g.innerHTML = CHECK_ROWS; });

// Selecting a direction shows that variant and restarts the loop from its first beat.
function selectDirection(dir) {
  figure.dataset.dir = dir;
  document.querySelector(`input[name="dir"][value="${dir}"]`).checked = true;
  history.replaceState(null, '', `#${dir}`);
}
document.querySelectorAll('input[name="dir"]').forEach((radio) =>
  radio.addEventListener('change', () => {
    selectDirection(radio.value);
    if (!matchMedia('(prefers-reduced-motion: reduce)').matches) document.getElementById('replay').click();
  })
);
const fromHash = location.hash.slice(1);
if (['a', 'b', 'c'].includes(fromHash)) selectDirection(fromHash);
