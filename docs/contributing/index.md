# Contributing (docs)

The on-ramp lives in [CONTRIBUTING.md](../../CONTRIBUTING.md) at the repo root. This section adds
the contributor-facing detail that's too long for that page.

- **[Model eval](./model-eval.md)** — how to report separation-quality problems and build the eval
  corpus. The single most valuable contribution early on.

## The rules that matter most

1. **Separation logic lives only in `sukoon-core`.** Shells call in; they never reimplement it.
2. **Never add ad-blocking/skipping/muting/disabling.** Ads always play in full; the engine just
   removes their background music like any other audio. Be aware this alters ad audio — a compliance
   trade-off ([design-considerations §1](../design-considerations.md#1-ads-play-in-full--with-their-music-removed)).
3. **Never add video download/rip.** We process audio live or clean files the user already has
   ([§2](../design-considerations.md#2-why-we-never-download-the-video)).
4. **Don't commit model weights or media.** They're downloaded/ignored.
5. **Halal-aware text needs advisor review** before shipping ([GOVERNANCE.md](../../GOVERNANCE.md)).

## Local checks

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
pnpm lint && pnpm -r test
```

## Commits

Conventional Commits, scopes from the package names (`core`, `cli`, `extension`, `desktop`,
`mobile`, `web`, `docs`). Husky runs lint-staged + commitlint on commit.
