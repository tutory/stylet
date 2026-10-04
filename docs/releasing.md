# Releasing

Releases run on GitHub Actions. Start the **Release** workflow from the
[Actions tab](https://github.com/tutory/stylet/actions/workflows/release.yml)
("Run workflow", enter the version), or:

```sh
gh workflow run release.yml -f version=0.2.0      # or 0.2.0-beta.1 for a prerelease
gh run watch $(gh run list -w release.yml -L1 --json databaseId -q '.[0].databaseId')
```

The [workflow](../.github/workflows/release.yml):

1. sets the workspace version in `Cargo.toml`, commits "Release v0.2.0" to `main` and
   tags it `v0.2.0`,
2. builds `stylet` for macOS (arm64, x64), Linux (x64, arm64; static musl binaries) and
   Windows (x64),
3. publishes `@tutory_de/stylet-<platform>` packages with the binaries and then
   `@tutory_de/stylet`, which depends on them optionally (npm installs only the one
   for the current platform) and provides the `stylet` command,
4. packages the VS Code extension once per platform with its binary (plus one without
   a binary for other platforms) and publishes it to the
   [VS Code Marketplace](https://marketplace.visualstudio.com/items?itemName=tutory.stylet)
   and [Open VSX](https://open-vsx.org/extension/tutory/stylet) (used by Cursor,
   VSCodium, Windsurf),
5. creates a GitHub release with the binaries as archives, the VSIX files and
   generated notes.

The marketplaces only accept `X.Y.Z` versions: prereleases aren't published there;
their VSIX files (marked as pre-release) are attached to the GitHub release.

If a later step fails, fix the cause and use "Re-run failed jobs": the commit and tag
stay, and packages that were already published are skipped.

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

The extension is published with two more secrets; while one is missing, that
marketplace is skipped with a warning:

- `VSCE_PAT`: an Azure DevOps personal access token with the scope "Marketplace →
  Manage" for "All accessible organizations", from an account that manages the
  publisher `tutory` on [marketplace.visualstudio.com/manage](https://marketplace.visualstudio.com/manage).
- `OVSX_PAT`: an [Open VSX](https://open-vsx.org) access token (sign in with GitHub,
  accept the publisher agreement); the namespace is created once with
  `npx ovsx create-namespace tutory -p <token>`.

```sh
gh secret set VSCE_PAT --repo tutory/stylet
gh secret set OVSX_PAT --repo tutory/stylet
```

## Trying the packages locally

```sh
cargo build --release -p stylet-cli
mkdir -p /tmp/bin/darwin-arm64 && cp target/release/stylet /tmp/bin/darwin-arm64/
# build.mjs expects every platform; put placeholder files in the other directories
node npm/build.mjs 0.0.0-local /tmp/bin
npm pack npm/dist/stylet npm/dist/stylet-darwin-arm64
```
