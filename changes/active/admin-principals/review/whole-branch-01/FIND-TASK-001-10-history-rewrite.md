# `FIND-TASK-001-10` — the branch-owner history rewrite

The finding assigns this step to the branch owner, not the implementation
agent, and forbids `git config`, identity environment variables, and any
replacement trailer. This file is what the implementation agent can supply: the
exact command, and a correction to the state it has to clean up.

## Correction to the candidate

Nine commits made while closing these findings — `d4fab19d1`, `ad4eb9dd7`,
`7798918b6`, `d5670dce4`, `32bc80f77`, `bbd09ba2b`, `07c638d9a`, `aae714ec1`,
`b4fb9af6a`, `5ea453f13`, `8b603c36e`, `5166921e6`, `4c9c81f6d`, `52d3a35d4`,
`50fc266d0` — carry `Co-Authored-By: Claude Opus 5 (1M context)` and a
`Claude-Session:` trailer. AGENTS.md section 13 forbids AI co-author trailers,
so these are additional instances of the violation this finding names, not
exceptions to it. Their author and committer are already
`Thorrester <sjforrester32@gmail.com>`; only the trailers need removing. Later
commits on this branch omit both trailers.

## The rewrite

From the branch, with the contributor identity already configured:

```bash
git rebase main --exec 'git commit --amend --reset-author \
  -m "$(git log -1 --pretty=%B | grep -v "^Co-Authored-By: Claude" \
                               | grep -v "^Claude-Session:")"'
```

`--reset-author` takes the author from the configured identity and the rebase
sets the committer, so no `git config` call and no identity environment
variable is involved. The `grep -v` pair strips the two offending trailer forms
and adds nothing in their place.

Verify the cumulative tree is unchanged and no offending identity survives:

```bash
git diff <pre-rewrite-HEAD> HEAD --stat        # must be empty
git log main..HEAD --format='%an <%ae> | %cn <%ce>' | sort -u
git log main..HEAD --format=%B | grep -iE 'co-authored-by|claude-session'
```

The first command must print nothing, the second only
`Thorrester <sjforrester32@gmail.com> | Thorrester <sjforrester32@gmail.com>`,
and the third nothing.

Do this after the tree is final. It rewrites every unmerged commit hash, so any
other worktree or session on this branch must be re-pointed afterwards — and
the concurrent `verified-change-contract` change shares this worktree.
