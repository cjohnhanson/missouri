#!/usr/bin/env node
// Assemble the npm packages: one wrapper plus one package per platform,
// each carrying the binary for that platform.
//
// The version comes from Cargo.toml, so the wrapper and every platform
// package carry the same string and cannot drift. The pins are exact.
// A caret would let a wrapper pair with a binary it was not built
// against.
//
// Run after the release binaries exist:
//   node npm/generate.mjs <short> <cmd> <dir-of-built-binaries>
// where the directory holds <target>/<cmd>, one per target triple.
import { existsSync, readFileSync, writeFileSync, mkdirSync, copyFileSync, chmodSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const argv = process.argv.slice(2);
const partial = argv.includes("--partial");
const [short, cmd, binRoot] = argv.filter((a) => a !== "--partial");
// One scope holds every tool's platform packages.
const SCOPE = "cjohnhanson";
if (!short || !cmd || !binRoot) {
  console.error("usage: generate.mjs <short> <cmd> <dir-of-built-binaries>");
  process.exit(1);
}

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, "..");

const cargo = readFileSync(resolve(repo, "Cargo.toml"), "utf8");
const version = cargo.match(/^version = "([^"]+)"/m)?.[1];
if (!version) {
  console.error("no version in Cargo.toml");
  process.exit(1);
}

const TARGETS = [
  { target: "aarch64-apple-darwin", os: "darwin", cpu: "arm64" },
  { target: "x86_64-apple-darwin", os: "darwin", cpu: "x64" },
  { target: "aarch64-unknown-linux-musl", os: "linux", cpu: "arm64", libc: "musl" },
  { target: "x86_64-unknown-linux-musl", os: "linux", cpu: "x64", libc: "musl" },
];

const wrapperDir = resolve(here, short);
const wrapper = JSON.parse(readFileSync(resolve(wrapperDir, "package.json"), "utf8"));
wrapper.version = version;

// Refuse an incomplete set before writing anything. A wrapper that
// names a platform package which was never built resolves to nothing,
// and the failure lands on whoever installs it rather than here. A
// refusal that came after the first package was written left that
// package on disk. Pass --partial only for a local build of one target.
if (!partial) {
  const missing = TARGETS.map((t) => resolve(binRoot, t.target, cmd)).filter((p) => !existsSync(p));
  if (missing.length > 0) {
    for (const p of missing) console.error(`no binary at ${p}`);
    console.error("build every target first, or pass --partial for a local check");
    process.exit(1);
  }
}

let made = 0;
for (const t of TARGETS) {
  const suffix = t.libc ? `-${t.libc}` : "";
  const name = `${short}-${t.os}-${t.cpu}${suffix}`;
  const src = resolve(binRoot, t.target, cmd);
  if (!existsSync(src)) {
    console.error(`skip ${name}: no binary at ${src}`);
    delete wrapper.optionalDependencies[`@${SCOPE}/${name}`];
    continue;
  }
  const dir = resolve(here, name);
  mkdirSync(dir, { recursive: true });
  const manifest = {
    name: `@${SCOPE}/${name}`,
    version,
    license: wrapper.license,
    // npm publishes with provenance only when the manifest names the
    // public repository the workflow runs in, and the registry checks
    // the match. A platform package without this field fails at the
    // last registry, after crates.io and PyPI hold the version.
    repository: wrapper.repository,
    os: [t.os],
    cpu: [t.cpu],
    // No `libc` field. npm enforces it, and a static musl binary runs
    // on a glibc host too; with the field, `npm install` on Ubuntu
    // refused the package.
  };
  writeFileSync(resolve(dir, "package.json"), `${JSON.stringify(manifest, null, 2)}\n`);
  copyFileSync(src, resolve(dir, cmd));
  // npm does not restore the execute bit.
  chmodSync(resolve(dir, cmd), 0o755);
  wrapper.optionalDependencies[`@${SCOPE}/${name}`] = version;
  made += 1;
}

writeFileSync(resolve(wrapperDir, "package.json"), `${JSON.stringify(wrapper, null, 2)}\n`);
console.log(`${short} ${version}: wrapper + ${made} platform package(s)`);
