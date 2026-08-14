use std::collections::BTreeMap;
use std::process::Command;
use std::time::{Duration, Instant};

use camino::{Utf8Path, Utf8PathBuf};
use rayon::prelude::*;
use tempfile::TempDir;

use crate::compare::{self, ComparisonResult, OutputDiff};
use crate::error;
use crate::graph::{Assertion, SandboxConfig, StateGraph, StateId, Transition};
use crate::paths::TestPath;

/// Output from executing a command.
#[derive(Debug)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// Helper trait for downcasting trait objects.
pub trait AsAny {
    fn as_any(&self) -> &dyn std::any::Any;
}

impl<T: 'static> AsAny for T {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// Backend for building commands in a specific execution environment.
pub trait Backend: std::fmt::Debug + Send + Sync + AsAny {
    /// Build a Command for a shell command (sh -c "...").
    fn build_shell_command(
        &self,
        command: &str,
        work_dir: &Utf8Path,
        env: &BTreeMap<String, String>,
        path_env: &str,
    ) -> Command;

    /// Build a Command for a direct (non-shell) command.
    fn build_direct_command(
        &self,
        parts: &[&str],
        work_dir: &Utf8Path,
        env: &BTreeMap<String, String>,
        path_env: &str,
    ) -> Command;

    /// Run a command and return its output. The default builds a local
    /// Command and runs it. A backend that runs the command elsewhere, for
    /// example inside a microVM, overrides this method.
    fn execute(
        &self,
        command: &str,
        shell: bool,
        work_dir: &Utf8Path,
        env: &BTreeMap<String, String>,
        path_env: &str,
    ) -> Result<CommandOutput, String> {
        let mut cmd = if shell {
            self.build_shell_command(command, work_dir, env, path_env)
        } else {
            let parts: Vec<&str> = command.split_whitespace().collect();
            if parts.is_empty() {
                return Err("empty command".into());
            }
            self.build_direct_command(&parts, work_dir, env, path_env)
        };
        let output = cmd.output().map_err(|e| format!("failed to execute command: {e}"))?;
        Ok(CommandOutput {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            exit_code: output.status.code().unwrap_or(-1),
        })
    }

    /// True when this backend runs a command inside a Docker container.
    /// Then execute_transition calls execute() instead of
    /// build_*_command().
    fn is_docker(&self) -> bool {
        false
    }

    /// Warm the backend so that parallel calls never compete for shared
    /// state. The default does nothing. NixBackend uses this method to fill
    /// the nix store cache before the paths run at the same time.
    fn warm(&self) -> Result<(), String> {
        Ok(())
    }
}

/// No sandbox. Calls env_clear and builds PATH by hand.
#[derive(Debug)]
pub struct BareBackend;

impl Backend for BareBackend {
    fn build_shell_command(
        &self,
        command: &str,
        work_dir: &Utf8Path,
        env: &BTreeMap<String, String>,
        path_env: &str,
    ) -> Command {
        let mut cmd = Command::new("sh");
        cmd.arg("-c")
            .arg(command)
            .current_dir(work_dir.as_std_path())
            .env_clear()
            .envs(env.iter())
            .env("PATH", path_env);
        cmd
    }

    fn build_direct_command(
        &self,
        parts: &[&str],
        work_dir: &Utf8Path,
        env: &BTreeMap<String, String>,
        path_env: &str,
    ) -> Command {
        let mut cmd = Command::new(parts[0]);
        cmd.args(&parts[1..])
            .current_dir(work_dir.as_std_path())
            .env_clear()
            .envs(env.iter())
            .env("PATH", path_env);
        cmd
    }
}

/// Nix shell sandbox. Each command runs inside
/// `nix shell nixpkgs#pkg1 ... --command`.
///
/// During warm-up, this backend resolves `nixpkgs` to a pinned flake URL
/// that holds a commit hash. Every later command then uses
/// `--no-use-registries`. Parallel paths therefore never compete for the
/// flake registry file.
///
/// The flag must be the non-deprecated form. The deprecated
/// `--no-registries` makes nix print a deprecation warning on stderr,
/// and that warning merges into the stderr of the command under test,
/// which breaks every stderr assertion in a suite that declares
/// packages.
#[derive(Debug)]
pub struct NixBackend {
    /// Absolute path to the `nix` binary.
    pub nix_bin: Utf8PathBuf,
    /// Package names to provide via nixpkgs.
    pub packages: Vec<String>,
    /// Pinned nixpkgs flake URL (resolved during warm-up).
    /// When set, used instead of bare `nixpkgs` to avoid registry lookups.
    pinned_nixpkgs: Option<String>,
}

impl NixBackend {
    fn nix_prefix_args(&self) -> Vec<String> {
        let mut args: Vec<String> = vec!["shell".into()];
        args.push("--extra-experimental-features".into());
        args.push("nix-command flakes".into());
        if self.pinned_nixpkgs.is_some() {
            args.push("--no-use-registries".into());
        }
        let flake_ref = self.pinned_nixpkgs.as_deref().unwrap_or("nixpkgs");
        for pkg in &self.packages {
            args.push(format!("{flake_ref}#{pkg}"));
        }
        args.push("--command".into());
        args
    }

    /// Resolve nixpkgs to a pinned flake URL. Then run a command that does
    /// nothing, to fill the nix store. After that, every parallel call uses
    /// the pinned URL with `--no-use-registries` and never reads the flake
    /// registry.
    fn warm_cache(&mut self) -> Result<(), String> {
        // Resolve nixpkgs → pinned URL with commit hash
        let output = std::process::Command::new(self.nix_bin.as_str())
            .args([
                "flake",
                "metadata",
                "nixpkgs",
                "--json",
                "--extra-experimental-features",
                "nix-command flakes",
            ])
            .output()
            .map_err(|e| format!("failed to resolve nixpkgs: {e}"))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("failed to resolve nixpkgs: {stderr}"));
        }
        let metadata: serde_json::Value = serde_json::from_slice(&output.stdout)
            .map_err(|e| format!("failed to parse nixpkgs metadata: {e}"))?;
        let url = metadata["url"]
            .as_str()
            .ok_or_else(|| "nixpkgs metadata missing 'url' field".to_string())?;
        self.pinned_nixpkgs = Some(url.to_string());

        // Run a no-op to ensure packages are cached
        let mut args = self.nix_prefix_args();
        args.push("true".into());
        let output = std::process::Command::new(self.nix_bin.as_str())
            .args(&args)
            .output()
            .map_err(|e| format!("failed to warm nix cache: {e}"))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("nix cache warm failed: {stderr}"));
        }
        Ok(())
    }
}

impl Backend for NixBackend {
    fn warm(&self) -> Result<(), String> {
        // warm_cache requires &mut self, so it's called from detect_sandbox instead
        Ok(())
    }

    fn build_shell_command(
        &self,
        command: &str,
        work_dir: &Utf8Path,
        env: &BTreeMap<String, String>,
        path_env: &str,
    ) -> Command {
        // nix shell adds the nix packages to PATH. Prepend the project and
        // state bin directories only, not the system PATH. The nix packages
        // then take priority over the system binaries, and the project
        // wrappers still win. Take the non-system prefix from path_env.
        let system_path =
            std::env::var("PATH").unwrap_or_else(|_| "/usr/local/bin:/usr/bin:/bin".into());
        let project_dirs: String = path_env
            .split(':')
            .filter(|dir| !system_path.split(':').any(|sys| sys == *dir))
            .collect::<Vec<_>>()
            .join(":");
        let wrapped = if project_dirs.is_empty() {
            command.to_string()
        } else {
            format!("export PATH=\"{project_dirs}:$PATH\"; {command}")
        };
        let mut args = self.nix_prefix_args();
        args.extend(["sh".into(), "-c".into(), wrapped]);
        let mut cmd = Command::new(self.nix_bin.as_str());
        cmd.args(&args)
            .current_dir(work_dir.as_std_path())
            .env_clear()
            .envs(env.iter())
            .env("PATH", path_env);
        cmd
    }

    fn build_direct_command(
        &self,
        parts: &[&str],
        work_dir: &Utf8Path,
        env: &BTreeMap<String, String>,
        path_env: &str,
    ) -> Command {
        // nix shell overrides PATH, so wrap in sh with explicit PATH export.
        let inner = parts.iter().map(|s| shell_escape(s)).collect::<Vec<_>>().join(" ");
        let wrapped = format!("export PATH=\"{path_env}:$PATH\"; {inner}");
        let mut args = self.nix_prefix_args();
        args.extend(["sh".into(), "-c".into(), wrapped]);
        let mut cmd = Command::new(self.nix_bin.as_str());
        cmd.args(&args)
            .current_dir(work_dir.as_std_path())
            .env_clear()
            .envs(env.iter())
            .env("PATH", path_env);
        cmd
    }
}

