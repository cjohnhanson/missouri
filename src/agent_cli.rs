//! The agent surface that `missouri agent eval` needs.
//!
//! This was `clc-sdk`, a crate in the clc workspace. clc is mothballed,
//! and missouri is the only tool that still needs this, so the surface
//! lives here. missouri depends on no other workspace crate.
//!
//! Only what the eval path uses is here. A spec from an eval file's
//! frontmatter, the defaults it overlays, the resolved config, and the
//! command that starts an agent.

use std::path::Path;
use std::process::Command;

use serde::{Deserialize, Serialize};

/// The agent settings an eval file may declare in its frontmatter.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct AgentSpec {
    /// The model, such as `haiku`, `sonnet`, or `opus`.
    #[serde(default)]
    pub model: Option<String>,
    /// The turn ceiling. No Claude Code flag sets this yet. The field
    /// stays for a future agent that supports one.
    #[serde(default)]
    pub max_turns: Option<u32>,
    /// The cost ceiling, in cents.
    #[serde(default)]
    pub max_cost_cents: Option<u32>,
    #[serde(default)]
    pub extra_args: Vec<String>,
    #[serde(default)]
    pub allowed_tools: Vec<String>,
}

/// The values a spec overlays.
#[derive(Debug, Clone)]
pub struct AgentDefaults {
    pub model: String,
    pub system_prompt: String,
    pub initial_prompt: String,
    pub extra_args: Vec<String>,
    pub allowed_tools: Vec<String>,
}

/// A spec resolved against its defaults.
#[derive(Debug, Clone)]
pub struct AgentConfig {
    pub model: String,
    pub system_prompt: String,
    pub initial_prompt: String,
    pub extra_args: Vec<String>,
    pub allowed_tools: Vec<String>,
}

impl AgentSpec {
    /// Parse a spec from YAML.
    pub fn from_yaml(yaml: &str) -> Result<Self, serde_yml::Error> {
        serde_yml::from_str(yaml)
    }

    /// Parse a spec from markdown with optional YAML frontmatter.
    ///
    /// Returns the spec and the body. A file with no frontmatter yields
    /// the default spec and the whole file as the body.
    pub fn from_markdown(text: &str) -> Result<(Self, String), serde_yml::Error> {
        let Some(rest) = text.strip_prefix("---\n") else {
            return Ok((Self::default(), text.to_string()));
        };
        let Some(end) = rest.find("\n---\n") else {
            return Ok((Self::default(), text.to_string()));
        };
        let spec = Self::from_yaml(&rest[..end])?;
        let body = rest[end + "\n---\n".len()..].to_string();
        Ok((spec, body))
    }

    /// Overlay this spec onto the defaults.
    ///
    /// A field the spec sets wins. The `extra_args` and `allowed_tools`
    /// lists concatenate, with the defaults first.
    #[must_use]
    pub fn to_agent_config(&self, defaults: &AgentDefaults) -> AgentConfig {
        let mut extra_args = defaults.extra_args.clone();
        if let Some(cents) = self.max_cost_cents {
            // Claude Code takes dollars, and the spec states cents.
            let dollars = f64::from(cents) / 100.0;
            extra_args.push("--max-budget-usd".to_string());
            extra_args.push(format!("{dollars:.2}"));
        }
        extra_args.extend(self.extra_args.clone());

        let mut allowed_tools = defaults.allowed_tools.clone();
        allowed_tools.extend(self.allowed_tools.clone());

        AgentConfig {
            model: self.model.clone().unwrap_or_else(|| defaults.model.clone()),
            system_prompt: defaults.system_prompt.clone(),
            initial_prompt: defaults.initial_prompt.clone(),
            extra_args,
            allowed_tools,
        }
    }
}

