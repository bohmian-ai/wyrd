<script lang="ts">
  import Frame from '$lib/mocks/Frame.svelte';
  import Sidebar from '$lib/mocks/Sidebar.svelte';
  import Toc from '$lib/mocks/Toc.svelte';

  const toc: [string, string, boolean][] = [
    ['Card envelope', 'envelope', false],
    ['CardRef', 'cardref', false],
    ['Registry endpoints', 'endpoints', false],
    ['Error codes', 'errors', false]
  ];
</script>

<Frame dir="a" product="wyrd">
  <div class="shell">
    <aside class="side"><Sidebar active="schemas" /></aside>
    <main class="main">
      <article class="prose" style="max-width:none">
        <div class="crumbs">Wyrd <span class="sep">/</span> Reference <span class="sep">/</span> Schema reference</div>
        <h1>Schema reference</h1>
        <p class="deck">
          The literal contract every registered Card carries, the reference shape that links
          them, the registry endpoints, and the stable error codes an agent can branch on.
        </p>

        <div class="anchor-rail">
          <a href="#envelope">apiVersion</a>
          <a href="#envelope">metadata</a>
          <a href="#envelope">kind</a>
          <a href="#envelope">spec</a>
          <a href="#envelope">relationships</a>
          <a href="#envelope">status</a>
          <a href="#cardref">CardRef</a>
          <a href="#errors">errors</a>
        </div>

        <h2 class="ref-h2" id="envelope">Card envelope</h2>
        <table class="ref">
          <thead>
            <tr><th>Field</th><th>Type</th><th>Origin</th><th>Notes</th></tr>
          </thead>
          <tbody>
            <tr><td class="nm">apiVersion</td><td class="ty">string</td><td>author</td><td>Always <code>wyrd/v1</code>.</td></tr>
            <tr><td class="nm">metadata</td><td class="ty">Metadata</td><td>author</td><td>name, version, space, labels.</td></tr>
            <tr><td class="nm">kind</td><td class="ty">string</td><td>author</td><td>Card kind — Model, Prompt, Agent, Service, Dataset, Artifact.</td></tr>
            <tr><td class="nm">spec</td><td class="ty">object</td><td>author</td><td>Kind-specific body. Content-hashed for idempotency.</td></tr>
            <tr><td class="nm">relationships</td><td class="ty">Edge[]</td><td><span class="st warn">server</span></td><td>Derived from refs. Never authored.</td></tr>
            <tr><td class="nm">status</td><td class="ty">Status</td><td><span class="st warn">server</span></td><td>Managed lifecycle state.</td></tr>
          </tbody>
        </table>

        <h2 class="ref-h2" id="cardref">CardRef</h2>
        <dl class="schema">
          <dt>kind</dt><dd>string — target Card kind</dd>
          <dt>name</dt><dd>string — target Card name</dd>
          <dt>version</dt><dd>string — single version field</dd>
          <dt>space</dt><dd>string? — optional namespace</dd>
          <dt>uid</dt><dd>string? — optional pinned identity</dd>
        </dl>

        <h2 class="ref-h2" id="endpoints">Registry endpoints</h2>
        <table class="ref">
          <thead>
            <tr><th>Method</th><th>Path</th><th>Returns</th></tr>
          </thead>
          <tbody>
            <tr><td><span class="pill post">POST</span></td><td>/v1/cards</td><td>CardRef</td></tr>
            <tr><td><span class="pill get">GET</span></td><td>/v1/cards/&#123;kind&#125;/&#123;name&#125;</td><td>Card</td></tr>
            <tr><td><span class="pill get">GET</span></td><td>/v1/cards/&#123;kind&#125;/&#123;name&#125;/relationships</td><td>Edge[]</td></tr>
          </tbody>
        </table>

        <h2 class="ref-h2" id="errors">Error codes</h2>
        <table class="ref">
          <thead>
            <tr><th>Code</th><th>HTTP</th><th>Meaning</th></tr>
          </thead>
          <tbody>
            <tr><td class="nm">WYRD_CARD_CONFLICT</td><td>409</td><td>Name + version already bound to a different spec hash.</td></tr>
            <tr><td class="nm">WYRD_REF_UNRESOLVED</td><td>422</td><td>A CardRef points at a card that is not registered.</td></tr>
            <tr><td class="nm">WYRD_SPEC_INVALID</td><td>400</td><td>Spec failed kind schema validation.</td></tr>
            <tr><td class="nm">WYRD_UNAUTHORIZED</td><td>401</td><td>Missing or invalid token for the target space.</td></tr>
          </tbody>
        </table>
      </article>
    </main>
    <aside><Toc items={toc} active="envelope" /></aside>
  </div>
</Frame>