/// Escape a string for safe inclusion in a shell command.
fn shell_escape(s: &str) -> String {
    if s.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '/' || c == '.') {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

/// Docker backend. Each transition runs inside a Docker container. The
/// container has no network access (`network_mode: "none"`), and bollard
/// mounts the volumes.
///
/// Each transition gets a fresh container. Missouri removes the container
/// after the command finishes. The `execute` method overrides the default
/// and runs the command inside the container. Nothing calls the
/// `build_*_command` methods.
#[derive(Debug)]
pub struct DockerBackend {
    image: String,
}

/// Default Docker image for missouri test containers.
const DEFAULT_DOCKER_IMAGE: &str = "debian:bookworm-slim";

/// Image for a transition that replays network traffic. It must hold
/// mitmdump and iptables. It must also hold the mitmproxy CA in the system
/// trust store.
const MITM_DOCKER_IMAGE: &str = "mitm-test";

impl DockerBackend {
    /// Build a Docker image from a Dockerfile in the given directory.
    /// Returns the image tag.
    ///
    /// The user owns the Dockerfile. It can use nix, apt, or another tool
    /// to set up the environment. Missouri builds the Dockerfile and caches
    /// the result by content hash. It changes nothing else.
    async fn build_image_from_dockerfile(
        &self,
        dockerfile_dir: &Utf8Path,
        docker: &bollard::Docker,
    ) -> Result<String, String> {
        use bollard::image::BuildImageOptions;
        use futures_util::StreamExt;

        let dockerfile_path = dockerfile_dir.join("Dockerfile");
        let dockerfile_content = std::fs::read_to_string(&dockerfile_path)
            .map_err(|e| format!("failed to read {dockerfile_path}: {e}"))?;

        // Hash Dockerfile + all files in the directory for a stable image tag
        let hash = format!("{:x}", md5_hash(dockerfile_content.as_bytes()));
        let image_tag = format!("missouri:{hash}");

        // Check if image already exists
        if docker.inspect_image(&image_tag).await.is_ok() {
            return Ok(image_tag);
        }

        eprintln!("missouri: building Docker image from {dockerfile_path} (first build may take several minutes)...");

        // Create a tar archive of the directory (Dockerfile + context)
        let tar_data = create_build_context(dockerfile_dir)
            .map_err(|e| format!("failed to create build context: {e}"))?;

        let options = BuildImageOptions {
            t: image_tag.as_str(),
            rm: true,
            ..Default::default()
        };

        let mut stream = docker.build_image(options, None, Some(tar_data.into()));

        while let Some(result) = stream.next().await {
            match result {
                Ok(info) => {
                    if let Some(err) = info.error {
                        return Err(format!("docker build error: {err}"));
                    }
                }
                Err(e) => return Err(format!("docker build failed: {e}")),
            }
        }

        Ok(image_tag)
    }

    /// Run a command inside a Docker container with network isolation and
    /// volume mounting, then capture stdout/stderr/exit_code.
    ///
    /// When `replay_flow` is set, the container uses the mitmproxy image and
    /// intercepts traffic transparently. iptables redirects outbound port 80
    /// and port 443 to mitmdump. mitmdump then serves the recorded responses
    /// from the flow file. The process under test needs no proxy environment
    /// variable and no application configuration.
    fn run_in_container(
        &self,
        command: &str,
        work_dir: &Utf8Path,
        env: &BTreeMap<String, String>,
        replay_flow: Option<&Utf8Path>,
        replay_hosts: &[String],
        dockerfile_dir: Option<&Utf8Path>,
    ) -> Result<CommandOutput, String> {
        use bollard::container::{Config, CreateContainerOptions, RemoveContainerOptions};
        use bollard::models::HostConfig;
        use bollard::Docker;

        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| format!("failed to create tokio runtime: {e}"))?;

        rt.block_on(async {
            let docker = Docker::connect_with_local_defaults()
                .map_err(|e| format!("failed to connect to Docker: {e}"))?;

            // Build env vars as "KEY=VALUE" strings
            let env_vec: Vec<String> = env.iter().map(|(k, v)| format!("{k}={v}")).collect();

            // Determine the Docker image to use:
            // 1. If replay is active, use the mitmproxy image
            // 2. If a Dockerfile exists, build it
            // 3. Otherwise, use the configured default image (or docker_image from config)
            let built_image: Option<String> = if let Some(dir) = dockerfile_dir {
                Some(self.build_image_from_dockerfile(dir, &docker).await?)
            } else {
                None
            };
            let image: &str = if replay_flow.is_some() {
                MITM_DOCKER_IMAGE
            } else if let Some(ref bi) = built_image {
                bi.as_str()
            } else {
                self.image.as_str()
            };

            let mut binds = vec![format!("{}:/work", work_dir)];
            if let Some(flow) = replay_flow {
                binds.push(format!("{}:/replay.flow:ro", flow));
            }

            let host_config = HostConfig {
                network_mode: Some("none".to_string()),
                binds: Some(binds),
                cap_add: if replay_flow.is_some() {
                    Some(vec!["NET_ADMIN".to_string()])
                } else {
                    None
                },
                ..Default::default()
            };

            let config = Config {
                image: Some(image),
                cmd: Some(vec!["sleep", "300"]),
                working_dir: Some("/work"),
                env: Some(env_vec.iter().map(|s| s.as_str()).collect()),
                host_config: Some(host_config),
                ..Default::default()
            };

            let container = docker
                .create_container(
                    Some(CreateContainerOptions {
                        name: "",
                        platform: None,
                    }),
                    config,
                )
                .await
                .map_err(|e| format!("failed to create container: {e}"))?;

            let container_id = container.id;

            docker
                .start_container::<String>(&container_id, None)
                .await
                .map_err(|e| format!("failed to start container: {e}"))?;


            // If replay mode, set up transparent interception inside the container:
            // 1. Write /etc/hosts entries so hostnames resolve to 127.0.0.1
            // 2. iptables redirect outbound 80/443 → mitmdump (excluding mitmuser)
            // 3. Start mitmdump in transparent replay mode (detached exec)
            if replay_flow.is_some() {
                // Step 1+2: /etc/hosts and iptables (returns immediately)
                let mut setup_parts: Vec<String> = replay_hosts
                    .iter()
                    .map(|h| format!("echo '127.0.0.1 {h}' >> /etc/hosts"))
                    .collect();
                setup_parts.push(
                    "MITMUSER_UID=$(id -u mitmuser) && \
                    iptables -t nat -A OUTPUT -p tcp --dport 80 -m owner ! --uid-owner $MITMUSER_UID -j REDIRECT --to-port 18080 && \
                    iptables -t nat -A OUTPUT -p tcp --dport 443 -m owner ! --uid-owner $MITMUSER_UID -j REDIRECT --to-port 18080"
                        .to_string(),
                );
                self.exec_in_container(&docker, &container_id, &setup_parts.join(" && "))
                    .await?;

                // Step 3: start mitmdump as a detached exec (doesn't block)
                self.exec_detached(
                    &docker,
                    &container_id,
                    "mitmuser",
                    &[
                        "mitmdump", "--mode", "transparent", "-p", "18080",
                        "--server-replay", "/replay.flow",
                        "--set", "connection_strategy=lazy",
                        "--set", "upstream_cert=false",
                        "--set", "server_replay_reuse=true",
                        "--set", "server_replay_extra=kill",
                        "--set", "confdir=/home/mitmuser/.mitmproxy",
                        "-q",
                    ],
                )
                .await?;

                // Wait for mitmdump to bind port
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }

            // Run the actual command
            let (stdout, stderr, exit_code) =
                self.exec_capture(&docker, &container_id, command).await?;

            // Stop and remove container
            let _ = docker.stop_container(&container_id, None).await;
            let _ = docker
                .remove_container(
                    &container_id,
                    Some(RemoveContainerOptions {
                        force: true,
                        ..Default::default()
                    }),
                )
                .await;

            Ok(CommandOutput {
                stdout,
                stderr,
                exit_code,
            })
        })
    }

    /// Execute a command inside a container without capturing output (for setup).
    async fn exec_in_container(
        &self,
        docker: &bollard::Docker,
        container_id: &str,
        command: &str,
    ) -> Result<(), String> {
        use bollard::exec::{CreateExecOptions, StartExecResults};
        use futures_util::StreamExt;

        let exec = docker
            .create_exec(
                container_id,
                CreateExecOptions {
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    cmd: Some(vec!["sh", "-c", command]),
                    ..Default::default()
                },
            )
            .await
            .map_err(|e| format!("failed to create exec: {e}"))?;

        if let StartExecResults::Attached { mut output, .. } = docker
            .start_exec(&exec.id, None)
            .await
            .map_err(|e| format!("failed to start exec: {e}"))?
        {
            while output.next().await.is_some() {} // drain
        }

        let inspect = docker
            .inspect_exec(&exec.id)
            .await
            .map_err(|e| format!("failed to inspect exec: {e}"))?;

        if inspect.exit_code != Some(0) {
            return Err(format!(
                "setup command failed with exit code {:?}",
                inspect.exit_code
            ));
        }
        Ok(())
    }

    /// Start a long-running process inside a container and do not wait for
    /// it. Missouri uses this for a background service such as mitmdump.
    async fn exec_detached(
        &self,
        docker: &bollard::Docker,
        container_id: &str,
        user: &str,
        cmd: &[&str],
    ) -> Result<(), String> {
        use bollard::exec::{CreateExecOptions, StartExecOptions};

        let exec = docker
            .create_exec(
                container_id,
                CreateExecOptions {
                    cmd: Some(cmd.to_vec()),
                    user: Some(user),
                    ..Default::default()
                },
            )
            .await
            .map_err(|e| format!("failed to create detached exec: {e}"))?;

        docker
            .start_exec(&exec.id, Some(StartExecOptions { detach: true, ..Default::default() }))
            .await
            .map_err(|e| format!("failed to start detached exec: {e}"))?;

        Ok(())
    }

    /// Execute a command inside a container and capture stdout/stderr/exit_code.
    async fn exec_capture(
        &self,
        docker: &bollard::Docker,
        container_id: &str,
        command: &str,
    ) -> Result<(String, String, i32), String> {
        use bollard::exec::{CreateExecOptions, StartExecResults};
        use futures_util::StreamExt;

        let exec = docker
            .create_exec(
                container_id,
                CreateExecOptions {
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    cmd: Some(vec!["sh", "-c", command]),
                    ..Default::default()
                },
            )
            .await
            .map_err(|e| format!("failed to create exec: {e}"))?;

        let mut stdout = String::new();
        let mut stderr = String::new();

        if let StartExecResults::Attached { mut output, .. } = docker
            .start_exec(&exec.id, None)
            .await
            .map_err(|e| format!("failed to start exec: {e}"))?
        {
            while let Some(msg) = output.next().await {
                match msg {
                    Ok(bollard::container::LogOutput::StdOut { message }) => {
                        stdout.push_str(&String::from_utf8_lossy(&message));
                    }
                    Ok(bollard::container::LogOutput::StdErr { message }) => {
                        stderr.push_str(&String::from_utf8_lossy(&message));
                    }
                    Err(e) => {
                        stderr.push_str(&format!("exec stream error: {e}"));
                    }
                    _ => {}
                }
            }
        }

        let exec_inspect = docker
            .inspect_exec(&exec.id)
            .await
            .map_err(|e| format!("failed to inspect exec: {e}"))?;

        let exit_code = exec_inspect.exit_code.map(|c| c as i32).unwrap_or(-1);

        Ok((stdout, stderr, exit_code))
    }
}

impl Backend for DockerBackend {
    fn build_shell_command(
        &self,
        _command: &str,
        _work_dir: &Utf8Path,
        _env: &BTreeMap<String, String>,
        _path_env: &str,
    ) -> Command {
        unreachable!("DockerBackend uses execute() instead of build_shell_command()")
    }

    fn build_direct_command(
        &self,
        _parts: &[&str],
        _work_dir: &Utf8Path,
        _env: &BTreeMap<String, String>,
        _path_env: &str,
    ) -> Command {
        unreachable!("DockerBackend uses execute() instead of build_direct_command()")
    }

    fn execute(
        &self,
        command: &str,
        _shell: bool,
        work_dir: &Utf8Path,
        env: &BTreeMap<String, String>,
        _path_env: &str,
    ) -> Result<CommandOutput, String> {
        self.run_in_container(command, work_dir, env, None, &[], None)
    }

    fn is_docker(&self) -> bool {
        true // reuses the same execute_transition path
    }
}

/// Detect and prepare backend from project-level config.
///
/// Reads `graph.sandbox_config` to determine the backend:
/// - `SandboxConfig::None` → `BareBackend`
/// - `SandboxConfig::Packages(pkgs)` → `NixBackend` (or `BareBackend` if preinstalled)
/// - `SandboxConfig::Docker` → `DockerBackend`
///
/// When `MISSOURI_SANDBOX=preinstalled` is set, a packages config resolves
/// to `BareBackend`. Missouri then assumes that every tool is already on
/// PATH. This happens inside a nix derivation where the packages are
/// `nativeCheckInputs`.
pub fn detect_sandbox(graph: &StateGraph) -> error::Result<Box<dyn Backend>> {
    // Check for preinstalled override
    if std::env::var("MISSOURI_SANDBOX").ok().as_deref() == Some("preinstalled") {
        return Ok(Box::new(BareBackend));
    }

    match &graph.sandbox_config {
        SandboxConfig::None => Ok(Box::new(BareBackend)),
        SandboxConfig::Packages(packages) => {
            let nix_bin = which_nix().ok_or_else(|| error::Error::NixNotFound {
                root: graph.root.clone(),
            })?;
            let mut backend = NixBackend {
                nix_bin,
                packages: packages.clone(),
                pinned_nixpkgs: None,
            };
            backend
                .warm_cache()
                .map_err(|msg| error::Error::SandboxWarm { message: msg })?;
            Ok(Box::new(backend))
        }
        SandboxConfig::Docker { image } => Ok(Box::new(DockerBackend {
            image: image.as_deref().unwrap_or(DEFAULT_DOCKER_IMAGE).to_string(),
        })),
    }
}

/// Resolve the absolute path to `nix` from the current process's PATH.
fn which_nix() -> Option<Utf8PathBuf> {
    let path_var = std::env::var("PATH").ok()?;
    for dir in path_var.split(':') {
        let candidate = Utf8PathBuf::from(dir).join("nix");
        if candidate.exists() {
            return Some(candidate);
        }
    }
    std::option::Option::None
}

/// Build the extra environment variables needed for mitmproxy interception.
///
/// Sets:
/// - `HTTPS_PROXY` / `HTTP_PROXY` → `http://127.0.0.1:{port}`
/// - `NODE_EXTRA_CA_CERTS` → path to the mitmproxy CA certificate
pub fn build_network_env(port: u16) -> BTreeMap<String, String> {
    let proxy = format!("http://127.0.0.1:{port}");
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
    let ca_cert = format!("{home}/.mitmproxy/mitmproxy-ca-cert.pem");
    let mut env = BTreeMap::new();
    env.insert("HTTPS_PROXY".into(), proxy.clone());
    env.insert("HTTP_PROXY".into(), proxy);
    env.insert("NODE_EXTRA_CA_CERTS".into(), ca_cert);
    env
}

