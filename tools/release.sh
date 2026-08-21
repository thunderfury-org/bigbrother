#!/usr/bin/env bash
set -euo pipefail

# Release script for bigbrother.
# Usage: tools/release.sh 0.2.0

VERSION="${1:-}"

if [ -z "$VERSION" ]; then
    echo "Usage: $0 <version>"
    echo "Example: $0 0.2.0"
    exit 1
fi

# Validate we're on main branch
BRANCH="$(git rev-parse --abbrev-ref HEAD)"
if [ "$BRANCH" != "main" ]; then
    echo "Error: must be on 'main' branch (currently on '$BRANCH')"
    exit 1
fi

# Validate working tree is clean
if [ -n "$(git status --porcelain)" ]; then
    echo "Error: working tree is dirty. Commit or stash changes first."
    exit 1
fi

# Validate git-cliff is installed
if ! command -v git-cliff &> /dev/null; then
    echo "Error: git-cliff is not installed."
    echo "Install it with: cargo install git-cliff"
    exit 1
fi

# Validate version format (semver-like)
if ! echo "$VERSION" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]+(-[a-zA-Z0-9.]+)?$'; then
    echo "Error: version must be semver-like (e.g., 0.2.0, 0.2.0-rc1)"
    exit 1
fi

echo "=== Preparing release v$VERSION ==="

# Run tests
echo ""
echo "--- Running tests ---"
cargo test

# Run lint
echo ""
echo "--- Running lint ---"
cargo fmt --all -- --check
cargo clippy -- -D warnings

# Bump version in Cargo.toml
echo ""
echo "--- Bumping version to $VERSION ---"
sed -i.bak "s/^version = \".*\"/version = \"$VERSION\"/" Cargo.toml
rm -f Cargo.toml.bak

# Verify it compiles with new version
echo ""
echo "--- Verifying compilation ---"
cargo check

# Generate changelog
echo ""
echo "--- Generating changelog ---"
git cliff --bump --tag "v$VERSION" -o CHANGELOG.md

# Commit and tag
echo ""
echo "--- Committing and tagging ---"
git add Cargo.toml Cargo.lock CHANGELOG.md
git commit -m "chore: release v$VERSION"
git tag -a "v$VERSION" -m "v$VERSION"

echo ""
echo "=== Release v$VERSION prepared ==="
echo ""
echo "Review the commit and tag:"
echo "  git log -1"
echo "  git show v$VERSION"
echo ""
echo "When ready, push to trigger CI (Docker build + GitHub Release):"
echo "  git push --follow-tags"
