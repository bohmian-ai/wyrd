# Independent remediation verification

Wyrd candidate b5c0db2e01268d6ad033fbe7450069247edff6ee; library
9046d1035538df8dfb661f826b102316ee6d0b4e. Both started with clean tracked trees.

- Incremental Wyrd and library `git diff --check`: PASS.
- Library `git merge-base --is-ancestor 9046d103 origin/main`: PASS;
  local origin/main is 8adb0b7b5aed595eab31f4f3e99b3c20fb4ac343.
- Library `mise exec -- env RUSTDOCFLAGS='-D warnings' cargo doc --locked -p bohmian-code-review-pipeline --no-deps --document-private-items`: PASS. Existing vendored C grammar compiler warnings do not fail rustdoc.
- Wyrd `mise run check:docs`: FAIL, exit 101. Exact command is
  `RUSTDOCFLAGS="-D missing_docs -D rustdoc::broken_intra_doc_links" cargo doc --locked --no-deps --all-features -p wyrd-spec -p wyrd-auth-issue -p wyrd-auth-verify`.
  Fatal errors are unresolved module links GithubConnectResponse,
  GithubSetupQuery, GithubCallbackQuery and GithubConnectionView.
  Private-link warnings in auth-issue/sign_access_token,
  auth/human_connection/validate and auth/oidc/SHA256_HEX_PATTERN are nonfatal.
  A scoped independent standards reviewer evaluates exact authority and minimum
  correction; no warning sweep or source change is performed here.

Prior focused runtime tests and unaffected technical assertions remain the
approved full review's evidence. No runtime/mutation/cloud tests are repeated
for docs-only remediation. Source is immutable; only review reports are written.
