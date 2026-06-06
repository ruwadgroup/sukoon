# Security Policy

## Supported versions

Sukoon is in alpha. Security fixes are applied to `main` and the latest tagged release only.

| Version              | Supported |
| -------------------- | --------- |
| `0.1.x-alpha` (main) | ✅        |
| older pre-releases   | ❌        |

## Reporting a vulnerability

**Please do not open a public issue for security vulnerabilities.**

Report privately through GitHub:

- Open a confidential report via **Security Advisories** → "Report a vulnerability" on the repository.

_(A dedicated security contact email will be added soon.)_

Include: a description, steps to reproduce, affected component (`core`, `cli`, `extension`,
`desktop`), and impact. We aim to acknowledge within 72 hours and to ship a fix or
mitigation plan within 30 days, coordinating disclosure with you.

## Areas of particular concern

Because Sukoon handles user media entirely on-device, we treat these as high severity:

- **Privacy / data retention.** Any code path where audio is uploaded, logged, or cached beyond
  the user's configured retention. Sukoon is fully on-device; flag anything that sends media off
  the machine.
- **Model/asset downloads.** Tampering with model downloads. All assets are verified against a
  checksum in the registry; report any path that skips verification.
- **FFmpeg invocation.** Command injection via filenames or untrusted metadata.

## What is out of scope

- Vulnerabilities in third-party models, FFmpeg, or ONNX Runtime themselves (report upstream,
  but do tell us if Sukoon's usage amplifies them).
- Social-engineering and physical attacks.

Thank you for helping keep Sukoon and its users safe.
