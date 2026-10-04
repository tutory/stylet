# Releasing

```sh
scripts/release.sh 0.2.0          # or 0.2.0-beta.1 for a prerelease
```

The script sets the workspace version in `Cargo.toml`, runs the tests, commits
"Release v0.2.0", tags `v0.2.0` and (after asking) pushes both. The tag starts the
[Release workflow](../.github/workflows/release.yml), which:

1. checks that the tag matches the `Cargo.toml` version,
2. builds `stylet` for macOS (arm64, x64), Linux (x64, arm64; static musl binaries) and
   Windows (x64),
3. publishes `@tutory_de/stylet-<platform>` packages with the binaries and then
   `@tutory_de/stylet`, which depends on them optionally (npm installs only the one
   for the current platform) and provides the `stylet` command,
4. creates a GitHub release with the binaries as archives and generated notes.

Prereleases (`-beta.1` and the like) are published under the npm tag `next` and
marked as prereleases on GitHub.

## Setup

The workflow publishes with the repository secret `NPM_TOKEN`: a granular access
token of the `tutory_de` npm account with read and write access to the `@tutory_de`
scope (and "bypass two-factor authentication" enabled, since CI can't enter a code).

```sh
gh secret set NPM_TOKEN --repo tutory/stylet
```

Packages are published with provenance, so npm shows which workflow run built them.

## Trying the packages locally

```sh
cargo build --release -p stylet-cli
mkdir -p /tmp/bin/darwin-arm64 && cp target/release/stylet /tmp/bin/darwin-arm64/
# build.mjs expects every platform; put placeholder files in the other directories
node npm/build.mjs 0.0.0-local /tmp/bin
npm pack npm/dist/stylet npm/dist/stylet-darwin-arm64
```
