# Focused follow-up review

Subject: fixed snapshot `/tmp/wyrd-task-review-ef2fe8bd`, candidate `ef2fe8bda2071f7b32cb6803b7f9f4599f0d4183`, complete cumulative diff beside `subject.md`. This report resolves only the two assigned uncertainties. It does not validate the full finding ledger or select a final task verdict.

## Authority and inspected paths

Read the task-review skill, agent rules, reference router, spec-driven-development and maintainer-style references, approved spec r5 (REQ-002 and AC-004 in particular), original TASK-001 and its T07/T14/T15 recorded evidence; relevant runtime-identity/delegation sections of `architecture/wyrd-design.md`, `architecture/wyrd-security-posture.md`, and `architecture/v1/00-foundations/permission-model.md`. Discovery reports were treated as claims.

Source inspected in the snapshot:

- `crates/wyrd-spec/src/auth/permission_scope.rs`; `crates/shared/wyrd-runtime/src/{permission,permission_check,principal,builtin_roles}.rs`.
- `crates/wyrd/wyrd-auth/src/{issuance,exchange_api_key}.rs`; signed/verified authority shapes in `crates/shared/wyrd-auth-{issue,verify}/src/lib.rs`.
- `crates/wyrd/wyrd-server/src/components/gateway/{invocation,batches}.rs`.
- `sdks/wyrd-sdk-rust/tests/integration/register_and_hydrate.rs`; Python `tests/integration/{test_gateway_inference,test_register_and_hydrate}.py`; TypeScript `wyrd/tests/integration/{gateway-inference,register-and-hydrate}.test.ts`.
- `fixtures/cards/register_and_hydrate/{authored-prompts,portable-prompt,native-prompt,inline-prompt-agent}.yaml`; gateway inference Service/Agent fixtures; `crates/wyrd-spec/src/card/prompt/{spec,hash,codec,mod}.rs`.

No tests/builds were run. No source or authority files were modified.

## 1. Permission normalization and intersection

**RESOLVED: the escalation and lost explicit grant are real; exact behavioral intersection is not closed over the existing wire grant language. A bounded security correction exists, but that alone does not establish complete approved-contract closure.**

### Producer-to-consumer trace and shared correction owner

`PermissionScope::covers` (`permission_scope.rs:151–158`) treats All as covering any Gateway scope. `Permission::covers` (`permission.rs:298–301`) combines this with resource/action coverage. `PermissionSet::insert` (`866–871`) uses that relation to discard an object grant next to All in either order. Thus a built-in invocation role plus a custom explicit deployment/model role loses the custom grant before issuance.

Role JSON is decoded and merged at `issuance.rs:778–787`. The same `resolve` owner feeds ordinary API-key/OIDC/refresh/workload issuance, direct credential verification (`verify`, 500–519), and queued owner authority recovery (`recover`, 534–552). This is not just a delegation issue: normalization changes direct and recovered principal authority too.

The only production PermissionSet-intersection caller found is `TenantGrant::into_access_grant` (`issuance.rs:230`). `DelegateToken::execute` (`exchange_api_key.rs:279–307`) verifies both tokens, extracts the subject's effective permissions, resolves the actor's current roles through the issuer, then signs the meet. `intersection` (`permission.rs:917–952`) takes the narrower scope using `PermissionScope::covers` before meeting resource/action members. All × Gateway(D) therefore produces an actual Gateway(D) grant, including through wildcard/AnyOf operation axes. The claims carry that PermissionSet (`wyrd-auth-issue/src/lib.rs:107`, `wyrd-auth-verify/src/lib.rs:584`); conversion to effective principal authority clones it (`wyrd-auth-verify/src/lib.rs:767–779`). Actor attribution does not repair it.

Gateway authorization first uses ordinary `contains` via `PermissionCheck`, then additionally requires `contains_object_scoped` for Granted deployments (`invocation.rs:709–738`). That makes All × Gateway(D) authorize Granted D although the All-only party cannot directly authorize it. The same decision owner covers requested invocation (485), candidate fallbacks (558), model visibility (677), batch admission/settlement (batches.rs:414), batch list visibility (599), and file metadata (634); file contents and continuation requests route through invocation. Batch SQL pruning additionally reads raw scopes at `batches.rs:168–190`, then retains the authoritative decision. Fixing only a denied invocation example would leave these sibling consumers and issuance paths exposed.

