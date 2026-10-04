#!/usr/bin/env bash
# Releases stylet: sets the version, commits, tags and pushes; the Release
# workflow then builds the binaries and publishes to npm and GitHub.
#   scripts/release.sh 0.2.0
set -euo pipefail
cd "$(dirname "$0")/.."

version=${1:-}
if [[ ! $version =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$ ]]; then
  echo "usage: scripts/release.sh <version>   (e.g. 0.2.0 or 0.2.0-beta.1)" >&2
  exit 1
fi
if [[ $(git branch --show-current) != main ]]; then
  echo "release from main" >&2
  exit 1
fi
if [[ -n $(git status --porcelain) ]]; then
  echo "the working tree isn't clean" >&2
  exit 1
fi
git fetch -q origin main
if [[ $(git rev-parse HEAD) != $(git rev-parse origin/main) ]]; then
  echo "main isn't in sync with origin/main" >&2
  exit 1
fi
if git rev-parse -q --verify "refs/tags/v$version" >/dev/null; then
  echo "v$version already exists" >&2
  exit 1
fi

# The workspace version (first `version = ` in Cargo.toml) and the lock file.
perl -0pi -e "s/^version = \".*?\"/version = \"$version\"/m" Cargo.toml
cargo update --workspace --quiet
cargo test --workspace --quiet

git commit -qam "Release v$version"
git tag -a "v$version" -m "v$version"
read -r -p "Push v$version (starts the Release workflow)? [y/N] " answer
if [[ $answer == [yY] ]]; then
  git push -q origin main "v$version"
  echo "pushed; follow it with: gh run watch \$(gh run list -w Release -L1 --json databaseId -q '.[0].databaseId')"
else
  echo "not pushed; to publish later: git push origin main v$version"
fi
