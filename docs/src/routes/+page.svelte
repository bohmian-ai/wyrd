<script lang="ts">
  // Docs home: what Wyrd is, the Declare → Observe → Verify spine, one local
  // start, and a few contextual next tasks. Everything deeper is one click away
  // through the task-first sidebar and search, not a gateway.
  import { base } from '$app/paths';

  const next = [
    {
      href: '/self-hosting/',
      title: 'Run Wyrd for a team',
      body: 'Postgres, object storage, secrets, and readiness for a self-hosted server.'
    },
    {
      href: '/how-to/',
      title: 'Declare and verify components',
      body: 'Author Data, Model, and Prompt Cards and connect what they produce.'
    },
    {
      href: '/api/',
      title: 'Automate with the API',
      body: 'Live OpenAPI, schemas, stable errors, the CLI, and agent workflows.'
    }
  ];
</script>

<svelte:head>
  <title>Wyrd docs</title>
  <meta
    name="description"
    content="Wyrd verifies AI systems against what they declared. Run the server locally, then declare, observe, and verify."
  />
</svelte:head>

<div class="home">
  <h1>Verify AI systems against what they declared.</h1>
  <p class="lead">
    Wyrd gives every model, prompt, dataset, and agent an exact, versioned declaration, connects
    what it does at runtime to that version, and keeps the evidence that makes each result
    attributable.
  </p>
  <div class="start">
    <a class="button primary" href={`${base}/get-started/`}>Run Wyrd locally</a>
    <small>Ends with a running server and a verified client connection</small>
  </div>

  <h2>How it works</h2>
  <ol class="spine">
    <li>
      <span class="shape declare" aria-hidden="true"></span>
      <b>Declare</b>
      <p>
        Describe each component as a Card with an immutable name and version. The server validates
        it and derives lineage from the references between Cards.
      </p>
    </li>
    <li>
      <span class="shape observe" aria-hidden="true"></span>
      <b>Observe</b>
      <p>Runs and Observations record measured behavior against the exact versions that produced it.</p>
    </li>
    <li>
      <span class="shape verify" aria-hidden="true"></span>
      <b>Verify</b>
      <p>
        Verifiers judge observed behavior against declared expectations, and the judgment stays
        tied to <span class="evidence-mark">retained evidence</span>.
      </p>
    </li>
  </ol>
  <p class="status">
    Wyrd is pre-release. Data, Model, and Prompt Cards run end to end today;
    <a href={`${base}/overview/`}>the overview</a> says what is available at each stage.
  </p>

  <h2>Then</h2>
  <ul class="next-tasks">
    {#each next as t (t.href)}
      <li>
        <a href={`${base}${t.href}`}><b>{t.title}</b><span>{t.body}</span></a>
      </li>
    {/each}
  </ul>
</div>
