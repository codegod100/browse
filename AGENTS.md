# Browse — agent notes

## Cursor Cloud specific instructions

### Review proposals: [boxci](https://boxci.boxd.sh) patch instead of a PR

Whenever you would open or update a code-review proposal — including anything you would normally do with a GitHub PR (`ManagePullRequest`, `gh pr create` / `gh pr edit`, opening a PR URL for review):

1. **Create a [boxci](https://boxci.boxd.sh) Radicle patch instead.** Use the `boxci-github-patch` skill (`POST https://boxci.boxd.sh/api/patches/from-github`).
2. Push the GitHub commit/branch first (needed so boxci can fetch the SHA), then call [boxci](https://boxci.boxd.sh) to cherry-pick onto Radicle `main` and open the patch.
3. Poll `GET https://boxci.boxd.sh/api/runs/<run_id>` until finished; surface `patch_id=` from the `github-patch` step (and a Garden/explorer link). Do **not** report a GitHub PR URL as the review artifact.
4. Do **not** call `ManagePullRequest`, `gh pr create`, `git push rad …`, the `rad-patch` skill, or Radicle MCP `create_patch` unless the user explicitly asks for a GitHub PR or a local `rad` push.
5. If cherry-pick conflicts, rebase the fix onto the current Radicle tip, force-push the GitHub branch, and retry boxci — do not fall back to a GitHub PR or local `rad push`.

**boxci:** [https://boxci.boxd.sh](https://boxci.boxd.sh) · merge pipeline [`.boxci/pipeline.yml`](.boxci/pipeline.yml) · helpers [`scripts/boxci/`](scripts/boxci/)  
**This repo’s Radicle RID:** [`rad:z2QL7QdL2QGg6FmX3wcw3Mzm2ykE3`](https://nandi.radicle.garden/rad:z2QL7QdL2QGg6FmX3wcw3Mzm2ykE3)  
**GitHub:** `https://github.com/codegod100/browse.git`

`.cursor/environment.json` only installs personal Cursor skills (including `boxci-github-patch`). It does not set review workflow; that preference lives here and in `.cursor/rules/boxci-patch.mdc`.
