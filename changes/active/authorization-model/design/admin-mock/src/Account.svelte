<script>
  import Pill from './Pill.svelte';
</script>
<main class="work" style="max-width:820px">
  <header class="pagehead"><div><h1>Your account</h1><p>How you sign in to gtest.</p></div></header>
  <section class="card">
        <div class="row"><div class="label"><b>Name</b><span>From Google; updates each time you sign in.</span></div><div class="ctl">Steven F.</div></div>
    <div class="row"><div class="label"><b>Email</b><span>Verified by Google.</span></div><div class="ctl">steven@example.test</div></div>
    <div class="row"><div class="label"><b>Signs in with</b><span>Wyrd never links accounts by matching email.</span></div><div class="ctl"><Pill tone="ok">Google</Pill></div></div>
    <div class="row"><div class="label"><b>Roles</b></div><div class="ctl"><span class="role">admin</span></div></div>
  </section>
</main>
