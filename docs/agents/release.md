# Release

When tagging, bumping the crate version, generating the changelog, or running `make release` / `make changelog`.

Release cuts happen on a clean `main`. Other work still uses a working branch.

## Version

`make changelog` previews unreleased commits and the next semver git-cliff would pick. Use that value, or an explicit pre-release such as `0.2.0-rc1`, as `VERSION`. `git-cliff` must be installed.

## Cut

`make release VERSION=x.y.z` prepares the release locally: tests, lint, `Cargo.toml` bump, changelog, `chore: release vX` commit, and annotated `vX` tag. It does not push.

Review `git log -1` and `git show vX`. Then `git push --follow-tags`.

CI builds a multi-arch image to `ghcr.io` and opens a GitHub Release whose body is that version's CHANGELOG section.
