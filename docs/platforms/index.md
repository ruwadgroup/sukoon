# Platforms

Each platform is a thin shell. They differ in _how_ they get audio in and out, and _which_ engine
they run.

| Platform                    | Engine                               | Live?   |
| --------------------------- | ------------------------------------ | ------- |
| [Extension](./extension.md) | DeepFilterNet (real-time, on-device) | yes     |
| [Desktop](./desktop.md)     | MDX-Net (local, files)               | files   |
| [Mobile](./mobile.md)       | on-device (planned)                  | pending |
| [Web](./web.md)             | planned                              | no      |

The **extension** is the live path: it runs **DeepFilterNet in real time** in the page, on-device.
True vocal/instrumental separation (**MDX-Net**) is a **file** operation in the
desktop/CLI tools, which call `sukoon-core` directly. The in-browser separators were tried and removed
— see [extension trials](../research/extension-trials.md); a real-time, best-quality separator is the
goal of [Sukoon's own model](../research/own-model-plan.md).

See each page for the build spec. The shared file engine is documented under
[architecture](../architecture/index.md); numbers and the device matrix are in
[reference/performance.md](../reference/performance.md).
