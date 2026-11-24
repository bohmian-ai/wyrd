# Independent execution evidence

Candidate: `c8cbfeeb0d5cf3a4fa63bb386cbf460f009ead24`.
Base: `451aa0c4d332af3bf8f334ea9eb1bc9d0679a34a`.
Commands run sequentially on unchanged production source. Reports are the only
working-tree additions. Historical implementation evidence remains in TASK-002.

| Command | Result |
| --- | --- |
| `git diff --check 451aa0c4d...c8cbfeeb0` | PASS |
| `mise exec -- cargo nextest run --locked -p wyrd-kms -p wyrd-crypt --lib` | PASS 21/21; nextest `f75f94ea-d423-409c-b62b-11a69629d56b` |
| `mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-cli --test cli --run-ignored=all -E 'test(=pg_tests::init_keeps_one_stored_secret_key_for_every_owner)'` | PASS 1/1, 60 excluded; nextest `1258304f-d826-443f-ab03-6c02bcf42896` |

`mise run test:kms:vault` also passed 1/1 against real dev Vault Transit; two cloud tests excluded. Nextest `7556e942-e391-4983-a890-6ca442ff90ca`.

The init command corrects the original task evidence's inaccurate `--test pg_tests`
and unqualified selector; the existing mise lane already has the correct target.
The CLI linker emitted a macOS compact-unwind size warning; the selected test passed.

No live AWS/GCP or Linux-only startup execution is claimed. The maintainer explicitly
accepted those CI limits. No benchmark was requested; the current root AGENTS and
spec explicitly withdraw automatic qualification. No broad aggregate rerun.
TDD history is not recreated by GREEN reruns.

## Exact Vault selector and setup correction

A direct `mise exec -- cargo nextest run --locked -p wyrd-kms --test integration
--run-ignored=all -E 'test(=integration_vault::transit_wraps_rotates_and_refuses_through_the_facade)'`
failed at fixture startup because `vault` was not on PATH. It is scoped to the
managed task's tool declaration. This setup failure is not a product diagnosis.
The exact command with the same task tool supplied passed:

`mise exec vault@2.1.1 -- cargo nextest run --locked -p wyrd-kms --test integration --run-ignored=all -E 'test(=integration_vault::transit_wraps_rotates_and_refuses_through_the_facade)'`

PASS 1/1; nextest `a888bed6-8252-43e6-afbe-e9decea3a7cb`.
