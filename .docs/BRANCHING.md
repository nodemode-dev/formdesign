# Branching and release model

FormDesign is built by a small team with agents doing much of the typing, so the process stays light: one protected mainline, short-lived work branches, and every change reviewed as a diff before it lands.

## The branches

```
main ──●────────●────────●──────── protected, always releasable, tagged v*.*.*
        \      /  \     /
         feat/..   fix/..           short-lived work branches, deleted after merge
```

| Branch | Role | Rules |
| --- | --- | --- |
| `main` | Stable, releasable at any commit | Protected. CI (`.github/workflows/ci.yml`) must pass. No force-push. Releases are tagged here. |
| `feat/[area]-[slug]` | New capability | Branch from `main`, pull request back to `main`. |
| `fix/[area]-[slug]` | Bug fix | Branch from `main`, pull request back to `main`. |
| `docs/[slug]` | README, `.docs/wiki`, `docs/` | Branch from `main`. Self-merge is fine for docs-only changes; say so in the pull request. |
| `chore/[slug]` | Scripts, CI, dependencies | Branch from `main`, pull request back to `main`. |
| `experiment/[slug]` | Spikes | Never merged directly; cherry-pick what survives onto a fresh `feat/*` branch. |

## Area prefixes

Name the area the change mostly lives in: `core` (native/formdesign-core), `engine` (backend), `make` (backend/app/autolayout), `intake`, `templates`, `editor` (frontend/src/editor), `panels` (frontend/src/features), `desktop` (src-tauri), `agents` (.claude, MCP), `ci`, `docs`. Examples: `feat/panels-table`, `fix/make-fit-short-fields`.

## Commits

- Conventional Commits with the area as the scope: `feat(intake): read pages and dates from a brief`.
- No emoji, no em dash, in messages or anywhere in the tree.
- One coherent change per commit; the version bump commit comes last in a release series.
- Commit identity for release work: `nodemode-ai <mackin@nodemode.ai>`.

## Versioning

Semver. Patch for fixes and docs, minor for new features or modules, major for milestones and breaking changes. A version reads `v` then three dot-separated integers, always with dots: `v0.7.0`, never `v0_7_0`. Bump every version site in one commit:

| File | Field |
| --- | --- |
| `frontend/package.json` | `version` |
| `frontend/package-lock.json` | the top-level `version` and `packages[""].version` |
| `frontend/src/version.ts` | `VERSION` |
| `backend/app/__init__.py` | `__version__` |
| `native/Cargo.toml` and `native/Cargo.lock` | `workspace.package.version`, the `formdesign-core` entry |
| `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`, `src-tauri/tauri.conf.json` | the `formdesign` package, `version` |
| `README.md` | the version badge and the Status heading |

## Releasing

1. Merge the release pull request into `main` with the bump commit last.
2. Tag it: `git tag -a v0.7.0 -m "FormDesign v0.7.0: ..."` and `git push origin main --tags`.
3. The tag runs `.github/workflows/desktop.yml`: macOS arm64, Linux x64 and Windows x64 bundles, the Intel Mac core, and the `formdesign-core-[triple]` assets go into a draft GitHub release.
4. Review the draft and publish it. `./formdesign install-core` downloads its prebuilt core from the published release, so a draft is not enough for machines without Rust.

Signing secrets and the updater key are described on the wiki's Releasing page.

## The wiki

Edit pages in `.docs/wiki/`, never in the GitHub wiki editor. `.github/workflows/wiki.yml` publishes the folder when it changes on `main`. The GitHub wiki needs one page created by hand before the first publish.

## One-time GitHub settings

Under Settings, Branches, protect `main`: require a pull request, require the `ci` checks, block force pushes and deletions. Under Settings, Actions, allow workflows to write contents (the desktop and wiki workflows need it).