Smallest existing boundary: Permission/PermissionSet owns normalization and meet; TenantGrant/issuer owns refusing issuance when an exact meet cannot be carried. Preserve explicit Gateway grants beside an All grant, including broad resource/action grants that cover gateway invoke. Preserve ordinary All-to-object *requirement* matching for Open deployments: globally making `Permission::covers(All, Gateway)` false without adjusting the requirement-vs-grant distinction breaks the approved Open case. Preserve Bifrost/Verifier and unrelated wildcard normalization. No additional gateway role lookup, grant store, consumer-by-consumer deny guard, or actor-authority rule is needed.

### Why exact intersection cannot always be represented

REQ-002 defines an All-only grant as all Open deployments, and an explicit Gateway(D) grant as D in either access mode. For All-only actor and D-scoped subject (or the reverse), their common behavioral authority is **D only while D is Open**. That set is nonempty and narrower than each party. Existing atoms are only All, Deployment(D), and Model(D,M); PermissionSet combines atoms by OR (`contains`, 875–878). All widens to other Open deployments; any matching Gateway atom also admits Granted D; an empty set loses the common Open-D operation. Neither multiple existing atoms nor both All and D can encode the conjunction. Enumerating currently Open deployments into explicit grants would also improperly authorize them after their access mode changes. Meeting operation axes first fixes no part of this representational gap.

There are representable cases: All × All; same/containing explicit deployment/model grants; disjoint explicit grants; and some complete mixed sets whose other shared All/explicit grants already cover the cross-pair result. Do not reject every occurrence of All plus a scoped grant blindly. The actual resulting behavioral intersection, not merely one pair, determines representability.

### Exact meet, attenuation, and refusal are different outcomes

1. **Exact meet:** required by spec REQ-002 (“Delegation retains its permission intersection”), design `220–221` and `686–687`, security-posture `277–278`, permission-model `189–191`, and the existing PermissionSet method contract (`permission.rs:907–916`: every permission covered by both and nothing else). Existing wire atoms cannot preserve this in all valid mixed gateway cases. An additional representational/security-contract decision would be needed to issue such a token successfully.
2. **Safe attenuation:** omitting unrepresentable cross contributions prevents new Granted authority and can retain representable shared permissions. It is a safe subset, but is not the exact behavioral intersection. It silently removes Open-D access that both parties had. The security report's “fail closed” advice is sufficient for preventing amplification, not by itself proof of unchanged approved delegation behavior.
3. **Refuse the exchange:** the existing issuer/error/transaction boundary can return a structured failure before signing; `DelegateToken::execute` already propagates non-store issuance failures and stages its refusal without committing (303–316). This needs no new wire scope. However `TenantGrant::into_access_grant` is currently infallible, and neither `IssuanceError` (`260–287`) nor `DelegateError` (`98–121`) currently declares unrepresentable permission authority as an accepted refusal condition. This pair is not corrupt, inactive, malformed identity, or a signing-size failure. Reusing plumbing is possible; claiming an existing approved refusal policy is not supported by source.

Authority does not contain a sentence promising every syntactically valid exchange always succeeds; existing depth, size, activity and identity refusals are supported. It also does not expressly permit refusing a valid mixed-scope exchange because the newly introduced permission semantics cannot be represented. Both security-posture and design describe a successful delegated token with the intersection. Therefore no wire extension is inevitable **if a fail-closed exchange policy is approved**, but choosing that policy versus successful exact preservation is a new externally observable availability/security decision. This follow-up cannot silently choose it. Under the approved behavioral-intersection reading, full closure needs specification clarification/revision approving one outcome. The normalization bug is independently a bounded implementation correction. An immediate no-amplification hard stop is also bounded engineering, but must not be reported as complete exact-intersection closure.

### FOLLOW-001 — clarification of SEC-001 / INVREV-001 correction boundary

- Classification: INCORRECT, existing proposed finding; this is not a second independent exploit.
- Locations: `permission.rs:866–871,917–952`; issuer `issuance.rs:230`.
- Required bounded correction: preserve explicit gateway authority during normalization; never produce a Gateway-scoped grant from an All-only party's authority. Keep existing issuer, claim, and gateway decision owners.
- Contract decision: approve successful representation of exact mixed intersections or explicit rejection/attenuation of unrepresentable exchanges. Do not prescribe a new scope, silently drop common Open permissions, or mask the case as malformed input without approval.
- Focused proof after that decision: both insertion orders for All plus deployment/model grants; wildcard/AnyOf resource/action cases; both mixed intersection directions; explicit × explicit and All × All; mixed complete sets where other grants make the exact result representable; unchanged Bifrost behavior. Extend existing real delegation/gateway proof with direct and delegated Open/Granted outcomes, explicit-grant role merging, and no upstream IO on denial. Do not require a new harness or broad aggregate for remediation.

