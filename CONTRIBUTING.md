# Contributing to Sukoon

Thank you for helping build Sukoon. This is a tool meant to benefit a community, and it gets
better mostly through honest, specific feedback on where the separation falls short.

## The most valuable contributions (especially early)

1. **Evaluation clips + honest quality reports.** "Audible music left under this nasheed's
   backing track," "MDX-Net clipped the speaker's `s` sounds here," "abrupt music→silence
   transition produced a click." These shape the whole project. See
   [docs/contributing/model-eval.md](./docs/contributing/model-eval.md).
2. **Platform shells.** Desktop, mobile, and web shells are wanted — they just need to call
   `sukoon-core`, never reimplement separation.
3. **Scholarly input** on voice-preservation policy — coordinated via [GOVERNANCE.md](./GOVERNANCE.md).
4. **Docs fixes.** If a page confused you, that's a bug.

## Ground rules

- **Never add ad-blocking, ad-skipping, or ad-muting.** It's against the project's principles
  and against store policy. PRs that touch ads will be closed. Ads are _bypassed_, never altered.
- **Don't reimplement separation in a shell.** If a platform needs a capability, add it to
  `sukoon-core` and expose it.
- **Don't commit model weights or media.** They're downloaded or ignored; keep eval clips local.

## Development setup

| Tool   | Version          | For                                |
| ------ | ---------------- | ---------------------------------- |
| Rust   | stable ≥ 1.80    | `sukoon-core`, `sukoon-cli`, Tauri |
| Node   | ≥ 22             | extension, web, desktop UI         |
| pnpm   | ≥ 10             | JS workspaces                      |
| FFmpeg | ≥ 6 (LGPL build) | audio I/O at runtime               |

```bash
git clone https://github.com/ruwadgroup/sukoon.git
cd sukoon
cargo build                 # core + cli
pnpm install                # JS workspaces
cargo test && pnpm -r test  # run the suites
```

See [docs/start/installation.md](./docs/start/installation.md) for the full path including model downloads.

## Workflow

1. Branch from `main`: `git checkout -b feat/short-description`.
2. Make the change. Add or update tests. Keep `sukoon-core` the single source of truth.
3. Run the checks below.
4. Open a PR using the template. Link the issue. Describe what you tested and on what hardware.

## Checks (must pass before merge)

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
pnpm lint && pnpm -r test
```

## Commit style

We use [Conventional Commits](https://www.conventionalcommits.org). Examples:

- `feat(core): add mdx-lite low-RAM fallback engine`
- `fix(extension): debounce ad-state MutationObserver`
- `docs(extension): clarify ad handling`

Scopes follow the package names: `core`, `cli`, `extension`, `desktop`, `web`, `docs`.

## Reporting bugs & ideas

Use the [issue templates](https://github.com/ruwadgroup/sukoon/issues/new/choose). For a quality
problem, attach (or describe) a short clip and tell us: which tool (extension or file separation),
platform, and hardware.

## Code of Conduct

Participation is governed by our [Code of Conduct](./CODE_OF_CONDUCT.md). Be kind; assume good faith.
