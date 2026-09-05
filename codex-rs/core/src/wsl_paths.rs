//! Translate paths at the local WSL/Windows process boundary, without changing
//! sandbox access modes or interpreting paths belonging to remote executors.

use anyhow::Context;
use codex_config::types::McpServerTransportConfig;
use codex_mcp::SandboxState;
use codex_protocol::models::ManagedFileSystemPermissions;
use codex_protocol::models::PermissionProfile;
use codex_protocol::permissions::FileSystemPath;
use codex_protocol::user_input::UserInput;
use codex_utils_path_uri::LegacyAppPathString;
use codex_utils_path_uri::PathConvention;
use codex_utils_path_uri::PathUri;
use std::borrow::Cow;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

fn convert_path(path: &str, target: PathConvention) -> anyhow::Result<String> {
    let flag = match target {
        PathConvention::Posix => "-u",
        PathConvention::Windows => "-w",
    };
    let output = Command::new("wslpath")
        .args([flag, path])
        .output()
        .context("unable to run WSL's path translator")?;
    anyhow::ensure!(
        output.status.success(),
        "WSL could not translate path {path:?}"
    );
    let translated = String::from_utf8(output.stdout).context("WSL returned a non-UTF-8 path")?;
    let translated = translated.trim_end_matches(['\r', '\n']);
    // Validate the translator's output rather than accepting a relative path.
    LegacyAppPathString::from_string(translated)
        .to_path_uri(target)
        .context("WSL returned a path for the wrong operating system")?;
    Ok(translated.to_string())
}

pub(crate) fn local_image_path(path: &str) -> anyhow::Result<Cow<'_, str>> {
    if codex_utils_path::is_wsl()
        && LegacyAppPathString::from_string(path).infer_absolute_path_convention()
            == Some(PathConvention::Windows)
    {
        return convert_path(path, PathConvention::Posix).map(Cow::Owned);
    }
    Ok(Cow::Borrowed(path))
}

pub(crate) fn local_image_input(input: UserInput) -> UserInput {
    let UserInput::LocalImage { path, detail } = input else {
        return input;
    };
    let Some(path_text) = path.to_str() else {
        return UserInput::LocalImage { path, detail };
    };
    match local_image_path(path_text) {
        Ok(Cow::Borrowed(_)) => UserInput::LocalImage { path, detail },
        Ok(Cow::Owned(mapped)) => UserInput::LocalImage {
            path: PathBuf::from(mapped),
            detail,
        },
        Err(error) => UserInput::Text {
            text: format!(
                "Codex could not read the local image at `{}`: {error:#}",
                path.display()
            ),
            text_elements: Vec::new(),
        },
    }
}

fn windows_stdio_command(transport: Option<&McpServerTransportConfig>) -> bool {
    matches!(transport, Some(McpServerTransportConfig::Stdio { command, .. })
        if command.to_ascii_lowercase().ends_with(".exe")
            && (Path::new(command).is_absolute()
                || LegacyAppPathString::from_string(command).infer_absolute_path_convention()
                    == Some(PathConvention::Windows)))
}

pub(crate) fn sandbox_state(
    state: SandboxState,
    transport: Option<&McpServerTransportConfig>,
    environment_id: &str,
) -> anyhow::Result<SandboxState> {
    if !codex_utils_path::is_wsl()
        || environment_id != codex_config::DEFAULT_MCP_SERVER_ENVIRONMENT_ID
        || !windows_stdio_command(transport)
    {
        return Ok(state);
    }
    map_sandbox_paths(state, |path| convert_path(path, PathConvention::Windows))
}

fn map_sandbox_paths(
    mut state: SandboxState,
    mut map: impl FnMut(&str) -> anyhow::Result<String>,
) -> anyhow::Result<SandboxState> {
    let map_uri = |path: &PathUri, map: &mut dyn FnMut(&str) -> anyhow::Result<String>| {
        if path.infer_path_convention() != Some(PathConvention::Posix) {
            return Ok(path.clone());
        }
        let native = LegacyAppPathString::from_path_uri(path, PathConvention::Posix)?;
        Ok::<_, anyhow::Error>(
            LegacyAppPathString::from_string(map(native.as_str())?)
                .to_path_uri(PathConvention::Windows)?,
        )
    };
    state.sandbox_cwd = map_uri(&state.sandbox_cwd, &mut map)?;
    // A Windows peer must use its Windows sandbox launcher. The Linux ELF
    // helper cannot be executed by that process, even through a translated path.
    state.codex_linux_sandbox_exe = None;
    if let PermissionProfile::Managed {
        file_system: ManagedFileSystemPermissions::Restricted { entries, .. },
        ..
    } = &mut state.permission_profile
    {
        for entry in entries {
            match &mut entry.path {
                FileSystemPath::Path { path } => *path = map_uri(path, &mut map)?,
                FileSystemPath::GlobPattern { pattern }
                    if pattern.starts_with('/') && !pattern.starts_with("//") =>
                {
                    // wslpath encodes wildcard characters as literal Windows
                    // filename characters. Translate only the literal prefix.
                    if let Some(wildcard) = pattern.find(['*', '?', '[', '{']) {
                        let prefix_end = pattern[..wildcard].rfind('/').unwrap_or(0);
                        let (prefix, suffix) = pattern.split_at(prefix_end);
                        anyhow::ensure!(
                            !suffix.chars().any(|c| c.is_control()
                                || matches!(c, '<' | '>' | ':' | '"' | '|' | '\\')),
                            "cannot safely translate Windows sandbox glob {pattern:?}"
                        );
                        let prefix = map(if prefix.is_empty() { "/" } else { prefix })?;
                        *pattern = format!(
                            "{}{suffix}",
                            prefix.trim_end_matches('\\').replace('\\', "/")
                        );
                    } else {
                        *pattern = map(pattern)?.replace('\\', "/");
                    }
                }
                FileSystemPath::GlobPattern { .. } | FileSystemPath::Special { .. } => {}
            }
        }
    }
    Ok(state)
}

#[cfg(test)]
#[path = "wsl_paths_tests.rs"]
mod tests;
