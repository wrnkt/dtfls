# Releasing

Pushing a `v*` tag triggers `.github/workflows/release.yml`, which:

1. Builds the sdist and wheel, and fails if the tag doesn't match the
   version in `pyproject.toml`.
2. Attaches both artifacts to a GitHub Release.
3. Publishes to PyPI via Trusted Publishing.
4. Bumps the `dtfls` formula in the `wrnkt/homebrew-dtfls` tap to point at
   the new tag.

Steps 3 and 4 each require one-time setup outside this repo before the
first release.

## One-time setup

### PyPI Trusted Publishing

No API tokens to create or store — GitHub's OIDC identity for this
workflow is registered directly with PyPI as a trusted publisher.

1. On [pypi.org](https://pypi.org), under Account Settings → Publishing,
   add a **pending publisher**:
   - PyPI project name: `dtfls`
   - Owner: `wrnkt`
   - Repository name: `dtfls`
   - Workflow name: `release.yml`
   - Environment name: `pypi`
2. This can be done before the `dtfls` project exists on PyPI at all —
   the first successful publish from that workflow creates it.

### Homebrew tap

1. Create a public repo named `wrnkt/homebrew-dtfls`.
2. Commit `packaging/homebrew/dtfls.rb` from this repo into it at
   `Formula/dtfls.rb` (the placeholder `url`/`sha256` will be overwritten
   by the first automated bump, but the file needs to exist first).
3. Create a GitHub PAT with `contents:write` on the tap repo, and add it
   to this repo (`wrnkt/dtfls`) as a secret named `HOMEBREW_TAP_TOKEN`.
   The default `GITHUB_TOKEN` can't push to a different repository, which
   is why a separate token is needed here.

## Cutting a release

1. Bump `version` in `pyproject.toml`.
2. Commit, then tag and push:
   ```
   git tag vX.Y.Z
   git push origin vX.Y.Z
   ```
3. Watch the `Release` workflow run. The `pypi` and `homebrew` jobs will
   fail (independently of the GitHub Release, which doesn't depend on
   them) until the one-time setup above is complete.
