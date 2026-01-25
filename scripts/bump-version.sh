#!/bin/bash
# Manual version sync script for emergency releases
# Usage: ./scripts/bump-version.sh 0.2.0

set -euo pipefail

VERSION="${1:-}"

if [[ -z "$VERSION" ]]; then
    echo "Usage: $0 <version>"
    echo "Example: $0 0.2.0"
    exit 1
fi

# Remove 'v' prefix if present
VERSION="${VERSION#v}"

echo "Bumping version to $VERSION..."

# Update Cargo.toml (workspace version)
sed -i "s/^version = \"[^\"]*\"/version = \"$VERSION\"/" Cargo.toml

# Update pixi.toml
sed -i "s/^version = \"[^\"]*\"/version = \"$VERSION\"/" pixi.toml

# Update dashboard/package.json
jq ".version = \"$VERSION\"" dashboard/package.json > dashboard/package.json.tmp && \
    mv dashboard/package.json.tmp dashboard/package.json

# Update bubbaloop-tui/package.json
jq ".version = \"$VERSION\"" bubbaloop-tui/package.json > bubbaloop-tui/package.json.tmp && \
    mv bubbaloop-tui/package.json.tmp bubbaloop-tui/package.json

# Update release-please manifest
jq ". = {\".\": \"$VERSION\"}" .release-please-manifest.json > .release-please-manifest.json.tmp && \
    mv .release-please-manifest.json.tmp .release-please-manifest.json

echo "Version bumped to $VERSION in:"
echo "  - Cargo.toml"
echo "  - pixi.toml"
echo "  - dashboard/package.json"
echo "  - bubbaloop-tui/package.json"
echo "  - .release-please-manifest.json"
echo ""
echo "Don't forget to commit these changes!"
