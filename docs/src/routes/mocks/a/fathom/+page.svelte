<script lang="ts">
  import Frame from '$lib/mocks/Frame.svelte';
  import Fuji from '$lib/mocks/Fuji.svelte';
  import Plane from '$lib/mocks/Plane.svelte';
</script>

<Frame dir="a" product="fathom">
  <section class="soon-wrap">
    <Plane />
    <div class="soon-in">
      <div class="fuji"><Fuji size={54} /></div>
      <div class="soon-eye">FATHOM · COMING SOON</div>
      <h1>The intelligence layer that <span class="v">verifies</span> how your AI behaves.</h1>
      <p class="lead">
        Wyrd records what every model, prompt, agent, and dataset declared. Fathom reads that
        typed graph and turns runtime behavior into evidence — leakage, eval, drift versus spec,
        and audit. We're building it in the open. Get notified when it ships.
      </p>
      <form class="soon-form" onsubmit={(e) => e.preventDefault()}>
        <input type="email" placeholder="you@team.dev" aria-label="Email" />
        <button class="btn btn-lime" type="submit">▶ Notify me</button>
      </form>
      <div class="soon-feat">
        <span>Leakage detection</span>
        <span>Eval as evidence</span>
        <span>Drift vs spec</span>
        <span>Runtime audit</span>
      </div>
    </div>
  </section>
</Frame>
