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
const os = require("os");

// The Linux binaries are static musl, which runs on a musl host and on
// a glibc host alike. One Linux entry serves both, and no probe of the
// host libc is needed.
const PLATFORMS = {
	darwin: {
		arm64: "@cjohnhanson/msri-darwin-arm64/missouri",
		x64: "@cjohnhanson/msri-darwin-x64/missouri",
	},
	linux: {
		arm64: "@cjohnhanson/msri-linux-arm64-musl/missouri",
		x64: "@cjohnhanson/msri-linux-x64-musl/missouri",
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
			"installed. Reinstall with `npm install --include=optional`.",
	);
	process.exitCode = 1;
} else if (!bin) {
	console.error(
		`msri ships no prebuilt binary for ${platform} ${arch}. ` +
			"Install it with `cargo install missouri`, or set MSRI_BINARY to a path.",
	);
	process.exitCode = 1;
} else {
	const result = spawnSync(bin, process.argv.slice(2), {
		shell: false,
		stdio: "inherit",
	});
	if (result.error) {
		// A path that is a directory, or a file without the execute bit,
		// fails here. Reported raw it is a ten-frame stack trace naming
		// this file, which tells a reader nothing about their own setup.
		console.error(`msri cannot run ${bin}: ${result.error.message}`);
		process.exitCode = 1;
	} else if (result.signal) {
		// A binary killed by a signal has a null status. Assigning that
		// null exits 0, so a crash reads as a pass to the git hooks and the
		// continuous integration that call this. Report it the way a shell
		// does, as 128 plus the signal number.
		const number = os.constants.signals[result.signal];
		console.error(`msri: ${bin} was killed by ${result.signal}`);
		process.exitCode = number ? 128 + number : 1;
	} else {
		process.exitCode = result.status;
	}
}