## 2. AC-004 proof conflict

**RESOLVED: behavior review correctly identifies a bounded proof gap. The invariant report's AC-004 PASS overstates T07/T14/T15; implementation preservation and complete three-SDK evidence must be separate.**

| Actual proof | What it establishes | What it does not establish |
|---|---|---|
| Rust T07 `register_and_hydrate.rs:485–557` | Real register/GET/hydrate for standalone portable and standalone native plus portable inline Agent Prompt materialized as its own Prompt Card; exact authored spec JSON on GET/hydrate; derived variables on GET; repeated registration keeps references; Agent navigates to generated Prompt. | Native inline authored form; explicit content-hash assertion before/after GET/hydration; derived introspection after hydration. Stable CardRef on replay is useful identity evidence, but not a comparison with the authored Prompt hash. |
| Python T14 `test_gateway_inference.py:181–230` | Both fixture forms execute through hydrated Agents; override leaves declared execution_route intact; target evidence and denial. | No GET/authored Prompt JSON comparison; no Prompt hash assertion; no variable/schema introspection assertion; both Agent fixtures reference standalone Prompt paths, so this test supplies no inline-Prompt preservation case. |
| TS T15 `gateway-inference.test.ts:387–435` | Same route/invoke behavior and declared-route check as Python. | Same preservation/hash/introspection/inline gaps. |
| Python `test_register_and_hydrate.py:163–204`; TS `register-and-hydrate.test.ts:144–183` | Existing inline/path/CardRef Prompt navigation and identity; Python checks the native runtime model; TS compares a path Prompt's spec with the independently hydrated same Prompt. | Do not compare both authored portable/native forms (standalone and inline) against authored JSON, do not prove Prompt hashes, do not assert derived variables/schema and absence of derived duplicate JSON fields. These useful navigation checks do not fill the AC-004 matrix. |

The shared implementation supports authored serialization and JCS hashing (`PromptSpec` in `card/prompt/spec.rs:6–16,47–49`; hash owner `card/prompt/hash.rs:15–23`). Existing Rust unit hash tests (`card/prompt/mod.rs:331–382`) prove changes to model/request/response format affect hashes and identical inputs repeat; they are supporting proof, not register/GET/hydrate preservation through all three SDKs. This follow-up reports missing evidence, not an established storage/hash corruption bug.

### FOLLOW-002 — MISSING: AC-004 is not fully proved through the three SDKs

- Authority: spec AC-004 (`spec.md:954–956`), task scenario 7 and scenario 12/T07/T14/T15, repository user-journey rule.
- Anchors: Rust `register_and_hydrate.rs:499–506` (only three form/context combinations), Python `test_gateway_inference.py:194–202`, TS `gateway-inference.test.ts:400–405` (invocation/route assertions stand in for preservation proof).
- Impact: Python/TS boundary serialization, inline native materialization, retained hash identity, and derived introspection can regress while these named tests still pass. The invariant matrix cannot mark all of AC-004 PASS from those tests.
- Smallest correction: extend the existing register-and-hydrate journey owners and checked-in authored Prompt fixtures. In each SDK assert portable/native authored JSON for standalone and inline materialized Prompt Cards after GET and hydrate; assert preserved authored hash identity through the owning public surface/retained Card representation; inspect derived variables and response/schema introspection while asserting those fields are absent from stored authored JSON. Complete the missing native-inline combination and add only the missing assertions to existing Rust coverage. Reuse current server fixtures and Cards/WyrdState owners; no additional harness or production abstraction.
- Focused proof: the extended existing Rust T07 and focused Python/TS register-and-hydrate preservation tests, with exact selectors recorded by the implementer. Do not rerun or require unrelated route/override matrices for this finding; final aggregate belongs to change review.

## Result and limits

**RESOLVED** for both assigned uncertainties. FOLLOW-001 narrows an existing finding's remediation claim; FOLLOW-002 records the source-backed missing AC-004 proof. All proposed dispositions remain subject to the independent Ponytail validator.

Static review only; tests/builds were intentionally not run. This report does not audit other findings or assert the complete cumulative change passes.
