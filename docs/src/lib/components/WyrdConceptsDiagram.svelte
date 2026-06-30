<!--
  Brutalist SVG of the Wyrd concept model. Geometry: hard-shadowed boxes, 2.5px
  borders, 3px arrows. All colors flow through the global `.wyrd-diagram` token
  rules in src/styles/wyrd.css, so light/dark both work.
-->
<svg
  class="wyrd-diagram"
  viewBox="0 0 720 620"
  role="img"
  aria-label="Wyrd concepts: Card and Spec declare intent, services produce lineage, policy decisions, audit history, Runs, and Observations"
  xmlns="http://www.w3.org/2000/svg"
>
  <defs>
    <marker
      id="wyrd-arrow"
      viewBox="0 0 10 10"
      refX="9"
      refY="5"
      markerWidth="7"
      markerHeight="7"
      orient="auto-start-reverse"
    >
      <path d="M0,0 L10,5 L0,10 Z" fill="var(--border)" />
    </marker>
  </defs>

  <text class="arrow-label" x="80" y="28">declared contract</text>
  <text class="arrow-label" x="80" y="292">service-owned outcomes</text>
  <text class="arrow-label" x="80" y="506">execution evidence</text>

  <g transform="translate(260, 44)">
    <rect class="node-shadow" x="6" y="6" width="200" height="72" />
    <rect class="node-fill-accent" x="0" y="0" width="200" height="72" />
    <rect class="node-stroke" x="0" y="0" width="200" height="72" />
    <text class="node-text node-text-on-accent" x="100" y="30">CARD</text>
    <text class="node-sub node-sub-on-accent" x="100" y="50">envelope + kind</text>
  </g>

  <g transform="translate(260, 156)">
    <rect class="node-shadow" x="6" y="6" width="200" height="64" />
    <rect class="node-fill" x="0" y="0" width="200" height="64" />
    <rect class="node-stroke" x="0" y="0" width="200" height="64" />
    <text class="node-text" x="100" y="28">SPEC</text>
    <text class="node-sub" x="100" y="48">declared intent + refs</text>
  </g>

  <g transform="translate(260, 252)">
    <rect class="node-shadow" x="6" y="6" width="200" height="64" />
    <rect class="node-fill" x="0" y="0" width="200" height="64" />
    <rect class="node-stroke" x="0" y="0" width="200" height="64" />
    <text class="node-text" x="100" y="28">LINEAGE</text>
    <text class="node-sub" x="100" y="48">derived graph view</text>
  </g>

  <g transform="translate(70, 344)">
    <rect class="node-shadow" x="6" y="6" width="160" height="60" />
    <rect class="node-fill" x="0" y="0" width="160" height="60" />
    <rect class="node-stroke" x="0" y="0" width="160" height="60" />
    <text class="node-text" x="80" y="27">POLICYCARD</text>
    <text class="node-sub" x="80" y="46">rule spec</text>
  </g>

  <g transform="translate(280, 344)">
    <rect class="node-shadow" x="6" y="6" width="160" height="60" />
    <rect class="node-fill" x="0" y="0" width="160" height="60" />
    <rect class="node-stroke" x="0" y="0" width="160" height="60" />
    <text class="node-text" x="80" y="27">POLICY</text>
    <text class="node-sub" x="80" y="46">service decisions</text>
  </g>

  <g transform="translate(490, 344)">
    <rect class="node-shadow" x="6" y="6" width="160" height="60" />
    <rect class="node-fill" x="0" y="0" width="160" height="60" />
    <rect class="node-stroke" x="0" y="0" width="160" height="60" />
    <text class="node-text" x="80" y="27">AUDIT HISTORY</text>
    <text class="node-sub" x="80" y="46">who / what / why</text>
  </g>

  <g transform="translate(490, 424)">
    <rect class="node-shadow" x="6" y="6" width="160" height="60" />
    <rect class="node-fill" x="0" y="0" width="160" height="60" />
    <rect class="node-stroke" x="0" y="0" width="160" height="60" />
    <text class="node-text" x="80" y="27">AUDITCARD</text>
    <text class="node-sub" x="80" y="46">evidence spec</text>
  </g>

  <g transform="translate(170, 540)">
    <rect class="node-shadow" x="6" y="6" width="160" height="60" />
    <rect class="node-fill" x="0" y="0" width="160" height="60" />
    <rect class="node-stroke" x="0" y="0" width="160" height="60" />
    <text class="node-text" x="80" y="27">RUN</text>
    <text class="node-sub" x="80" y="46">one execution</text>
  </g>

  <g transform="translate(390, 540)">
    <rect class="node-shadow" x="6" y="6" width="160" height="60" />
    <rect class="node-fill" x="0" y="0" width="160" height="60" />
    <rect class="node-stroke" x="0" y="0" width="160" height="60" />
    <text class="node-text" x="80" y="27">OBSERVATION</text>
    <text class="node-sub" x="80" y="46">measured fact</text>
  </g>

  <path class="arrow" d="M 360 118 L 360 154" />
  <text class="arrow-label" x="372" y="140">kind selects</text>

  <path class="arrow" d="M 360 222 L 360 250" />
  <text class="arrow-label" x="372" y="240">server derives</text>

  <path class="arrow" d="M 232 374 L 278 374" />
  <text class="arrow-label" x="238" y="366">evaluates</text>

  <path class="arrow" d="M 442 374 L 488 374" />
  <text class="arrow-label" x="448" y="366">records</text>

  <path class="arrow" d="M 570 422 L 570 406" />
  <text class="arrow-label" x="584" y="420">may reference</text>

  <path class="arrow" d="M 332 570 L 388 570" />
  <text class="arrow-label" x="340" y="562">produces</text>
</svg>
