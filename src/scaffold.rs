use camino::{Utf8Path, Utf8PathBuf};

use crate::error::{Error, Result};

/// Initialize a new missouri project at `root`.
pub fn init_project(root: &Utf8Path, config_dir: &str) -> Result<()> {
    let config_path = root.join(config_dir);

    if config_path.exists() {
        return Err(Error::AlreadyInitialized { path: config_path });
    }

    std::fs::create_dir_all(&config_path)?;
    std::fs::write(config_path.join("missouri.yml"), "{}\n")?;
    std::fs::create_dir_all(config_path.join("bin"))?;
    std::fs::write(
        config_path.join("ignore"),
        "# Patterns to exclude from state comparison (gitignore syntax)\n",
    )?;

    Ok(())
}

/// Add a new state directory, optionally copying from an existing state.
pub fn add_state(root: &Utf8Path, config_dir: &str, name: &str, from: Option<&str>) -> Result<()> {
    let state_dir = root.join(name);

    if state_dir.exists() {
        return Err(Error::StateAlreadyExists {
            name: name.to_string(),
        });
    }

    match from {
        None => {
            let cfg_dir = state_dir.join(config_dir);
            std::fs::create_dir_all(&cfg_dir)?;
            std::fs::write(cfg_dir.join("missouri.yml"), "{}\n")?;
        }
        Some(source_name) => {
            let source_dir = root.join(source_name);
            if !source_dir.exists() || !source_dir.join(config_dir).join("missouri.yml").exists() {
                return Err(Error::SourceStateNotFound {
                    name: source_name.to_string(),
                });
            }

            copy_dir_all(&source_dir, &state_dir)?;
            append_transition(&source_dir, config_dir, name)?;
        }
    }

    Ok(())
}

/// Recursively copy a directory and all its contents.
fn copy_dir_all(src: &Utf8Path, dst: &Utf8Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = Utf8PathBuf::try_from(entry.path())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let file_name = src_path
            .file_name()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "no file name"))?;
        let dst_path = dst.join(file_name);
        let ft = entry.file_type()?;
        if ft.is_dir() {
            copy_dir_all(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

/// Append a placeholder transition to a state's missouri.yml.
fn append_transition(source_dir: &Utf8Path, config_dir: &str, target_name: &str) -> Result<()> {
    let config_path = source_dir.join(config_dir).join("missouri.yml");
    let content = std::fs::read_to_string(&config_path)?;

    let transition_item = format!(
        "  - name: \"TODO\"\n    command: \"echo TODO\"\n    target: \"../{target_name}\"\n"
    );

    let new_content = if content.contains("transitions:") {
        // Append to existing transitions list
        format!("{content}{transition_item}")
    } else {
        let trimmed = content.trim();
        if trimmed == "{}" || trimmed.is_empty() {
            // Replace empty config
            format!("transitions:\n{transition_item}")
        } else {
            // Append transitions section
            format!("{content}\ntransitions:\n{transition_item}")
        }
    };

    std::fs::write(&config_path, new_content)?;
    Ok(())
}
