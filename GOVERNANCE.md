# Governance

Sukoon is an open-source project maintained by Tamim Bin Hakim and contributors. This document
describes how decisions are made — including the part unusual to most software projects: the
**halal-aware feature set**.

## Roles

- **Maintainers** — review and merge code, cut releases, set technical direction. Listed in
  [`.github/CODEOWNERS`](./.github/CODEOWNERS).
- **Contributors** — anyone who opens an issue or PR. No formal commitment required.
- **Scholarly advisors** — qualified individuals who review the religious framing and the
  explanatory text shipped with it.

## Technical decisions

Standard lazy-consensus model:

1. Propose via issue or PR.
2. If no maintainer objects within a reasonable window, it proceeds.
3. Disagreements are resolved by discussion; maintainers have the final call on technical merit.

Larger changes (new engine, new platform, breaking the core API, changing the on-device privacy
model) require an issue tagged `proposal` and explicit maintainer sign-off before implementation.

## Religious / halal-aware decisions

This is the part we take special care with.

- **Sukoon does not issue rulings.** It does not declare what is _halal_ or _haram_. It provides
  _options_ that map to **documented scholarly positions**, each shipped with a short, sourced
  explanation so users can choose informed.
- Any new halal-aware mode, default, or piece of explanatory text must be reviewed by at least
  one scholarly advisor before it ships. The sources must be cited in
  [docs/halal-aware/scholarly-positions.md](./docs/halal-aware/scholarly-positions.md).
- The **default** behaviour (remove all music) is chosen for being the broadest common
  denominator, not as an endorsement of one position over another.
- Disputes about framing are resolved in favour of _presenting multiple positions neutrally_
  rather than picking a side.

If you are a qualified advisor willing to help, please reach out by opening a
[GitHub Discussion](https://github.com/ruwadgroup/sukoon/discussions) (a dedicated contact email will
be added soon).

## Releases

See [RELEASING.md](./RELEASING.md). Releases follow semver; pre-1.0 means minor versions may break.

## Changing this document

Changes to governance are themselves proposals: open an issue tagged `proposal`, allow
discussion, and require maintainer sign-off.