/// Start mitmdump in server-replay mode using the given flow file.
///
/// `path_env` is the PATH to search for the `mitmdump` binary.
/// Returns a `MitmdumpHandle` that holds the port it found. The handle
/// kills the process on drop. Returns an error string when mitmdump is
/// missing, fails to start, or prints no listening port.
pub fn start_mitmdump_replay(
    flow: &camino::Utf8Path,
    path_env: &str,
) -> Result<MitmdumpHandle, String> {
    // Find mitmdump on the given PATH
    let mitmdump_bin = path_env
        .split(':')
        .map(|dir| camino::Utf8PathBuf::from(dir).join("mitmdump"))
        .find(|p| p.exists())
        .ok_or_else(|| {
            format!(
                "mitmdump not found on PATH — add mitmproxy to packages or install it manually"
            )
        })?;

    let mut child = std::process::Command::new(mitmdump_bin.as_str())
        .args([
            "--server-replay",
            flow.as_str(),
            "--listen-port",
            "0",
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to start mitmdump: {e}"))?;

    // Read stderr lines until we find the port announcement.
    // Format: "Proxy server listening at http://*:PORT"
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "failed to capture mitmdump stderr".to_string())?;
    let reader = std::io::BufReader::new(stderr);
    use std::io::BufRead;
    let mut port: Option<u16> = None;
    for line in reader.lines() {
        let line = line.map_err(|e| format!("failed to read mitmdump stderr: {e}"))?;
        if let Some(p) = parse_mitmdump_port(&line) {
            port = Some(p);
            break;
        }
    }

    let port = port.ok_or_else(|| {
        let _ = child.kill();
        let _ = child.wait();
        "mitmdump exited without announcing a listening port".to_string()
    })?;

    Ok(MitmdumpHandle { child, port })
}

/// Start mitmdump in record mode, writing captured traffic to `output`.
///
/// `path_env` is the PATH to search for the `mitmdump` binary.
/// Returns a `MitmdumpHandle` that holds the port it found. The handle
/// kills the process on drop. Returns an error string when mitmdump is
/// missing, fails to start, or prints no listening port.
pub fn start_mitmdump_record(
    output: &camino::Utf8Path,
    path_env: &str,
) -> Result<MitmdumpHandle, String> {
    let mitmdump_bin = path_env
        .split(':')
        .map(|dir| camino::Utf8PathBuf::from(dir).join("mitmdump"))
        .find(|p| p.exists())
        .ok_or_else(|| {
            "mitmdump not found on PATH — add mitmproxy to packages or install it manually"
                .to_string()
        })?;

    let mut child = std::process::Command::new(mitmdump_bin.as_str())
        .args(["-w", output.as_str(), "--listen-port", "0"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to start mitmdump: {e}"))?;

    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "failed to capture mitmdump stderr".to_string())?;
    let reader = std::io::BufReader::new(stderr);
    use std::io::BufRead;
    let mut port: Option<u16> = None;
    for line in reader.lines() {
        let line = line.map_err(|e| format!("failed to read mitmdump stderr: {e}"))?;
        if let Some(p) = parse_mitmdump_port(&line) {
            port = Some(p);
            break;
        }
    }

    let port = port.ok_or_else(|| {
        let _ = child.kill();
        let _ = child.wait();
        "mitmdump exited without announcing a listening port".to_string()
    })?;

    Ok(MitmdumpHandle { child, port })
}

/// Parse the listening port from a mitmdump stderr line.
/// Matches lines like "Proxy server listening at http://*:8080".
fn parse_mitmdump_port(line: &str) -> Option<u16> {
    if line.contains("listening at") {
        // Extract port from the end: "http://*:PORT" or "http://0.0.0.0:PORT"
        line.rsplit(':').next()?.trim().parse().ok()
    } else {
        None
    }
}

/// RAII handle for a running mitmdump process. Kills the process on drop.
#[derive(Debug)]
pub struct MitmdumpHandle {
    child: std::process::Child,
    pub port: u16,
}

impl Drop for MitmdumpHandle {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Default regex pattern for extracting port from service stderr.
const DEFAULT_PORT_PATTERN: &str = r"listening.*:(\d+)";

/// RAII handle for a running background service. Sends SIGTERM then SIGKILL on drop.
#[derive(Debug)]
pub struct ServiceHandle {
    child: std::process::Child,
    pub port: u16,
    /// Thread draining stderr to prevent pipe buffer blocking.
    _stderr_drain: Option<std::thread::JoinHandle<()>>,
}

impl Drop for ServiceHandle {
    fn drop(&mut self) {
        let pid = self.child.id() as i32;
        // SIGTERM the process group
        unsafe { libc::kill(-pid, libc::SIGTERM) };
        std::thread::sleep(Duration::from_millis(100));
        // SIGKILL if still alive
        if let Ok(None) = self.child.try_wait() {
            unsafe { libc::kill(-pid, libc::SIGKILL) };
        }
        let _ = self.child.wait();
    }
}

/// Start a background service, capture its port from stderr, and return a handle.
///
/// The service command starts in its own process group, through
/// `process_group(0)`. A drop can then kill the whole process tree.
///
/// After the port is captured, a drain thread keeps reading stderr. This
/// stops the service from blocking on a full pipe buffer.
pub fn start_service(
    config: &crate::config::ServiceConfig,
    work_dir: &Utf8Path,
    env: &BTreeMap<String, String>,
    path_env: &str,
    sandbox: &dyn Backend,
) -> Result<ServiceHandle, String> {
    use std::io::BufRead;
    use std::os::unix::process::CommandExt;

    let mut cmd = if config.shell {
        sandbox.build_shell_command(&config.command, work_dir, env, path_env)
    } else {
        let parts: Vec<&str> = config.command.split_whitespace().collect();
        if parts.is_empty() {
            return Err("empty service command".into());
        }
        sandbox.build_direct_command(&parts, work_dir, env, path_env)
    };

    cmd.stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .process_group(0);

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("failed to start service '{}': {e}", config.command))?;

    // Parse port from stderr
    let pattern_str = config
        .port_pattern
        .as_deref()
        .unwrap_or(DEFAULT_PORT_PATTERN);
    let pattern = regex::Regex::new(pattern_str)
        .map_err(|e| format!("invalid port_pattern '{pattern_str}': {e}"))?;

    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "failed to capture service stderr".to_string())?;
    let reader = std::io::BufReader::new(stderr);
    let mut port: Option<u16> = None;
    let mut lines = reader.lines();

    // Read stderr lines until port is found (with timeout)
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        match lines.next() {
            Some(Ok(line)) => {
                if let Some(caps) = pattern.captures(&line) {
                    if let Some(m) = caps.get(1) {
                        if let Ok(p) = m.as_str().parse::<u16>() {
                            port = Some(p);
                            break;
                        }
                    }
                }
            }
            Some(Err(e)) => {
                return Err(format!("failed to read service stderr: {e}"));
            }
            None => {
                // Service exited before announcing port
                let _ = child.wait();
                return Err(format!(
                    "service '{}' exited before announcing a port",
                    config.command
                ));
            }
        }
    }

    let port = port.ok_or_else(|| {
        let pid = child.id() as i32;
        unsafe { libc::kill(-pid, libc::SIGKILL) };
        let _ = child.wait();
        format!(
            "service '{}' did not announce port within 30s",
            config.command
        )
    })?;

    // Spawn drain thread for remaining stderr
    let drain = std::thread::spawn(move || {
        for line in lines {
            let _ = line; // just drain
        }
    });

    Ok(ServiceHandle {
        child,
        port,
        _stderr_drain: Some(drain),
    })
}

/// Build environment variables for service ports.
///
/// Single service: sets `PORT`.
/// Multiple services: sets `PORT_0`, `PORT_1`, etc. Also sets `PORT` = first port.
fn build_service_env(ports: &[u16]) -> BTreeMap<String, String> {
    let mut env = BTreeMap::new();
    match ports.len() {
        0 => {}
        1 => {
            env.insert("PORT".into(), ports[0].to_string());
        }
        _ => {
            env.insert("PORT".into(), ports[0].to_string());
            for (i, port) in ports.iter().enumerate() {
                env.insert(format!("PORT_{i}"), port.to_string());
            }
        }
    }
    env
}

/// Run a readiness check command with exponential backoff.
/// Retries up to 10 times with 100ms, 200ms, 400ms, ... delays (max 5s each).
fn run_ready_check(
    command: &str,
    work_dir: &Utf8Path,
    env: &BTreeMap<String, String>,
    path_env: &str,
    sandbox: &dyn Backend,
) -> Result<(), String> {
    let mut delay = Duration::from_millis(100);
    let max_attempts = 10;

    for attempt in 0..max_attempts {
        let output = crate::signal::run_tracked(
            &mut sandbox.build_shell_command(command, work_dir, env, path_env),
        );
        match output {
            Ok(o) if o.status.success() => return Ok(()),
            _ if attempt == max_attempts - 1 => {
                return Err(format!(
                    "service ready check '{command}' failed after {max_attempts} attempts",
                ));
            }
            _ => {
                std::thread::sleep(delay);
                delay = std::cmp::min(delay * 2, Duration::from_secs(5));
            }
        }
    }
    unreachable!()
}

/// Start all services from a config list. Returns handles (for RAII drop)
/// and the port environment variables to inject.
fn start_services(
    services: &[crate::config::ServiceConfig],
    work_dir: &Utf8Path,
    env: &BTreeMap<String, String>,
    path_env: &str,
    sandbox: &dyn Backend,
) -> Result<(Vec<ServiceHandle>, BTreeMap<String, String>), String> {
    let mut handles = Vec::new();
    let mut ports = Vec::new();

    for svc in services {
        // Include previously assigned ports in the env for this service
        let mut svc_env = env.clone();
        svc_env.extend(build_service_env(&ports));

        let handle = start_service(svc, work_dir, &svc_env, path_env, sandbox)?;
        ports.push(handle.port);

        // Run ready check if specified
        if let Some(ready_cmd) = &svc.ready {
            let mut ready_env = env.clone();
            ready_env.extend(build_service_env(&ports));
            run_ready_check(ready_cmd, work_dir, &ready_env, path_env, sandbox)?;
        }

        handles.push(handle);
    }

    let port_env = build_service_env(&ports);
    Ok((handles, port_env))
}

/// Build the PATH env var: state bin/ → project bin/ → base path.
fn build_path_env(
    state_bin: Option<&Utf8Path>,
    project_bin: Option<&Utf8Path>,
    base_path: &str,
) -> String {
    let mut parts: Vec<&str> = Vec::new();
    if let Some(sb) = state_bin {
        parts.push(sb.as_str());
    }
    if let Some(pb) = project_bin {
        parts.push(pb.as_str());
    }
    parts.push(base_path);
    parts.join(":")
}

/// How assertions interact with the run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckMode {
    /// Run transitions + filesystem comparison + output assertions + state assertions.
    Full,
    /// Run only state assertions (no transitions, no filesystem comparison).
    CheckOnly,
    /// Run transitions + filesystem comparison, skip all assertions.
    NoCheck,
}

/// Result of running a single state assertion.
#[derive(Debug)]
pub struct AssertionResult {
    pub name: String,
    pub passed: bool,
    pub exit_code: Option<i32>,
    pub stdout_diff: Option<(String, String)>,
    pub stderr_diff: Option<(String, String)>,
    pub error: Option<String>,
    pub duration: Duration,
}

/// Result of executing a single transition.
#[derive(Debug)]
pub struct StepResult {
    pub transition_name: String,
    pub source_name: String,
    pub target_name: String,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub comparison: Option<ComparisonResult>,
    pub output_diffs: Vec<OutputDiff>,
    pub assertion_results: Vec<AssertionResult>,
    pub passed: bool,
    pub duration: Duration,
}

/// Result of executing a full test path.
#[derive(Debug)]
pub struct PathResult {
    pub path_display: String,
    pub steps: Vec<StepResult>,
    pub passed: bool,
    pub duration: Duration,
}

/// Configuration for recording transition output.
#[derive(Debug, Clone)]
pub struct RecordingConfig {
    /// Base output directory for recordings (e.g. `<root>/<config_dir>/runs/<run_id>/`).
    pub output_dir: Utf8PathBuf,
    /// The run ID.
    pub run_id: String,
}

/// Options for test execution.
pub struct RunOptions {
    pub keep_temp: bool,
    pub verbose: bool,
    pub sandbox: Box<dyn Backend>,
    pub check_mode: CheckMode,
    /// If set, record transition output to .cast files.
    pub recording: Option<RecordingConfig>,
}

