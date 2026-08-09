# Browse — agent notes

## Cursor Cloud specific instructions

### Review proposals: boxci patch, not `rad push`

When opening or updating a code-review proposal from a Cloud Agent:

1. **Prefer** the `boxci-github-patch` skill (`POST https://boxci.boxd.sh/api/patches/from-github`).
2. **Do not** open patches by running `git push rad …`, the `rad-patch` skill, or the Radicle MCP `create_patch` tool from this VM unless the user explicitly asks for a local `rad` push (Cloud clones usually have no usable `rad` remote / identity).
3. Push the GitHub commit first, then ask boxci to cherry-pick it onto Radicle `main` and open the patch. Poll the run until `patch_id=` appears; report that id and the Garden URL.
4. If cherry-pick conflicts, rebase the fix onto the current Radicle tip, force-push the GitHub branch, and retry boxci — do not fall back to local `rad push`.

**This repo’s Radicle RID:** `rad:z2QL7QdL2QGg6FmX3wcw3Mzm2ykE3`  
**GitHub:** `https://github.com/codegod100/browse.git`

`.cursor/environment.json` only installs personal Cursor skills (including `boxci-github-patch`). It does not set review workflow; that preference lives here.
