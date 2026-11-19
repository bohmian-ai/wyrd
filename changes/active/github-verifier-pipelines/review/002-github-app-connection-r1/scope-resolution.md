# Human-directed scope resolution

After the immutable revision 4 review, the user directed the conventional
GitHub App flow and removal of the Enterprise Server compatibility decision.
The user then requested the specification revision and a decision-complete
remediation packet. Approved revision 5 records GitHub.com’s documented App
installation and OAuth flow with development protocol mocks.

The original verdict and findings remain evidence for
`3dd1555a0f40117ee512ecaaff1f5b51845ef3b6` under revision 4. The current handoff
is [TASK-002-R1](002-R1-conventional-github-app.md) under revision 5. Source
corrections remain required; this resolution grants no implementation PASS.

| Original finding | Current disposition |
|---|---|
| FIND-002-1 | Remediate credential handoff at the existing slot revision boundary. |
| FIND-002-2 | Remediate saved association and OAuth transaction identity checks. Cross-Enterprise-host support is outside revision 5. |
| FIND-002-3 through FIND-002-6 | Remediate typed signing-key validation, OAuth media negotiation, suspension observations and removal fences. |
| FIND-002-7 | Superseded by GitHub.com scope. Do not add private Enterprise Server transport support. |
| FIND-002-8 through FIND-002-13 | Remediate cache and guide correctness, batching HMAC reuse, rustdoc, fixture and source placement with those changes. |
| FIND-002-14 | Historical evidence satisfied by the independent source traces already in the review reports; attach those references without commissioning another diagnosis. |
| FIND-002-15 | Record missing exact commands during the required focused runs, with no separate broad suite. |
| FIND-002-16 | Spec decision resolved through GitHub.com scope. Use the provider’s documented protocol with existing state and PKCE. |

The packet carries production provider validation and documentation updates
through the existing configuration owner. Ordinary GitHub installation
confirmation follows GitHub’s own guidance. No extra confirmation screen or
provider capability framework is introduced. [GitHub setup guidance](https://docs.github.com/en/apps/creating-github-apps/registering-a-github-app/about-the-setup-url)

## Ponytail calibration after the user challenged the correction burden

The active packet excludes removed provider scope, drops custom retry machinery
and uses ordinary typed failure for a stale provider observation or token result.
It requires an unaffected-repository control on a fresh request. Repository
cleanup is batched with behavior fixes. The independent source analysis already
records the historical failure causes, satisfying the diagnosis evidence without
another reviewer. Original immutable reports remain as the historical record.