/// Result of running a single setup command.
#[derive(Debug)]
pub struct SetupResult {
    pub name: String,
    pub passed: bool,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

/// Run setup commands before test paths. Returns results and whether all passed.
/// Setup commands always run on the host (BareBackend), never inside a sandbox —
/// they're for building binaries, initializing state, etc.
pub fn run_setup_phase(graph: &StateGraph, _opts: &RunOptions) -> Vec<SetupResult> {
    let base_path = std::env::var("PATH").unwrap_or_else(|_| "/usr/local/bin:/usr/bin:/bin".into());
    let path_env = build_path_env(None, graph.project_bin.as_deref(), &base_path);
    let bare = BareBackend;

    graph
        .setup
        .iter()
        .scan(true, |still_passing, cmd| {
            if !*still_passing || crate::signal::is_interrupted() {
                return None; // stop after first failure or interruption
            }
            let result = run_single_setup(
                cmd,
                &graph.project_root,
                &path_env,
                &graph.project_env,
                &bare,
            );
            if !result.passed {
                *still_passing = false;
            }
            Some(result)
        })
        .collect()
}

/// Run a single setup command.
fn run_single_setup(
    cmd: &crate::graph::SetupCommand,
    work_dir: &Utf8Path,
    path_env: &str,
    project_env: &std::collections::BTreeMap<String, String>,
    sandbox: &dyn Backend,
) -> SetupResult {
    let output = if cmd.shell {
        crate::signal::run_tracked(&mut sandbox.build_shell_command(
            &cmd.command,
            work_dir,
            project_env,
            path_env,
        ))
    } else {
        let parts: Vec<&str> = cmd.command.split_whitespace().collect();
        if parts.is_empty() {
            return SetupResult {
                name: cmd.name.clone(),
                passed: false,
                exit_code: None,
                stdout: String::new(),
                stderr: "empty command".into(),
            };
        }
        crate::signal::run_tracked(&mut sandbox.build_direct_command(
            &parts,
            work_dir,
            project_env,
            path_env,
        ))
    };

    match output {
        Ok(o) => {
            let exit_code = o.status.code();
            SetupResult {
                name: cmd.name.clone(),
                passed: o.status.success(),
                exit_code,
                stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
            }
        }
        Err(e) => SetupResult {
            name: cmd.name.clone(),
            passed: false,
            exit_code: None,
            stdout: String::new(),
            stderr: format!("failed to execute command: {e}"),
        },
    }
}

/// Progress events emitted during test execution.
pub enum ProgressEvent<'a> {
    /// A path is about to start executing.
    PathStarted {
        index: usize,
        total: usize,
        display: &'a str,
    },
    /// A path finished executing.
    PathFinished { index: usize, passed: bool },
    /// Execution was interrupted by a signal.
    Interrupted,
}

/// Execute all test paths in parallel and return results.
pub fn run_all_paths(
    graph: &StateGraph,
    paths: &[TestPath],
    opts: &RunOptions,
    on_progress: Option<&(dyn Fn(ProgressEvent) + Sync)>,
) -> Vec<PathResult> {

    let total = paths.len();

    let results: Vec<PathResult> = paths
        .par_iter()
        .enumerate()
        .map(|(path_idx, path)| {
            if crate::signal::is_interrupted() {
                return PathResult {
                    path_display: path.display(graph),
                    steps: Vec::new(),
                    passed: false,
                    duration: Duration::ZERO,
                };
            }

            let display = path.display(graph);
            if let Some(cb) = on_progress {
                cb(ProgressEvent::PathStarted {
                    index: path_idx,
                    total,
                    display: &display,
                });
            }
            let result = run_path(graph, path, opts, path_idx);
            if let Some(cb) = on_progress {
                cb(ProgressEvent::PathFinished {
                    index: path_idx,
                    passed: result.passed,
                });
            }
            result
        })
        .collect();

    if crate::signal::is_interrupted()
        && let Some(cb) = on_progress
    {
        cb(ProgressEvent::Interrupted);
    }

    results
}

/// Execute a single test path.
fn run_path(graph: &StateGraph, path: &TestPath, opts: &RunOptions, path_idx: usize) -> PathResult {
    let path_display = path.display(graph);
    let start = Instant::now();

    let mut result = match opts.check_mode {
        CheckMode::CheckOnly => run_path_check_only(graph, path, path_display, opts),
        CheckMode::Full | CheckMode::NoCheck => {
            run_path_transitions(graph, path, path_display, opts, path_idx)
        }
    };
    result.duration = start.elapsed();
    result
}

/// CheckOnly mode: iterate states in path order, run assertions on each.
fn run_path_check_only(
    graph: &StateGraph,
    path: &TestPath,
    path_display: String,
    opts: &RunOptions,
) -> PathResult {
    let mut steps = Vec::new();
    let mut passed = true;

    // Collect states in path order (source of first, then targets)
    let mut state_ids: Vec<StateId> = Vec::new();
    if let Some(&first_ti) = path.steps.first() {
        state_ids.push(graph.transitions[first_ti].source);
    }
    for &ti in &path.steps {
        state_ids.push(graph.transitions[ti].target);
    }

    for (i, &state_id) in state_ids.iter().enumerate() {
        if crate::signal::is_interrupted() {
            passed = false;
            break;
        }
        let state = &graph.states[state_id.0];
        let assertions = graph.assertions_for(state_id);
        if assertions.is_empty() {
            continue;
        }

        let step_start = Instant::now();

        // Copy state to temp dir to run assertions
        let (temp_dir, work_dir) = match copy_state_to_temp(state_id, graph) {
            Ok(pair) => pair,
            Err(e) => {
                steps.push(StepResult {
                    transition_name: format!("assertions on {}", state.name),
                    source_name: state.name.clone(),
                    target_name: state.name.clone(),
                    exit_code: None,
                    stdout: String::new(),
                    stderr: e,
                    comparison: None,
                    output_diffs: Vec::new(),
                    assertion_results: Vec::new(),
                    passed: false,
                    duration: step_start.elapsed(),
                });
                passed = false;
                break;
            }
        };

        let assertion_results =
            run_assertions(&assertions, &work_dir, &state.env, graph, &*opts.sandbox);
        let assertions_passed = assertion_results.iter().all(|a| a.passed);
        if !assertions_passed {
            passed = false;
        }

        // Determine a label — use transition name if available, else state name
        let label = if i > 0 {
            let ti = path.steps[i - 1];
            graph.transitions[ti].name.clone()
        } else {
            format!("(root) {}", state.name)
        };

        steps.push(StepResult {
            transition_name: label,
            source_name: state.name.clone(),
            target_name: state.name.clone(),
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
            comparison: None,
            output_diffs: Vec::new(),
            assertion_results,
            passed: assertions_passed,
            duration: step_start.elapsed(),
        });

        if !opts.keep_temp {
            drop(temp_dir);
        }

        if !assertions_passed {
            break;
        }
    }

    PathResult {
        path_display,
        steps,
        passed,
        duration: Duration::ZERO,
    }
}

/// Full and NoCheck modes: execute transitions, compare filesystem, optionally run assertions.
fn run_path_transitions(
    graph: &StateGraph,
    path: &TestPath,
    path_display: String,
    opts: &RunOptions,
    path_idx: usize,
) -> PathResult {
    let mut steps = Vec::new();
    let mut passed = true;
    let run_assertions_flag = opts.check_mode == CheckMode::Full;

    // For chained paths (A → B → C), the output of one transition
    // becomes the input for the next. Start with the first state.
    let mut current_dir: Option<(TempDir, Utf8PathBuf)> = None;

    for (step_idx, &transition_idx) in path.steps.iter().enumerate() {

        if crate::signal::is_interrupted() {
            passed = false;
            break;
        }
        let transition = &graph.transitions[transition_idx];
        let source = &graph.states[transition.source.0];
        let target = &graph.states[transition.target.0];

        // Determine the working directory for this step.
        // First step: copy the source state to a temp dir.
        // Subsequent steps: use the temp dir from the previous step.
        let (temp_dir, work_dir) = if step_idx == 0 {
            match copy_state_to_temp(source.id, graph) {
                Ok(pair) => pair,
                Err(e) => {
                    steps.push(StepResult {
                        transition_name: transition.name.clone(),
                        source_name: source.name.clone(),
                        target_name: target.name.clone(),
                        exit_code: None,
                        stdout: String::new(),
                        stderr: e,
                        comparison: None,
                        output_diffs: Vec::new(),
                        assertion_results: Vec::new(),
                        passed: false,
                        duration: Duration::ZERO,
                    });
                    passed = false;
                    break;
                }
            }
        } else {
            match current_dir.take() {
                Some(pair) => pair,
                None => match copy_state_to_temp(source.id, graph) {
                    Ok(pair) => pair,
                    Err(e) => {
                        steps.push(StepResult {
                            transition_name: transition.name.clone(),
                            source_name: source.name.clone(),
                            target_name: target.name.clone(),
                            exit_code: None,
                            stdout: String::new(),
                            stderr: e,
                            comparison: None,
                            output_diffs: Vec::new(),
                            assertion_results: Vec::new(),
                            passed: false,
                            duration: Duration::ZERO,
                        });
                        passed = false;
                        break;
                    }
                },
            }
        };

        // In Full mode, run source state assertions on the first step
        let mut source_assertion_results = Vec::new();
        if run_assertions_flag && step_idx == 0 {
            let source_assertions = graph.assertions_for(source.id);
            if !source_assertions.is_empty() {
                source_assertion_results = run_assertions(
                    &source_assertions,
                    &work_dir,
                    &source.env,
                    graph,
                    &*opts.sandbox,
                );
            }
        }

        // Build recording path if recording is enabled
        let recording_path = opts.recording.as_ref().map(|rc| {
            let path_dir = rc.output_dir.join(format!("path-{path_idx}"));
            path_dir.join(format!("step-{step_idx}.cast"))
        });

        // Execute the transition command in the sandboxed env
        let step_result = execute_transition(
            transition,
            &work_dir,
            &source.env,
            target,
            graph,
            &*opts.sandbox,
            run_assertions_flag,
            recording_path.as_ref(),
        );

        // Merge source assertions into the step result (first step only)
        let mut step_result = step_result;
        if !source_assertion_results.is_empty() {
            let source_failed = source_assertion_results.iter().any(|a| !a.passed);
            step_result
                .assertion_results
                .splice(0..0, source_assertion_results);
            if source_failed {
                step_result.passed = false;
            }
        }

        let step_passed = step_result.passed;
        if !step_passed {
            passed = false;
        }

        // If this step passed and there are more steps, carry the temp dir forward
        if step_passed && step_idx + 1 < path.steps.len() {
            current_dir = Some((temp_dir, work_dir));
        } else if !opts.keep_temp {
            drop(temp_dir); // cleanup
        }

        steps.push(step_result);

        if !step_passed {
            break; // stop on first failure
        }
    }

    PathResult {
        path_display,
        steps,
        passed,
        duration: Duration::ZERO,
    }
}

/// Copy a state's files (excluding .missouri/) to a temp directory.
///
/// Directories in the config directory named `dot-<name>/` are restored as
/// `.<name>/` in the temp dir. This allows fixtures to carry dotfile state
/// (`.git/`, `.clc/`, etc.) that can't be tracked directly by git.
fn copy_state_to_temp(
    state_id: StateId,
    graph: &StateGraph,
) -> std::result::Result<(TempDir, Utf8PathBuf), String> {
    let state = &graph.states[state_id.0];
    let temp_dir = TempDir::new().map_err(|e| format!("failed to create temp dir: {e}"))?;
    let temp_path = Utf8PathBuf::try_from(temp_dir.path().to_owned())
        .map_err(|e| format!("temp dir path not UTF-8: {e}"))?;

    copy_dir_recursive(&state.path, &temp_path, &graph.config_dir)
        .map_err(|e| format!("failed to copy state to temp dir: {e}"))?;

    // Restore dot-<name>/ → .<name>/ for each matching directory in config dir.
    let config_path = state.path.join(&graph.config_dir);
    if config_path.exists() {
        if let Ok(entries) = std::fs::read_dir(&config_path) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str.starts_with("dot-") && entry.file_type().map_or(false, |ft| ft.is_dir()) {
                    let real_name = format!(".{}", &name_str[4..]);
                    let dst = temp_path.join(&real_name);
                    std::fs::create_dir_all(&dst)
                        .map_err(|e| format!("failed to create {real_name} dir: {e}"))?;
                    let src = Utf8PathBuf::try_from(entry.path())
                        .map_err(|e| format!("dot-dir path not UTF-8: {e}"))?;
                    copy_dir_recursive_inner(&src, &dst, &graph.config_dir, true)
                        .map_err(|e| format!("failed to copy {name_str} to {real_name}: {e}"))?;
                }
            }
        }
    }

    Ok((temp_dir, temp_path))
}

/// Recursively copy directory contents, skipping the config directory.
/// When `skip_gitkeep` is true, `.gitkeep` files are also skipped (used
/// for dot-dir restoration where `.gitkeep` is git plumbing, not content).
fn copy_dir_recursive(src: &Utf8Path, dst: &Utf8Path, config_dir: &str) -> std::io::Result<()> {
    copy_dir_recursive_inner(src, dst, config_dir, false)
}