/// Build the command that starts a Claude Code agent.
#[must_use]
pub fn build_start_command(config: &AgentConfig, working_dir: &Path) -> Command {
    let mut cmd = Command::new("claude");
    cmd.current_dir(working_dir);
    cmd.arg("--print");
    cmd.arg("--verbose");
    cmd.arg("--input-format").arg("stream-json");
    cmd.arg("--output-format").arg("stream-json");
    cmd.arg("--model").arg(&config.model);
    cmd.arg("--append-system-prompt").arg(&config.system_prompt);
    if !config.allowed_tools.is_empty() {
        cmd.arg("--allowedTools");
        cmd.arg(config.allowed_tools.join(" "));
    }
    for arg in &config.extra_args {
        cmd.arg(arg);
    }
    // Clear the marker, so the child does not read itself as nested.
    cmd.env_remove("CLAUDECODE");
    cmd
}

/// One message on the agent's stdin stream.
#[derive(Debug, Clone, Serialize)]
pub struct InputMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub message: InputContent,
}

#[derive(Debug, Clone, Serialize)]
pub struct InputContent {
    pub role: String,
    pub content: String,
}

impl InputMessage {
    #[must_use]
    pub fn user(content: &str) -> Self {
        Self {
            msg_type: "user".into(),
            message: InputContent {
                role: "user".into(),
                content: content.into(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn defaults() -> AgentDefaults {
        AgentDefaults {
            model: "sonnet".into(),
            system_prompt: "sys".into(),
            initial_prompt: "go".into(),
            extra_args: vec!["--base".into()],
            allowed_tools: vec!["Read".into()],
        }
    }

    #[test]
    fn a_spec_field_overrides_the_default() {
        let spec = AgentSpec {
            model: Some("haiku".into()),
            ..AgentSpec::default()
        };
        assert_eq!(spec.to_agent_config(&defaults()).model, "haiku");
    }

    #[test]
    fn an_unset_field_keeps_the_default() {
        assert_eq!(
            AgentSpec::default().to_agent_config(&defaults()).model,
            "sonnet"
        );
    }

    #[test]
    fn the_lists_concatenate_with_the_defaults_first() {
        let spec = AgentSpec {
            extra_args: vec!["--extra".into()],
            allowed_tools: vec!["Glob".into()],
            ..AgentSpec::default()
        };
        let cfg = spec.to_agent_config(&defaults());
        assert_eq!(cfg.extra_args, vec!["--base", "--extra"]);
        assert_eq!(cfg.allowed_tools, vec!["Read", "Glob"]);
    }

    #[test]
    fn a_cost_ceiling_becomes_dollars() {
        // The spec states cents, and the flag takes dollars.
        let spec = AgentSpec {
            max_cost_cents: Some(250),
            ..AgentSpec::default()
        };
        let cfg = spec.to_agent_config(&defaults());
        assert!(cfg.extra_args.contains(&"--max-budget-usd".to_string()));
        assert!(cfg.extra_args.contains(&"2.50".to_string()));
    }

    #[test]
    fn markdown_frontmatter_parses_and_the_body_survives() {
        let text = "---\nmodel: opus\n---\nThe body.\n";
        let (spec, body) = AgentSpec::from_markdown(text).unwrap();
        assert_eq!(spec.model.as_deref(), Some("opus"));
        assert_eq!(body, "The body.\n");
    }

    #[test]
    fn a_file_without_frontmatter_is_all_body() {
        let (spec, body) = AgentSpec::from_markdown("Just prose.\n").unwrap();
        assert!(spec.model.is_none());
        assert_eq!(body, "Just prose.\n");
    }

    #[test]
    fn the_start_command_never_leaks_the_nested_marker() {
        let cfg = AgentSpec::default().to_agent_config(&defaults());
        let cmd = build_start_command(&cfg, Path::new("/tmp"));
        let removed: Vec<_> = cmd
            .get_envs()
            .filter(|(_, v)| v.is_none())
            .map(|(k, _)| k.to_string_lossy().into_owned())
            .collect();
        assert!(removed.contains(&"CLAUDECODE".to_string()));
    }

    #[test]
    fn a_user_message_serializes_to_the_stream_shape() {
        let json = serde_json::to_string(&InputMessage::user("hi")).unwrap();
        assert!(json.contains(r#""type":"user""#), "{json}");
        assert!(json.contains(r#""role":"user""#), "{json}");
        assert!(json.contains(r#""content":"hi""#), "{json}");
    }
}
