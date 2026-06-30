<!-- Shared placeholder article content — identical across directions for a fair
     comparison. Real pages author this as .svx/.md; this is structural mock copy. -->
<article class="prose">
  <div class="crumbs">Wyrd <span class="sep">/</span> Server &amp; auth <span class="sep">/</span> Register cards</div>
  <h1>Register cards</h1>
  <p class="deck">
    Registration is how a local Card becomes a durable, versioned record in the
    registry. This page covers the contract, the CLI and Python surfaces, and how
    idempotency keeps repeated registration safe.
  </p>

  <div class="aside">
    <div class="at">◆ Doctrine</div>
    <p>
      Registration belongs to the registry, not the Card. A Card is a local holder;
      <code>client.cards.register(card)</code> is the durable side effect.
    </p>
  </div>

  <h2 id="overview">Overview</h2>
  <p>
    Every registered artifact uses the shared envelope — <code>apiVersion</code>,
    <code>metadata</code>, <code>kind</code>, <code>spec</code>, server-derived
    <code>relationships</code>, and server-managed <code>status</code>. Links between
    cards are expressed as a <a class="lk" href="#top">CardRef</a>, and the server
    derives the relationship graph from those refs.
  </p>
  <ul>
    <li>Cards are typed, versioned, and addressable by name + version.</li>
    <li>Relationships are server-derived — you never author a parallel lineage graph.</li>
    <li>Registration is idempotent on the content hash of the spec.</li>
  </ul>

  <h2 id="register">Register a card</h2>
  <p>Pick the surface that fits your workflow. Both resolve to the same registry contract.</p>

  <div class="tabs">
    <span class="tab on">CLI</span>
    <span class="tab">Python</span>
    <span class="tab">HTTP</span>
  </div>
  <div class="code">
    <div class="ch"><span class="lang">bash</span><span class="copy">copy</span></div>
    <pre><span class="cm"># register every card declared under ./cards</span>
<span class="kw">wyrd</span> cards register <span class="st">./cards/model.yaml</span> --space <span class="st">prod</span>
<span class="cm"># → registered model/churn-classifier@1.4.0 (control)</span></pre>
  </div>

  <h3 id="cli">From the CLI</h3>
  <p>
    The CLI prints the resolved <code>CardRef</code> and the derived relationships so
    an agent can confirm the write landed before continuing.
  </p>

  <h3 id="python">From Python</h3>
  <div class="code">
    <div class="ch"><span class="lang">python</span><span class="copy">copy</span></div>
    <pre><span class="kw">from</span> wyrd <span class="kw">import</span> Client, ModelCard

client <span class="kw">=</span> Client(<span class="st">"https://registry.local"</span>)
card <span class="kw">=</span> ModelCard(name<span class="kw">=</span><span class="st">"churn-classifier"</span>, version<span class="kw">=</span><span class="st">"1.4.0"</span>)
ref <span class="kw">=</span> client.cards.<span class="fn">register</span>(card)  <span class="cm"># idempotent</span></pre>
  </div>

  <h2 id="idempotency">Idempotency</h2>
  <p>
    Registering the same spec twice is a no-op that returns the existing
    <code>CardRef</code>. A changed spec mints a new version; the prior version stays
    addressable. This is what makes registration safe to retry from a flaky agent loop.
  </p>

  <h2 id="errors">Errors</h2>
  <p>
    Conflicts surface as stable Wyrd error codes — see the
    <a class="lk" href="#top">error reference</a>. A literal agent can branch on the
    code without parsing prose.
  </p>

  <div class="pag">
    <a class="prev" href="#top"><div class="dir">← Prev</div><div class="lbl">Authentication</div></a>
    <a class="next" href="#top"><div class="dir">Next →</div><div class="lbl">Schema reference</div></a>
  </div>
</article>
