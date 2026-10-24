# Discovery claim comparison

All seven required discovery reports are present for the immutable candidate. Comparison groups claims by obligation/cause; it does not confirm them.

| Proposed cause | Discovery IDs | Comparison |
|---|---|---|
| Removed HTTP query selectors are discarded before a flat read/delete | BEH-001, INV-001, SYSTEM-001, SEC-001 | Reports agree on by-ref GET/DELETE and request-to-commit path. Invariant/system also mention VersionListQuery as a sibling. Validator must decide its required scope from approved authority, avoiding invented generic query policy. |
| HTTP extractor refusals bypass stable Wyrd problem errors | BEH-002, INV-002; SEC-001 correction overlaps | Reports agree on metadata/reference declaration and list-query extraction. Validator must select an existing code and minimum adapter boundary without broad unrelated error normalization. |
| Required documentation absent/incomplete on materially changed Rust items | REPO-001, MNT-001 | Overlapping single gate cause. Repo report covers additional production/private/test items; maintainer adds find_card_by_ref and three test panic sections. Validator must audit exact material changes and retain only rule-grounded omissions, consolidating a complete bounded correction. |
| Persistence/concurrency/durability | No data findings | Data PASS; other reviews find no new core data/recovery failure. No disagreement to investigate. |

No focused followup-rev is needed: discovery reports do not materially conflict, expose an unreviewed reachable owner, or repeat prior remediation. The sibling VersionListQuery nuance can be resolved by the required independent validation against source/authority; it is already identified and traced in discovery. This is the first review round.

Nonblocking reviewer notes (test organization and stale authority wording) are not findings and are not automatically remediation scope. Independent validation must reject or establish any material consequence before inclusion. Rejected proposals must be omitted from final remediation.

Required validation inputs: subject.md, all seven discovery reports, approved spec/supplements, original task, full cumulative git diff, applicable repository authority. No intended verdict is assigned.