fn copy_dir_recursive_inner(
    src: &Utf8Path,
    dst: &Utf8Path,
    config_dir: &str,
    skip_gitkeep: bool,
) -> std::io::Result<()> {
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        if name_str == config_dir || (skip_gitkeep && name_str == ".gitkeep") {
            continue;
        }

        let src_path = Utf8PathBuf::try_from(entry.path())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let dst_path = dst.join(name_str.as_ref());

        let ft = entry.file_type()?;
        if ft.is_dir() {
            std::fs::create_dir_all(&dst_path)?;
            copy_dir_recursive_inner(&src_path, &dst_path, config_dir, skip_gitkeep)?;
        } else if ft.is_symlink() {
            let target = std::fs::read_link(&src_path)?;
            #[cfg(unix)]
            std::os::unix::fs::symlink(&target, &dst_path)?;
            #[cfg(windows)]
            std::os::windows::fs::symlink_file(&target, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

/// Run assertion commands against a state in a working directory.
fn run_assertions(
    assertions: &[&Assertion],
    work_dir: &Utf8Path,
    state_env: &std::collections::BTreeMap<String, String>,
    graph: &StateGraph,
    sandbox: &dyn Backend,
) -> Vec<AssertionResult> {
    assertions
        .iter()
        .map(|assertion| run_single_assertion(assertion, work_dir, state_env, graph, sandbox))
        .collect()
}

/// Run a single assertion command and compare output.
fn run_single_assertion(
    assertion: &Assertion,
    work_dir: &Utf8Path,
    state_env: &std::collections::BTreeMap<String, String>,
    graph: &StateGraph,
    sandbox: &dyn Backend,
) -> AssertionResult {
    // Agent assertions are translated into `missouri agent eval <name>` commands.
    // The eval command exits 0 on pass and 1 on fail, so the existing assertion
    // infrastructure handles it without special-casing the result.
    if let Some(agent_name) = &assertion.agent {
        let assertion_start = std::time::Instant::now();
        let state = &graph.states[assertion.state.0];
        let eval_cmd = format!(
            "missouri agent eval {} --config-dir {} -d {}",
            shell_escape(agent_name),
            shell_escape(&graph.config_dir),
            shell_escape(state.path.as_str()),
        );

        let system_path =
            std::env::var("PATH").unwrap_or_else(|_| "/usr/local/bin:/usr/bin:/bin".into());
        let base_path = state_env
            .get("PATH")
            .map(|s| s.as_str())
            .unwrap_or(&system_path);
        let bin_dir = state.path.join(&graph.config_dir).join("bin");
        let bin_dir_opt = if bin_dir.exists() {
            Some(bin_dir.as_path())
        } else {
            None
        };
        let path_env = build_path_env(bin_dir_opt, graph.project_bin.as_deref(), base_path);

        let result = crate::signal::run_tracked(
            &mut sandbox.build_shell_command(&eval_cmd, work_dir, state_env, &path_env),
        );

        return match result {
            Ok(output) => {
                let exit_code = output.status.code();
                let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
                AssertionResult {
                    name: assertion.name.clone(),
                    passed: output.status.success(),
                    exit_code,
                    stdout_diff: None,
                    stderr_diff: None,
                    error: if output.status.success() {
                        None
                    } else {
                        Some(stderr)
                    },
                    duration: assertion_start.elapsed(),
                }
            }
            Err(e) => AssertionResult {
                name: assertion.name.clone(),
                passed: false,
                exit_code: None,
                stdout_diff: None,
                stderr_diff: None,
                error: Some(format!("failed to execute agent eval: {e}")),
                duration: assertion_start.elapsed(),
            },
        };
    }

    let assertion_start = std::time::Instant::now();
    let state = &graph.states[assertion.state.0];
    let bin_dir = state.path.join(&graph.config_dir).join("bin");
    let bin_dir_opt = if bin_dir.exists() {
        Some(bin_dir.as_path())
    } else {
        None
    };
    let system_path =
        std::env::var("PATH").unwrap_or_else(|_| "/usr/local/bin:/usr/bin:/bin".into());
    let base_path = state_env
        .get("PATH")
        .map(|s| s.as_str())
        .unwrap_or(&system_path);
    let path_env = build_path_env(bin_dir_opt, graph.project_bin.as_deref(), base_path);

    // Start services if configured
    let _service_handles: Vec<ServiceHandle>;
    let assertion_env: std::borrow::Cow<'_, BTreeMap<String, String>>;
    if !assertion.services.is_empty() {
        match start_services(&assertion.services, work_dir, state_env, &path_env, sandbox) {
            Ok((handles, port_env)) => {
                let mut merged = state_env.clone();
                merged.extend(port_env);
                assertion_env = std::borrow::Cow::Owned(merged);
                _service_handles = handles;
            }
            Err(e) => {
                return AssertionResult {
                    name: assertion.name.clone(),
                    passed: false,
                    exit_code: None,
                    stdout_diff: None,
                    stderr_diff: None,
                    error: Some(format!("failed to start service: {e}")),
                    duration: assertion_start.elapsed(),
                };
            }
        }
    } else {
        _service_handles = Vec::new();
        assertion_env = std::borrow::Cow::Borrowed(state_env);
    }

    let output = if assertion.shell {
        Some(crate::signal::run_tracked(
            &mut sandbox.build_shell_command(&assertion.command, work_dir, &assertion_env, &path_env),
        ))
    } else {
        let parts: Vec<&str> = assertion.command.split_whitespace().collect();
        if parts.is_empty() {
            None
        } else {
            Some(crate::signal::run_tracked(
                &mut sandbox.build_direct_command(&parts, work_dir, &assertion_env, &path_env),
            ))
        }
    };

    let output = match output {
        Some(result) => result,
        None => {
            return AssertionResult {
                name: assertion.name.clone(),
                passed: false,
                exit_code: None,
                stdout_diff: None,
                stderr_diff: None,
                error: Some("empty command".into()),
                duration: assertion_start.elapsed(),
            };
        }
    };

    let output = match output {
        Ok(o) => o,
        Err(e) => {
            return AssertionResult {
                name: assertion.name.clone(),
                passed: false,
                exit_code: None,
                stdout_diff: None,
                stderr_diff: None,
                error: Some(format!("failed to execute command: {e}")),
                duration: assertion_start.elapsed(),
            };
        }
    };

    let exit_code = output.status.code();
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

    // Exit code check: should_fail inverts the expectation
    if assertion.should_fail {
        if output.status.success() {
            return AssertionResult {
                name: assertion.name.clone(),
                passed: false,
                exit_code,
                stdout_diff: None,
                stderr_diff: None,
                error: Some("expected command to fail, but it exited 0".into()),
                duration: assertion_start.elapsed(),
            };
        }
        // Command failed as expected — fall through to stdout/stderr comparison
    } else if !output.status.success() {
        return AssertionResult {
            name: assertion.name.clone(),
            passed: false,
            exit_code,
            stdout_diff: None,
            stderr_diff: None,
            error: Some(format!(
                "command exited with {}",
                exit_code
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "signal".into())
            )),
            duration: assertion_start.elapsed(),
        };
    }

    // Compare stdout/stderr if expected values are specified
    let stdout_diff = assertion.expected_stdout.as_ref().and_then(|expected| {
        if *expected != stdout {
            Some((expected.clone(), stdout.clone()))
        } else {
            None
        }
    });

    let stderr_diff = assertion.expected_stderr.as_ref().and_then(|expected| {
        if *expected != stderr {
            Some((expected.clone(), stderr.clone()))
        } else {
            None
        }
    });

    let passed = stdout_diff.is_none() && stderr_diff.is_none();

    AssertionResult {
        name: assertion.name.clone(),
        passed,
        exit_code,
        stdout_diff,
        stderr_diff,
        error: None,
        duration: assertion_start.elapsed(),
    }
}

/// Execute a single transition command and compare the result.
fn execute_transition(
    transition: &Transition,
    work_dir: &Utf8Path,
    source_env: &std::collections::BTreeMap<String, String>,
    target: &crate::graph::State,
    graph: &StateGraph,
    sandbox: &dyn Backend,
    run_assertions_flag: bool,
    recording_path: Option<&Utf8PathBuf>,
) -> StepResult {
    let step_start = Instant::now();
    let source_name = graph.states[transition.source.0].name.clone();
    let target_name = target.name.clone();

    // Build PATH: source state's config bin/ → project bin/ → base PATH
    let source_state = &graph.states[transition.source.0];
    let bin_dir = source_state.path.join(&graph.config_dir).join("bin");
    let bin_dir_opt = if bin_dir.exists() {
        Some(bin_dir.as_path())
    } else {
        None
    };
    let system_path =
        std::env::var("PATH").unwrap_or_else(|_| "/usr/local/bin:/usr/bin:/bin".into());
    let base_path = source_env
        .get("PATH")
        .map(|s| s.as_str())
        .unwrap_or(&system_path);
    let path_env = build_path_env(bin_dir_opt, graph.project_bin.as_deref(), base_path);

    // Start mitmdump if network interception is configured.
    // The handle must stay alive until the command completes (drop kills the process).
    let _mitmdump_handle: Option<MitmdumpHandle>;
    let mut cmd_env: std::borrow::Cow<'_, BTreeMap<String, String>>;

    // Host-side network interception: start mitmdump on the host for non-Docker backends.
    // Docker backend handles network replay inside the container instead.
    match (&transition.network, sandbox.is_docker()) {
        (Some(crate::config::NetworkConfig::Replay { replay, .. }), false) => {
            // Resolve flow file path relative to source state's config dir
            let flow_path = source_state.path.join(&graph.config_dir).join(replay);
            match start_mitmdump_replay(flow_path.as_ref(), &path_env) {
                Ok(handle) => {
                    let mut merged = source_env.clone();
                    merged.extend(build_network_env(handle.port));
                    cmd_env = std::borrow::Cow::Owned(merged);
                    _mitmdump_handle = Some(handle);
                }
                Err(e) => {
                    return StepResult {
                        transition_name: transition.name.clone(),
                        source_name,
                        target_name,
                        exit_code: None,
                        stdout: String::new(),
                        stderr: e,
                        comparison: None,
                        output_diffs: Vec::new(),
                        assertion_results: Vec::new(),
                        passed: false,
                        duration: step_start.elapsed(),
                    };
                }
            }
        }
        (Some(crate::config::NetworkConfig::Record { .. }), false) => {
            // Record mode: start mitmdump writing to a flow file in the source state's
            // .missouri/recordings/ directory, named after the transition.
            let recordings_dir = source_state
                .path
                .join(&graph.config_dir)
                .join("recordings");
            if let Err(e) = std::fs::create_dir_all(&recordings_dir) {
                return StepResult {
                    transition_name: transition.name.clone(),
                    source_name,
                    target_name,
                    exit_code: None,
                    stdout: String::new(),
                    stderr: format!("failed to create recordings directory: {e}"),
                    comparison: None,
                    output_diffs: Vec::new(),
                    assertion_results: Vec::new(),
                    passed: false,
                    duration: step_start.elapsed(),
                };
            }
            let flow_name = transition
                .name
                .replace(|c: char| !c.is_alphanumeric() && c != '-' && c != '_', "_");
            let flow_path = recordings_dir.join(format!("{flow_name}.flow"));
            match start_mitmdump_record(flow_path.as_ref(), &path_env) {
                Ok(handle) => {
                    let mut merged = source_env.clone();
                    merged.extend(build_network_env(handle.port));
                    cmd_env = std::borrow::Cow::Owned(merged);
                    _mitmdump_handle = Some(handle);
                }
                Err(e) => {
                    return StepResult {
                        transition_name: transition.name.clone(),
                        source_name,
                        target_name,
                        exit_code: None,
                        stdout: String::new(),
                        stderr: e,
                        comparison: None,
                        output_diffs: Vec::new(),
                        assertion_results: Vec::new(),
                        passed: false,
                        duration: step_start.elapsed(),
                    };
                }
            }
        }
        _ => {
            // No host-side network config (either no network config, or Docker handles it)
            _mitmdump_handle = None;
            cmd_env = std::borrow::Cow::Borrowed(source_env);
        }
    }

    // Start services if configured
    let _service_handles: Vec<ServiceHandle>;
    if !transition.services.is_empty() {
        match start_services(&transition.services, work_dir, &cmd_env, &path_env, sandbox) {
            Ok((handles, port_env)) => {
                let mut merged = cmd_env.into_owned();
                merged.extend(port_env);
                cmd_env = std::borrow::Cow::Owned(merged);
                _service_handles = handles;
            }
            Err(e) => {
                return StepResult {
                    transition_name: transition.name.clone(),
                    source_name,
                    target_name,
                    exit_code: None,
                    stdout: String::new(),
                    stderr: e,
                    comparison: None,
                    output_diffs: Vec::new(),
                    assertion_results: Vec::new(),
                    passed: false,
                    duration: step_start.elapsed(),
                };
            }
        }
    } else {
        _service_handles = Vec::new();
    }

    // Run the command. Docker backend handles everything inside a container,
    // including network replay if configured. Other backends use local Commands.
    let (exit_code, stdout, stderr) = if sandbox.is_docker() {
        // Resolve replay flow path and hosts for Docker backend
        let (replay_flow, replay_hosts) = match &transition.network {
            Some(crate::config::NetworkConfig::Replay { replay, hosts }) => {
                let flow = source_state.path.join(&graph.config_dir).join(replay);
                (Some(flow), hosts.as_slice())
            }
            _ => (None, [].as_slice()),
        };

        // Check for Dockerfile in the source state's config directory
        let dockerfile_dir = {
            let config_dir = source_state.path.join(&graph.config_dir);
            if config_dir.join("Dockerfile").exists() {
                Some(config_dir)
            } else {
                None
            }
        };

        // Downcast to DockerBackend to access run_in_container with replay
        let docker = sandbox
            .as_any()
            .downcast_ref::<DockerBackend>()
            .expect("is_docker() returned true but backend is not DockerBackend");

        match docker.run_in_container(
            &transition.command,
            work_dir,
            &cmd_env,
            replay_flow.as_deref(),
            replay_hosts,
            dockerfile_dir.as_deref(),
        ) {
            Ok(out) => (Some(out.exit_code), out.stdout, out.stderr),
            Err(e) => {
                return StepResult {
                    transition_name: transition.name.clone(),
                    source_name,
                    target_name,
                    exit_code: None,
                    stdout: String::new(),
                    stderr: format!("docker execution failed: {e}"),
                    comparison: None,
                    output_diffs: Vec::new(),
                    assertion_results: Vec::new(),
                    passed: false,
                    duration: step_start.elapsed(),
                };
            }
        }
    } else {
        let output = if let Some(cast_path) = recording_path {
            Some(crate::recorder::record_command(
                &transition.command,
                transition.shell,
                work_dir,
                &cmd_env,
                &path_env,
                cast_path,
                sandbox,
            ))
        } else if transition.shell {
            Some(crate::signal::run_tracked(
                &mut sandbox
                    .build_shell_command(&transition.command, work_dir, &cmd_env, &path_env),
            ))
        } else {
            let parts: Vec<&str> = transition.command.split_whitespace().collect();
            if parts.is_empty() {
                None
            } else {
                Some(crate::signal::run_tracked(
                    &mut sandbox.build_direct_command(&parts, work_dir, &cmd_env, &path_env),
                ))
            }
        };

        let output = match output {
            Some(result) => result,
            None => {
                return StepResult {
                    transition_name: transition.name.clone(),
                    source_name,
                    target_name,
                    exit_code: None,
                    stdout: String::new(),
                    stderr: "empty command".into(),
                    comparison: None,
                    output_diffs: Vec::new(),
                    assertion_results: Vec::new(),
                    passed: false,
                    duration: step_start.elapsed(),
                };
            }
        };

        match output {
            Ok(o) => (
                o.status.code(),
                String::from_utf8_lossy(&o.stdout).into_owned(),
                String::from_utf8_lossy(&o.stderr).into_owned(),
            ),
            Err(e) => {
                return StepResult {
                    transition_name: transition.name.clone(),
                    source_name,
                    target_name,
                    exit_code: None,
                    stdout: String::new(),
                    stderr: format!("failed to execute command: {e}"),
                    comparison: None,
                    output_diffs: Vec::new(),
                    assertion_results: Vec::new(),
                    passed: false,
                    duration: step_start.elapsed(),
                };
            }
        }
    };

    if exit_code != Some(0) {
        return StepResult {
            transition_name: transition.name.clone(),
            source_name,
            target_name,
            exit_code,
            stdout,
            stderr,
            comparison: None,
            output_diffs: Vec::new(),
            assertion_results: Vec::new(),
            passed: false,
            duration: step_start.elapsed(),
        };
    }

    // Compare transition stdout/stderr if expected values are specified
    let mut output_diffs = Vec::new();
    if let Some(expected) = &transition.expected_stdout
        && *expected != stdout
    {
        output_diffs.push(OutputDiff::StdoutMismatch {
            expected: expected.clone(),
            actual: stdout.clone(),
        });
    }
    if let Some(expected) = &transition.expected_stderr
        && *expected != stderr
    {
        output_diffs.push(OutputDiff::StderrMismatch {
            expected: expected.clone(),
            actual: stderr.clone(),
        });
    }

    // Build bin dirs for comparator PATH: state bin/ + project bin/
    let source_state = &graph.states[transition.source.0];
    let bin_dir = source_state.path.join(&graph.config_dir).join("bin");
    let mut comparator_bin_dirs: Vec<&Utf8Path> = Vec::new();
    if bin_dir.exists() {
        comparator_bin_dirs.push(bin_dir.as_path());
    }
    if let Some(ref pb) = graph.project_bin {
        comparator_bin_dirs.push(pb.as_path());
    }

    // Compare the result against the expected target state
    let comparison = compare::compare_trees(
        work_dir,
        &target.path,
        &transition.file_comparators,
        &comparator_bin_dirs,
        source_env,
        &graph.config_dir,
        &graph.ignore,
        sandbox,
    );

    // Compare env vars only when the target state or transition defines env expectations.
    let env_diffs = if !target.env.is_empty() || !transition.env_comparators.is_empty() {
        compare::compare_env(
            source_env,
            &target.env,
            &transition.env_comparators,
            &comparator_bin_dirs,
            source_env,
            sandbox,
        )
    } else {
        Vec::new()
    };

    let mut comparison = comparison;
    comparison.env_diffs = env_diffs;
    comparison.passed = comparison.passed && comparison.env_diffs.is_empty();

    // Run target state assertions in Full mode
    let assertion_results = if run_assertions_flag {
        let target_assertions = graph.assertions_for(transition.target);
        if !target_assertions.is_empty() {
            run_assertions(&target_assertions, work_dir, &target.env, graph, sandbox)
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    let assertions_passed = assertion_results.iter().all(|a| a.passed);
    let passed = comparison.passed && output_diffs.is_empty() && assertions_passed;

    StepResult {
        transition_name: transition.name.clone(),
        source_name,
        target_name,
        exit_code,
        stdout,
        stderr,
        comparison: Some(comparison),
        output_diffs,
        assertion_results,
        passed,
        duration: step_start.elapsed(),
    }
}

/// Simple hash for generating stable image tags from flake content.
fn md5_hash(data: &[u8]) -> u64 {
    // FNV-1a hash — not cryptographic, just for cache keys
    let mut hash: u64 = 0xcbf29ce484222325;
    for &byte in data {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// Create a tar archive from a directory, an injected Dockerfile, and extra files.
/// Create a tar archive from a directory for Docker build context.
fn create_build_context(dir: &Utf8Path) -> std::io::Result<Vec<u8>> {
    let mut archive = tar::Builder::new(Vec::new());

    for entry in walkdir::WalkDir::new(dir.as_std_path())
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let path = entry.path();
        let rel = path.strip_prefix(dir.as_std_path()).unwrap_or(path);
        let data = std::fs::read(path)?;
        let mut header = tar::Header::new_gnu();
        header.set_size(data.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        archive.append_data(&mut header, rel, data.as_slice())?;
    }

    archive.into_inner()
}

#[cfg(test)]
mod tests {
    use super::*;
    use camino::Utf8Path;
    use std::fs;

    fn make_state(tmp: &Utf8Path, name: &str, yaml: &str) {
        let state_dir = tmp.join(name);
        let missouri_dir = state_dir.join(".missouri");
        fs::create_dir_all(&missouri_dir).unwrap();
        fs::write(missouri_dir.join("missouri.yml"), yaml).unwrap();
    }

    #[test]
    fn detect_sandbox_none_when_no_config() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();

        make_state(
            root,
            "a",
            r#"
transitions:
  - command: "echo"
    target: "../b"
"#,
        );
        make_state(root, "b", "{}");

        let graph = StateGraph::discover(root, ".missouri").unwrap();
        assert!(matches!(graph.sandbox_config, SandboxConfig::None));
        let backend = detect_sandbox(&graph).unwrap();
        let debug = format!("{backend:?}");
        assert!(
            debug.starts_with("BareBackend"),
            "expected BareBackend, got {debug}"
        );
    }

    #[test]
    fn detect_sandbox_packages_resolves_to_nix() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();

        // Create project-level config with packages
        let root_missouri = root.join(".missouri");
        fs::create_dir_all(&root_missouri).unwrap();
        fs::write(
            root_missouri.join("missouri.yml"),
            "packages:\n  - python3\n  - uv\n",
        )
        .unwrap();

        make_state(
            root,
            "a",
            r#"
transitions:
  - command: "echo"
    target: "../b"
"#,
        );
        make_state(root, "b", "{}");

        let graph = StateGraph::discover(root, ".missouri").unwrap();
        assert!(matches!(graph.sandbox_config, SandboxConfig::Packages(_)));

        // nix must be on PATH for this test to produce NixBackend
        if which_nix().is_none() {
            eprintln!("skipping detect_sandbox_packages_resolves_to_nix: nix not on PATH");
            return;
        }

        // Clear MISSOURI_SANDBOX in case it's set (e.g., inside nix build)
        // SAFETY: test is single-threaded for this env var manipulation.
        let saved = std::env::var("MISSOURI_SANDBOX").ok();
        unsafe { std::env::remove_var("MISSOURI_SANDBOX") };

        let backend = detect_sandbox(&graph).unwrap();

        // Restore if it was set
        if let Some(val) = saved {
            unsafe { std::env::set_var("MISSOURI_SANDBOX", val) };
        }

        let debug = format!("{backend:?}");
        assert!(
            debug.starts_with("NixBackend"),
            "expected NixBackend, got {debug}"
        );
        assert!(
            debug.contains("nix"),
            "nix_bin should contain 'nix': {debug}"
        );
        assert!(
            debug.contains("python3"),
            "packages should contain python3: {debug}"
        );
        assert!(debug.contains("uv"), "packages should contain uv: {debug}");
    }

    #[test]
    fn detect_sandbox_preinstalled_via_env_var() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();

        // Create project-level config with packages
        let root_missouri = root.join(".missouri");
        fs::create_dir_all(&root_missouri).unwrap();
        fs::write(
            root_missouri.join("missouri.yml"),
            "packages:\n  - python3\n  - uv\n",
        )
        .unwrap();

        make_state(
            root,
            "a",
            r#"
transitions:
  - command: "echo"
    target: "../b"
"#,
        );
        make_state(root, "b", "{}");

        let graph = StateGraph::discover(root, ".missouri").unwrap();
        assert!(matches!(graph.sandbox_config, SandboxConfig::Packages(_)));

        // With MISSOURI_SANDBOX=preinstalled, packages config should resolve
        // to BareBackend (tools assumed already on PATH).
        // SAFETY: test is single-threaded for this env var manipulation.
        unsafe { std::env::set_var("MISSOURI_SANDBOX", "preinstalled") };
        let backend = detect_sandbox(&graph).unwrap();
        unsafe { std::env::remove_var("MISSOURI_SANDBOX") };
        let debug = format!("{backend:?}");
        assert!(
            debug.starts_with("BareBackend"),
            "expected BareBackend when MISSOURI_SANDBOX=preinstalled, got {debug}"
        );
    }

    #[test]
    fn detect_sandbox_docker_from_config() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();

        let root_missouri = root.join(".missouri");
        fs::create_dir_all(&root_missouri).unwrap();
        fs::write(
            root_missouri.join("missouri.yml"),
            "docker: true\n",
        )
        .unwrap();

        make_state(
            root,
            "a",
            r#"
transitions:
  - command: "echo hello"
    target: "../b"
"#,
        );
        make_state(root, "b", "{}");

        let graph = StateGraph::discover(root, ".missouri").unwrap();
        assert!(
            matches!(graph.sandbox_config, SandboxConfig::Docker { .. }),
            "expected Docker, got {:?}",
            graph.sandbox_config
        );

        // Clear MISSOURI_SANDBOX in case it's set (e.g., inside nix build)
        let saved = std::env::var("MISSOURI_SANDBOX").ok();
        unsafe { std::env::remove_var("MISSOURI_SANDBOX") };

        let backend = detect_sandbox(&graph).unwrap();

        if let Some(val) = saved {
            unsafe { std::env::set_var("MISSOURI_SANDBOX", val) };
        }

        let debug = format!("{backend:?}");
        assert!(
            debug.starts_with("DockerBackend"),
            "expected DockerBackend, got {debug}"
        );
    }

    #[test]
    fn detect_sandbox_docker_overrides_packages() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();

        let root_missouri = root.join(".missouri");
        fs::create_dir_all(&root_missouri).unwrap();
        fs::write(
            root_missouri.join("missouri.yml"),
            "docker: true\npackages:\n  - python3\n",
        )
        .unwrap();

        make_state(
            root,
            "a",
            r#"
transitions:
  - command: "echo hello"
    target: "../b"
"#,
        );
        make_state(root, "b", "{}");

        let graph = StateGraph::discover(root, ".missouri").unwrap();
        assert!(
            matches!(graph.sandbox_config, SandboxConfig::Docker { .. }),
            "docker: true should take precedence over packages"
        );
    }

    #[test]
    #[ignore = "requires Docker"]
    fn docker_backend_executes_command_in_vm() {
        let backend = DockerBackend { image: DEFAULT_DOCKER_IMAGE.to_string() };
        let env = BTreeMap::new();
        let work_dir = Utf8Path::new("/tmp");
        let result = backend
            .execute("echo hello-from-sandbox", true, work_dir, &env, "")
            .unwrap();
        assert_eq!(result.stdout.trim(), "hello-from-sandbox");
        assert_eq!(result.exit_code, 0);
    }

    #[test]
    #[ignore = "requires Docker"]
    fn docker_backend_captures_exit_code() {
        let backend = DockerBackend { image: DEFAULT_DOCKER_IMAGE.to_string() };
        let env = BTreeMap::new();
        let work_dir = Utf8Path::new("/tmp");
        let result = backend
            .execute("exit 42", true, work_dir, &env, "")
            .unwrap();
        assert_eq!(result.exit_code, 42);
    }

    #[test]
    #[ignore = "requires Docker"]
    fn docker_backend_captures_stderr() {
        let backend = DockerBackend { image: DEFAULT_DOCKER_IMAGE.to_string() };
        let env = BTreeMap::new();
        let work_dir = Utf8Path::new("/tmp");
        let result = backend
            .execute("echo oops >&2", true, work_dir, &env, "")
            .unwrap();
        assert_eq!(result.stderr.trim(), "oops");
        assert_eq!(result.exit_code, 0);
    }

    #[test]
    #[ignore = "requires Docker"]
    fn docker_backend_injects_env() {
        let backend = DockerBackend { image: DEFAULT_DOCKER_IMAGE.to_string() };
        let mut env = BTreeMap::new();
        env.insert("MISSOURI_TEST_VAR".into(), "sandbox-value".into());
        let work_dir = Utf8Path::new("/tmp");
        let result = backend
            .execute("echo $MISSOURI_TEST_VAR", true, work_dir, &env, "")
            .unwrap();
        assert_eq!(result.stdout.trim(), "sandbox-value");
    }

    #[test]
    #[ignore = "requires Docker"]
    fn docker_backend_runs_in_linux_vm() {
        let backend = DockerBackend { image: DEFAULT_DOCKER_IMAGE.to_string() };
        let env = BTreeMap::new();
        let work_dir = Utf8Path::new("/tmp");
        let result = backend
            .execute("uname -s", true, work_dir, &env, "")
            .unwrap();
        assert_eq!(result.stdout.trim(), "Linux");
    }

    #[test]
    fn which_nix_finds_binary() {
        let result = which_nix();
        if result.is_none() {
            eprintln!("skipping which_nix_finds_binary: nix not on PATH");
            return;
        }
        assert!(result.unwrap().as_str().ends_with("nix"));
    }

    #[test]
    fn bare_backend_shell_command_runs() {
        let backend = BareBackend;
        let tmp = tempfile::tempdir().unwrap();
        let work_dir = Utf8Path::from_path(tmp.path()).unwrap();
        let env = BTreeMap::new();

        let output = backend
            .build_shell_command("echo hello", work_dir, &env, "/usr/bin:/bin")
            .output()
            .unwrap();
        assert!(output.status.success());
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "hello");
    }

    #[test]
    fn bare_backend_direct_command_runs() {
        let backend = BareBackend;
        let tmp = tempfile::tempdir().unwrap();
        let work_dir = Utf8Path::from_path(tmp.path()).unwrap();
        let env = BTreeMap::new();

        let output = backend
            .build_direct_command(&["echo", "world"], work_dir, &env, "/usr/bin:/bin")
            .output()
            .unwrap();
        assert!(output.status.success());
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "world");
    }

    #[test]
    fn bare_backend_env_cleared_and_set() {
        let backend = BareBackend;
        let tmp = tempfile::tempdir().unwrap();
        let work_dir = Utf8Path::from_path(tmp.path()).unwrap();
        let mut env = BTreeMap::new();
        env.insert("MY_VAR".to_string(), "myvalue".to_string());

        let output = backend
            .build_shell_command("echo $MY_VAR", work_dir, &env, "/usr/bin:/bin")
            .output()
            .unwrap();
        assert!(output.status.success());
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "myvalue");
    }

    #[test]
    fn build_network_env_sets_https_proxy() {
        let port = 8080u16;
        let env = build_network_env(port);
        assert_eq!(
            env.get("HTTPS_PROXY").map(|s| s.as_str()),
            Some("http://127.0.0.1:8080")
        );
        assert_eq!(
            env.get("HTTP_PROXY").map(|s| s.as_str()),
            Some("http://127.0.0.1:8080")
        );
    }

    #[test]
    fn build_network_env_sets_ca_cert() {
        let port = 9999u16;
        let env = build_network_env(port);
        // NODE_EXTRA_CA_CERTS must point at the mitmproxy CA cert
        let ca = env.get("NODE_EXTRA_CA_CERTS").expect("NODE_EXTRA_CA_CERTS not set");
        assert!(
            ca.contains("mitmproxy"),
            "NODE_EXTRA_CA_CERTS should point at mitmproxy cert, got: {ca}"
        );
    }

    #[test]
    fn start_mitmdump_replay_errors_when_not_on_path() {
        // On a PATH with no mitmdump binary, start_mitmdump should return an error.
        let tmp = tempfile::tempdir().unwrap();
        let flow_path = tmp.path().join("test.flow");
        std::fs::write(&flow_path, b"").unwrap();
        let flow = camino::Utf8PathBuf::try_from(flow_path).unwrap();

        // Override PATH to an empty directory so mitmdump is not found
        let empty_dir = tmp.path().join("empty_bin");
        std::fs::create_dir_all(&empty_dir).unwrap();
        let empty_path = empty_dir.to_str().unwrap();

        let result = start_mitmdump_replay(&flow, empty_path);
        assert!(result.is_err(), "expected error when mitmdump not on PATH");
        let msg = result.unwrap_err();
        assert!(
            msg.contains("mitmdump"),
            "error message should mention mitmdump: {msg}"
        );
    }

    /// When a transition has `network: { replay: ... }` and mitmdump is not
    /// available on PATH, execute_transition should fail with an error
    /// mentioning mitmdump rather than silently ignoring the network config.
    #[test]
    fn execute_transition_network_replay_fails_without_mitmdump() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();

        // Source state: transition with network replay pointing to a flow file
        make_state(
            root,
            "a",
            r#"
transitions:
  - command: "echo hello"
    target: "../b"
    network:
      replay: test.flow
"#,
        );
        // Create the referenced flow file in the source state's .missouri/
        fs::write(root.join("a").join(".missouri").join("test.flow"), b"fake flow").unwrap();

        // Target state: empty (no expected files beyond .missouri/)
        make_state(root, "b", "{}");

        let graph = StateGraph::discover(root, ".missouri").unwrap();
        let transition = &graph.transitions[0];
        let target = &graph.states[transition.target.0];

        // PATH includes /usr/bin:/bin so sh can run, but no mitmdump
        let empty_bin = root.join("empty_bin");
        fs::create_dir_all(&empty_bin).unwrap();
        let mut source_env = BTreeMap::new();
        source_env.insert(
            "PATH".into(),
            format!("{empty_bin}:/usr/bin:/bin"),
        );

        // Work dir: empty temp (simulates a clean source state copy)
        let work = tempfile::tempdir().unwrap();
        let work_dir = Utf8Path::from_path(work.path()).unwrap();

        let result = execute_transition(
            transition,
            work_dir,
            &source_env,
            target,
            &graph,
            &BareBackend,
            false,
            None,
        );

        assert!(
            !result.passed,
            "transition with network replay should fail when mitmdump not on PATH"
        );
        assert!(
            result.stderr.contains("mitmdump"),
            "error should mention mitmdump, got: {}",
            result.stderr,
        );
    }

    #[test]
    fn start_mitmdump_record_errors_when_not_on_path() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();
        let output_file = root.join("output.flow");

        // Empty PATH so mitmdump is not found
        let empty_dir = root.join("empty");
        fs::create_dir_all(&empty_dir).unwrap();
        let empty_path = empty_dir.to_string();

        let result = start_mitmdump_record(&output_file, &empty_path);
        assert!(result.is_err(), "expected error when mitmdump not on PATH");
        let msg = result.unwrap_err();
        assert!(
            msg.contains("mitmdump"),
            "error message should mention mitmdump: {msg}"
        );
    }

    /// When a transition has `network: { record: true }` and mitmdump is not
    /// available on PATH, execute_transition should fail with an error
    /// mentioning mitmdump.
    #[test]
    fn execute_transition_network_record_fails_without_mitmdump() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();

        make_state(
            root,
            "a",
            r#"
transitions:
  - command: "echo hello"
    target: "../b"
    network:
      record: true
"#,
        );
        make_state(root, "b", "{}");

        let graph = StateGraph::discover(root, ".missouri").unwrap();
        let transition = &graph.transitions[0];
        let target = &graph.states[transition.target.0];

        // PATH includes /usr/bin:/bin so sh can run, but no mitmdump
        let empty_bin = root.join("empty_bin");
        fs::create_dir_all(&empty_bin).unwrap();
        let mut source_env = BTreeMap::new();
        source_env.insert(
            "PATH".into(),
            format!("{empty_bin}:/usr/bin:/bin"),
        );

        let work = tempfile::tempdir().unwrap();
        let work_dir = Utf8Path::from_path(work.path()).unwrap();

        let result = execute_transition(
            transition,
            work_dir,
            &source_env,
            target,
            &graph,
            &BareBackend,
            false,
            None,
        );

        assert!(
            !result.passed,
            "transition with network record should fail when mitmdump not on PATH"
        );
        assert!(
            result.stderr.contains("mitmdump"),
            "error should mention mitmdump, got: {}",
            result.stderr,
        );
    }

    /// When a transition has `network: { record: true }` and mitmdump is
    /// available, the transition command should receive HTTPS_PROXY and
    /// HTTP_PROXY environment variables pointing at the mitmdump proxy.
    #[test]
    fn execute_transition_network_record_injects_proxy_env() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();

        make_state(
            root,
            "a",
            r#"
transitions:
  - command: "echo $HTTPS_PROXY"
    target: "../b"
    network:
      record: true
"#,
        );
        make_state(root, "b", "{}");

        // Create a fake mitmdump that announces a port on stderr and stays alive
        let bin_dir = root.join("fake_bin");
        fs::create_dir_all(&bin_dir).unwrap();
        let fake_mitmdump = bin_dir.join("mitmdump");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::write(
                &fake_mitmdump,
                r#"#!/usr/bin/env python3
import socket, sys, time, signal
signal.signal(signal.SIGTERM, lambda *a: sys.exit(0))
signal.signal(signal.SIGINT, lambda *a: sys.exit(0))
s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
s.bind(('127.0.0.1', 0))
port = s.getsockname()[1]
s.listen(1)
print(f'Proxy server listening at http://*:{port}', file=sys.stderr, flush=True)
time.sleep(300)
"#,
            )
            .unwrap();
            fs::set_permissions(&fake_mitmdump, fs::Permissions::from_mode(0o755)).unwrap();
        }

        let graph = StateGraph::discover(root, ".missouri").unwrap();
        let transition = &graph.transitions[0];
        let target = &graph.states[transition.target.0];

        let mut source_env = BTreeMap::new();
        source_env.insert("PATH".into(), format!("{bin_dir}:/usr/bin:/bin"));

        let work = tempfile::tempdir().unwrap();
        let work_dir = Utf8Path::from_path(work.path()).unwrap();

        let result = execute_transition(
            transition,
            work_dir,
            &source_env,
            target,
            &graph,
            &BareBackend,
            false,
            None,
        );

        assert!(
            result.passed,
            "transition should pass with fake mitmdump in record mode; stderr: {}",
            result.stderr,
        );
        assert!(
            result.stdout.contains("http://127.0.0.1:"),
            "HTTPS_PROXY should be injected into command env in record mode, got stdout: '{}'",
            result.stdout,
        );
    }

    /// When a transition has `network: { replay: ... }` and mitmdump is
    /// available, the transition command should receive HTTPS_PROXY and
    /// HTTP_PROXY environment variables pointing at the mitmdump proxy.
    #[test]
    fn execute_transition_network_replay_injects_proxy_env() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();

        // Source state: command prints HTTPS_PROXY so we can verify injection
        make_state(
            root,
            "a",
            r#"
transitions:
  - command: "echo $HTTPS_PROXY"
    target: "../b"
    network:
      replay: test.flow
"#,
        );
        fs::write(
            root.join("a").join(".missouri").join("test.flow"),
            b"fake flow",
        )
        .unwrap();

        make_state(root, "b", "{}");

        // Create a fake mitmdump that announces a port on stderr and stays alive
        let bin_dir = root.join("fake_bin");
        fs::create_dir_all(&bin_dir).unwrap();
        let fake_mitmdump = bin_dir.join("mitmdump");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::write(
                &fake_mitmdump,
                r#"#!/usr/bin/env python3
import socket, sys, time, signal
signal.signal(signal.SIGTERM, lambda *a: sys.exit(0))
signal.signal(signal.SIGINT, lambda *a: sys.exit(0))
s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
s.bind(('127.0.0.1', 0))
port = s.getsockname()[1]
s.listen(1)
print(f'Proxy server listening at http://*:{port}', file=sys.stderr, flush=True)
time.sleep(300)
"#,
            )
            .unwrap();
            fs::set_permissions(&fake_mitmdump, fs::Permissions::from_mode(0o755)).unwrap();
        }

        let graph = StateGraph::discover(root, ".missouri").unwrap();
        let transition = &graph.transitions[0];
        let target = &graph.states[transition.target.0];

        let mut source_env = BTreeMap::new();
        source_env.insert("PATH".into(), format!("{bin_dir}:/usr/bin:/bin"));

        let work = tempfile::tempdir().unwrap();
        let work_dir = Utf8Path::from_path(work.path()).unwrap();

        let result = execute_transition(
            transition,
            work_dir,
            &source_env,
            target,
            &graph,
            &BareBackend,
            false,
            None,
        );

        assert!(
            result.passed,
            "transition should pass with fake mitmdump; stderr: {}",
            result.stderr,
        );
        assert!(
            result.stdout.contains("http://127.0.0.1:"),
            "HTTPS_PROXY should be injected into command env, got stdout: '{}'",
            result.stdout,
        );
    }

    #[test]
    fn build_service_env_single() {
        let env = build_service_env(&[8080]);
        assert_eq!(env.get("PORT").map(|s| s.as_str()), Some("8080"));
        assert!(env.get("PORT_0").is_none());
    }

    #[test]
    fn build_service_env_multiple() {
        let env = build_service_env(&[8080, 9090]);
        assert_eq!(env.get("PORT").map(|s| s.as_str()), Some("8080"));
        assert_eq!(env.get("PORT_0").map(|s| s.as_str()), Some("8080"));
        assert_eq!(env.get("PORT_1").map(|s| s.as_str()), Some("9090"));
    }

    #[test]
    fn build_service_env_empty() {
        let env = build_service_env(&[]);
        assert!(env.is_empty());
    }

    #[test]
    fn service_handle_starts_and_drops() {
        // Fake service that binds port 0 and prints it to stderr
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();

        let config = crate::config::ServiceConfig {
            command: r#"python3 -c "
import socket, sys, time, signal
signal.signal(signal.SIGTERM, lambda *a: sys.exit(0))
s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
s.bind(('127.0.0.1', 0))
port = s.getsockname()[1]
s.listen(1)
print(f'listening on :{port}', file=sys.stderr, flush=True)
time.sleep(300)
""#
            .into(),
            shell: true,
            port_pattern: None,
            ready: None,
        };

        let env = BTreeMap::new();
        let handle =
            start_service(&config, root, &env, "/usr/bin:/bin", &BareBackend).unwrap();

        assert!(handle.port > 0, "port should be assigned: {}", handle.port);

        // Verify process is running
        let pid = handle.child.id();
        let alive = unsafe { libc::kill(pid as i32, 0) };
        assert_eq!(alive, 0, "service process should be alive");

        // Drop kills it
        let pid_copy = pid;
        drop(handle);
        std::thread::sleep(Duration::from_millis(200));
        let dead = unsafe { libc::kill(pid_copy as i32, 0) };
        assert_ne!(dead, 0, "service process should be dead after drop");
    }

    #[test]
    fn service_handle_custom_port_pattern() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();

        let config = crate::config::ServiceConfig {
            command: r#"python3 -c "
import socket, sys, time, signal
signal.signal(signal.SIGTERM, lambda *a: sys.exit(0))
s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
s.bind(('127.0.0.1', 0))
port = s.getsockname()[1]
s.listen(1)
print(f'Serving HTTP on 0.0.0.0 port {port}', file=sys.stderr, flush=True)
time.sleep(300)
""#
            .into(),
            shell: true,
            port_pattern: Some(r"port (\d+)".into()),
            ready: None,
        };

        let env = BTreeMap::new();
        let handle =
            start_service(&config, root, &env, "/usr/bin:/bin", &BareBackend).unwrap();

        assert!(handle.port > 0, "port should be parsed with custom pattern");
        drop(handle);
    }

    #[test]
    fn service_exits_before_port_is_error() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();

        let config = crate::config::ServiceConfig {
            command: "echo 'no port here' >&2".into(),
            shell: true,
            port_pattern: None,
            ready: None,
        };

        let env = BTreeMap::new();
        let result = start_service(&config, root, &env, "/usr/bin:/bin", &BareBackend);
        assert!(result.is_err());
        assert!(
            result.unwrap_err().contains("exited before announcing"),
            "error should mention port announcement failure"
        );
    }

    #[test]
    fn execute_transition_with_services_injects_port() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();

        make_state(
            root,
            "a",
            r#"
transitions:
  - command: "echo $PORT"
    target: "../b"
    services:
      - command: "python3 -c \"import socket,sys,time,signal; signal.signal(signal.SIGTERM,lambda *a:sys.exit(0)); s=socket.socket(); s.bind(('127.0.0.1',0)); port=s.getsockname()[1]; s.listen(1); print(f'listening on :{port}',file=sys.stderr,flush=True); time.sleep(300)\""
"#,
        );
        make_state(root, "b", "{}");

        let graph = StateGraph::discover(root, ".missouri").unwrap();
        let transition = &graph.transitions[0];
        let target = &graph.states[transition.target.0];

        let mut source_env = BTreeMap::new();
        source_env.insert("PATH".into(), "/usr/bin:/bin".into());

        let work = tempfile::tempdir().unwrap();
        let work_dir = Utf8Path::from_path(work.path()).unwrap();

        let result = execute_transition(
            transition,
            work_dir,
            &source_env,
            target,
            &graph,
            &BareBackend,
            false,
            None,
        );

        assert!(
            result.passed,
            "transition with service should pass; stderr: {}",
            result.stderr,
        );
        let port: u16 = result.stdout.trim().parse().unwrap_or(0);
        assert!(
            port > 0,
            "PORT should be injected as a valid port number, got stdout: '{}'",
            result.stdout,
        );
    }

    #[test]
    fn copy_state_to_temp_restores_dot_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();

        // Create a state with .missouri/dot-git/ and .missouri/dot-clc/
        let state_dir = root.join("a");
        let missouri_dir = state_dir.join(".missouri");

        let dot_git_dir = missouri_dir.join("dot-git");
        fs::create_dir_all(&dot_git_dir).unwrap();
        fs::write(dot_git_dir.join("HEAD"), "ref: refs/heads/main\n").unwrap();

        let dot_clc_dir = missouri_dir.join("dot-clc");
        fs::create_dir_all(&dot_clc_dir).unwrap();

        fs::write(
            missouri_dir.join("missouri.yml"),
            "transitions:\n  - command: \"echo\"\n    target: \"../b\"\n",
        )
        .unwrap();
        fs::write(state_dir.join("README.md"), "hello").unwrap();

        make_state(root, "b", "{}");

        let graph = StateGraph::discover(root, ".missouri").unwrap();
        let state_id = graph
            .states
            .iter()
            .find(|s| s.name == "a")
            .unwrap()
            .id;

        let (_temp_dir, work_dir) = copy_state_to_temp(state_id, &graph).unwrap();

        // .git/ restored from dot-git/
        assert!(work_dir.join(".git").exists());
        assert_eq!(
            fs::read_to_string(work_dir.join(".git/HEAD")).unwrap(),
            "ref: refs/heads/main\n"
        );
        // .clc/ restored from dot-clc/
        assert!(work_dir.join(".clc").exists());
        // Regular files copied
        assert!(work_dir.join("README.md").exists());
        // .missouri/ not copied
        assert!(!work_dir.join(".missouri").exists());
    }

    /// Helper: build a minimal state graph with a single state containing assertions.
    fn make_assertion_graph(tmp: &Utf8Path, assertions_yaml: &str) -> StateGraph {
        let state_dir = tmp.join("s");
        let missouri_dir = state_dir.join(".missouri");
        fs::create_dir_all(&missouri_dir).unwrap();
        fs::write(
            missouri_dir.join("missouri.yml"),
            format!("assertions:\n{assertions_yaml}"),
        )
        .unwrap();
        StateGraph::discover(tmp, ".missouri").unwrap()
    }

    #[test]
    fn should_fail_with_matching_stderr_passes() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();

        let graph = make_assertion_graph(
            root,
            r#"  - name: "fails with correct stderr"
    command: "sh -c 'echo wrong-stderr >&2; exit 1'"
    should_fail: true
    stderr: "wrong-stderr\n"
"#,
        );

        let assertion = &graph.assertions[0];
        let state_env = std::collections::BTreeMap::new();
        let work_dir = &graph.states[0].path;
        let backend = BareBackend;

        let result = run_single_assertion(assertion, work_dir, &state_env, &graph, &backend);
        assert!(
            result.passed,
            "should_fail assertion with matching stderr should pass, got error: {:?}",
            result.error
        );
        assert!(
            result.stderr_diff.is_none(),
            "stderr should match, but got diff: {:?}",
            result.stderr_diff
        );
    }

    #[test]
    fn should_fail_with_mismatched_stderr_fails() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();

        let graph = make_assertion_graph(
            root,
            r#"  - name: "fails with wrong stderr"
    command: "sh -c 'echo actual-error >&2; exit 1'"
    should_fail: true
    stderr: "expected-error\n"
"#,
        );

        let assertion = &graph.assertions[0];
        let state_env = std::collections::BTreeMap::new();
        let work_dir = &graph.states[0].path;
        let backend = BareBackend;

        let result = run_single_assertion(assertion, work_dir, &state_env, &graph, &backend);
        assert!(
            !result.passed,
            "should_fail assertion with mismatched stderr should fail"
        );
        assert!(
            result.stderr_diff.is_some(),
            "should report stderr diff when stderr doesn't match"
        );
    }

    #[test]
    fn should_fail_with_mismatched_stdout_fails() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();

        let graph = make_assertion_graph(
            root,
            r#"  - name: "fails with wrong stdout"
    command: "sh -c 'echo actual-output; exit 1'"
    should_fail: true
    stdout: "expected-output\n"
"#,
        );

        let assertion = &graph.assertions[0];
        let state_env = std::collections::BTreeMap::new();
        let work_dir = &graph.states[0].path;
        let backend = BareBackend;

        let result = run_single_assertion(assertion, work_dir, &state_env, &graph, &backend);
        assert!(
            !result.passed,
            "should_fail assertion with mismatched stdout should fail"
        );
        assert!(
            result.stdout_diff.is_some(),
            "should report stdout diff when stdout doesn't match"
        );
    }

    #[test]
    fn should_fail_without_output_expectations_still_passes() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();

        let graph = make_assertion_graph(
            root,
            r#"  - name: "fails without output checks"
    command: "sh -c 'echo noise >&2; exit 1'"
    should_fail: true
"#,
        );

        let assertion = &graph.assertions[0];
        let state_env = std::collections::BTreeMap::new();
        let work_dir = &graph.states[0].path;
        let backend = BareBackend;

        let result = run_single_assertion(assertion, work_dir, &state_env, &graph, &backend);
        assert!(
            result.passed,
            "should_fail without stdout/stderr expectations should still pass"
        );
    }
}
