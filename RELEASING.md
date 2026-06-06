# Releasing

Sukoon follows [Semantic Versioning](https://semver.org). Pre-1.0, **minor versions may
introduce breaking changes**; patch versions never do.

## Versioning surfaces

The monorepo has independently versioned artifacts:

- **Rust crates** (`sukoon-core`, `sukoon-cli`) — versioned in their `Cargo.toml`, published to
  crates.io.
- **JS packages** (`@sukoon/*`) — versioned in their `package.json`, published to npm.
- **Extension / desktop apps** — versioned for store submission.

The repository tag (`v0.1.0`) tracks the coordinated release; individual artifacts may have their
own patch cadence between coordinated releases.

## Pre-release checklist

1. `cargo fmt --all -- --check` and `cargo clippy --all-targets -- -D warnings` clean.
2. `cargo test` and `pnpm -r test` green on CI.
3. `CHANGELOG.md` updated (grouped by package, Conventional-Commits-derived).
4. `LICENSING.md` re-checked if any model or FFmpeg dependency changed.
5. Docs reflect any API change.
6. Version bumped in the relevant manifest(s).

## Cutting a release

```bash
# Tag the coordinated release
git tag -a v0.1.0 -m "sukoon v0.1.0"
git push origin v0.1.0
```

CI publishes:

- crates to **crates.io** (`cargo publish` with provenance),
- npm packages with `--provenance`,
- desktop binaries as GitHub Release artifacts (Windows `.msi`, macOS `.dmg`, Linux AppImage),
- the extension package as an artifact for manual store submission (store review is not automated).

## Model assets

Model weights are **not** part of a release artifact. They are downloaded on first use and pinned
by checksum in the engine registry. Bumping a model means updating its registry entry (URL +
checksum + license) and noting it in `CHANGELOG.md` and, if licensing changes, `LICENSING.md`.

## Security releases

Critical fixes are released out-of-band as a patch and noted in the GitHub Security Advisory. See
[SECURITY.md](./SECURITY.md).
