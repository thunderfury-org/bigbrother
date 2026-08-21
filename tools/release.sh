#!/usr/bin/env bash
set -euo pipefail

# Prepare a release PR, or tag origin/main after that PR is merged.
# Usage:
#   tools/release.sh 0.2.0
#   tools/release.sh --tag 0.2.0

MODE="prepare"
VERSION=""

if [ "${1:-}" = "--tag" ]; then
    MODE="tag"
    VERSION="${2:-}"
    if [ -z "$VERSION" ] || [ -n "${3:-}" ]; then
        echo "Usage: $0 --tag <version>"
        echo "Example: $0 --tag 0.2.0"
        exit 1
    fi
else
    VERSION="${1:-}"
    if [ -z "$VERSION" ] || [ -n "${2:-}" ]; then
        echo "Usage: $0 <version>"
        echo "       $0 --tag <version>"
        echo "Example: $0 0.2.0"
        exit 1
    fi
fi

if ! echo "$VERSION" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]+(-[a-zA-Z0-9.]+)?$'; then
    echo "Error: version must be semver-like (e.g., 0.2.0, 0.2.0-rc1)"
    exit 1
fi

if [ -n "$(git status --porcelain)" ]; then
    echo "Error: working tree is dirty. Commit or stash changes first."
    exit 1
fi

crate_version() {
    sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1
}

if [ "$MODE" = "tag" ]; then
    BRANCH="$(git rev-parse --abbrev-ref HEAD)"
    if [ "$BRANCH" != "main" ]; then
        echo "Error: tagging requires 'main' (currently on '$BRANCH')"
        echo "After the release PR is merged: git checkout main && git pull"
        exit 1
    fi

    echo "=== Tagging v$VERSION ==="
    git fetch origin main

    HEAD_SHA="$(git rev-parse HEAD)"
    ORIGIN_SHA="$(git rev-parse origin/main)"
    if [ "$HEAD_SHA" != "$ORIGIN_SHA" ]; then
        echo "Error: local main does not match origin/main"
        echo "Run: git pull"
        exit 1
    fi

    CURRENT_VERSION="$(crate_version)"
    if [ "$CURRENT_VERSION" != "$VERSION" ]; then
        echo "Error: Cargo.toml is '$CURRENT_VERSION', expected '$VERSION'"
        echo "Tag after the release PR is merged to main"
        exit 1
    fi

    TAG="v$VERSION"
    if git ls-remote --tags origin "refs/tags/$TAG" | grep -q .; then
        echo "Error: $TAG already exists on origin"
        exit 1
    fi

    if git rev-parse "$TAG" >/dev/null 2>&1; then
        TAG_SHA="$(git rev-parse "$TAG^{commit}")"
        if [ "$TAG_SHA" != "$HEAD_SHA" ]; then
            echo "Error: local $TAG points at $TAG_SHA, not HEAD $HEAD_SHA"
            echo "Move it with: git tag -d $TAG"
            exit 1
        fi
        echo "Local $TAG already points at HEAD"
    else
        git tag -a "$TAG" -m "$TAG"
    fi

    echo "--- Pushing $TAG ---"
    git push origin "$TAG"

    echo ""
    echo "=== Tagged $TAG ==="
    echo "CI will build the Docker image and create the GitHub Release."
    exit 0
fi

if ! command -v git-cliff &> /dev/null; then
    echo "Error: git-cliff is not installed."
    echo "Install it with: cargo install git-cliff"
    exit 1
fi

RELEASE_BRANCH="dev/release-$VERSION"
TAG="v$VERSION"

if git show-ref --verify --quiet "refs/heads/$RELEASE_BRANCH"; then
    echo "Error: local branch $RELEASE_BRANCH already exists"
    exit 1
fi

git fetch origin main
if git ls-remote --heads origin "$RELEASE_BRANCH" | grep -q .; then
    echo "Error: origin already has $RELEASE_BRANCH"
    exit 1
fi

echo "=== Preparing release $TAG ==="
echo "--- Creating $RELEASE_BRANCH from origin/main ---"
git checkout -b "$RELEASE_BRANCH" origin/main

if [ "$(crate_version)" = "$VERSION" ]; then
    echo "Error: Cargo.toml is already version $VERSION"
    exit 1
fi

echo ""
echo "--- Running tests ---"
cargo test

echo ""
echo "--- Running lint ---"
cargo fmt --all -- --check
cargo clippy -- -D warnings

echo ""
echo "--- Bumping version to $VERSION ---"
sed -i.bak "s/^version = \".*\"/version = \"$VERSION\"/" Cargo.toml
rm -f Cargo.toml.bak

echo ""
echo "--- Verifying compilation ---"
cargo check

echo ""
echo "--- Generating changelog ---"
git cliff --bump --tag "$TAG" -o CHANGELOG.md

echo ""
echo "--- Committing ---"
git add Cargo.toml Cargo.lock CHANGELOG.md
git commit -m "chore: release $TAG"

echo ""
echo "=== Release $TAG prepared on $RELEASE_BRANCH ==="
echo ""
echo "Push and open a PR:"
echo "  git push -u origin HEAD"
echo "  gh pr create --base main --title \"chore: release $TAG\""
echo ""
echo "After it is merged:"
echo "  git checkout main && git pull"
echo "  make release-tag VERSION=$VERSION"
