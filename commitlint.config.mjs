/** @type {import("@commitlint/types").UserConfig} */
export default {
  extends: ["@commitlint/config-conventional"],
  rules: {
    "scope-enum": [
      2,
      "always",
      [
        "core",
        "cli",
        "ui",
        "extension",
        "desktop",
        "mobile",
        "web",
        "cloud",
        "docs",
        "ci",
        "deps",
        "release",
      ],
    ],
    "subject-case": [2, "never", ["upper-case", "pascal-case", "start-case"]],
    "body-max-line-length": [0, "always"],
  },
};
