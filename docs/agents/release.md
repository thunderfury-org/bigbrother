# Release

When tagging, bumping the crate version, generating the changelog, or running `make release` / `make release-tag` / `make changelog`.

`main` requires a PR. Do not push the release commit or tag from local `main`.

## Version

`make changelog` previews unreleased commits and the next semver git-cliff would pick. Use that value, or an explicit pre-release such as `0.2.0-rc1`, as `VERSION`. `git-cliff` must be installed.

## Cut

`make release VERSION=x.y.z` creates `dev/release-x.y.z` from `origin/main`, runs tests and lint, bumps `Cargo.toml`, writes the changelog, and commits `chore: release vX`. It does not tag.

Push the branch and open a PR. After it is merged onto `main`:

```bash
git checkout main && git pull
make release-tag VERSION=x.y.z
```

That tags `origin/main` as `vX` and pushes the tag. CI builds a multi-arch image to `ghcr.io` and opens a GitHub Release whose body is that version's CHANGELOG section.
