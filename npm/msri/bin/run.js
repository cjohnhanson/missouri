#!/usr/bin/env node
// The binary lives in a per-platform package under optionalDependencies,
// so a package manager installs only the one that matches. This resolves
// it through node's own lookup rather than a hardcoded node_modules path,
// and execs it.
//
// Nothing is downloaded at install time. There is no postinstall, so an
// offline install, an air-gapped runner, and `--ignore-scripts` all work.
const { platform, arch, env } = process;
const { spawnSync } = require("child_process");
const { existsSync } = require("fs");

// The Linux binaries are static musl, which runs on a musl host and on
// a glibc host alike. One Linux entry serves both, and no probe of the
// host libc is needed.
const PLATFORMS = {
  darwin: {
    arm64: "@msri/cli-darwin-arm64/missouri",
    x64: "@msri/cli-darwin-x64/missouri",
  },
  linux: {
    arm64: "@msri/cli-linux-arm64-musl/missouri",
    x64: "@msri/cli-linux-x64-musl/missouri",
  },
};

const rel = env.MSRI_BINARY ? null : PLATFORMS?.[platform]?.[arch];
let bin = env.MSRI_BINARY || null;
let unresolved = false;
if (!bin && rel) {
  // The platform is built, so a failure here means the package did not
  // install. That is a different problem from an unsupported platform
  // and it gets a different message.
  try {
    bin = require.resolve(rel);
  } catch {
    unresolved = true;
  }
}

// An override that does not exist is a reader's typo, not a platform
// they are stuck on. It gets its own message naming the path.
if (bin && !existsSync(bin)) {
  console.error(`MSRI_BINARY is set to ${bin}, and no file is there.`);
  process.exitCode = 1;
} else if (unresolved) {
  console.error(
    `msri supports ${platform} ${arch}, and its binary package is not ` +
      "installed. Reinstall, and if the install skipped optional " +
      "dependencies, allow them."
  );
  process.exitCode = 1;
} else if (!bin) {
  console.error(
    `msri ships no prebuilt binary for ${platform} ${arch}. ` +
      "Install it with `cargo install missouri`, or set MSRI_BINARY to a path."
  );
  process.exitCode = 1;
} else {
  const result = spawnSync(bin, process.argv.slice(2), {
    shell: false,
    stdio: "inherit",
  });
  if (result.error) throw result.error;
  process.exitCode = result.status;
}
