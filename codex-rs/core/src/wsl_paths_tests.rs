use super::*;
use codex_config::types::McpServerConfig;
use codex_protocol::permissions::FileSystemAccessMode;
use codex_protocol::permissions::FileSystemSandboxEntry;
use codex_protocol::permissions::FileSystemSpecialPath;
use codex_protocol::permissions::NetworkSandboxPolicy;
use pretty_assertions::assert_eq;

fn state() -> anyhow::Result<SandboxState> {
    Ok(SandboxState {
        permission_profile: PermissionProfile::Managed {
            file_system: ManagedFileSystemPermissions::Restricted {
                entries: vec![
                    FileSystemSandboxEntry::new(
                        FileSystemPath::Special {
                            value: FileSystemSpecialPath::Root,
                        },
                        FileSystemAccessMode::Read,
                    ),
                    FileSystemSandboxEntry::new(
                        FileSystemPath::Path {
                            path: PathUri::parse("file:///home/alice/repo")?,
                        },
                        FileSystemAccessMode::Write,
                    ),
                    FileSystemSandboxEntry::new(
                        FileSystemPath::Path {
                            path: PathUri::parse("file:///home/alice/repo/secret")?,
                        },
                        FileSystemAccessMode::Deny,
                    ),
                    FileSystemSandboxEntry::new(
                        FileSystemPath::GlobPattern {
                            pattern: "/home/alice/repo/**/.env".to_string(),
                        },
                        FileSystemAccessMode::Deny,
                    ),
                    FileSystemSandboxEntry::new(
                        FileSystemPath::GlobPattern {
                            pattern: "**/private".to_string(),
                        },
                        FileSystemAccessMode::Deny,
                    ),
                ],
                glob_scan_max_depth: std::num::NonZeroUsize::new(8),
            },
            network: NetworkSandboxPolicy::Restricted,
        },
        codex_linux_sandbox_exe: Some(PathBuf::from("/usr/bin/codex")),
        sandbox_cwd: PathUri::parse("file:///home/alice/repo")?,
        use_legacy_landlock: true,
    })
}

#[test]
fn maps_workspace_and_denies_without_changing_permission_authority() -> anyhow::Result<()> {
    let original = state()?;
    let mapped = map_sandbox_paths(original.clone(), |path| {
        Ok(format!(r"\\wsl.localhost\NixOS{}", path.replace('/', "\\")))
    })?;
    let mut expected = original;
    expected.codex_linux_sandbox_exe = None;
    expected.sandbox_cwd = PathUri::parse("file://wsl.localhost/NixOS/home/alice/repo")?;
    if let PermissionProfile::Managed {
        file_system: ManagedFileSystemPermissions::Restricted { entries, .. },
        ..
    } = &mut expected.permission_profile
    {
        entries[1].path = FileSystemPath::Path {
            path: expected.sandbox_cwd.clone(),
        };
        entries[2].path = FileSystemPath::Path {
            path: expected.sandbox_cwd.join("secret")?,
        };
        entries[3].path = FileSystemPath::GlobPattern {
            pattern: "//wsl.localhost/NixOS/home/alice/repo/**/.env".to_string(),
        };
    }
    assert_eq!(mapped, expected);
    Ok(())
}

#[test]
fn real_wsl_translation_keeps_sandbox_glob_operators() -> anyhow::Result<()> {
    if !codex_utils_path::is_wsl() {
        return Ok(());
    }
    let mapped = map_sandbox_paths(state()?, |path| convert_path(path, PathConvention::Windows))?;
    let PermissionProfile::Managed {
        file_system: ManagedFileSystemPermissions::Restricted { entries, .. },
        ..
    } = mapped.permission_profile
    else {
        anyhow::bail!("sandbox profile changed");
    };
    let FileSystemPath::GlobPattern { pattern } = &entries[3].path else {
        anyhow::bail!("glob changed kind");
    };
    assert!(pattern.ends_with("/home/alice/repo/**/.env"));
    Ok(())
}

#[test]
fn ambiguous_glob_translation_fails_closed() -> anyhow::Result<()> {
    let mut original = state()?;
    if let PermissionProfile::Managed {
        file_system: ManagedFileSystemPermissions::Restricted { entries, .. },
        ..
    } = &mut original.permission_profile
    {
        entries[3].path = FileSystemPath::GlobPattern {
            pattern: "/home/alice/repo/**/literal\\*.env".to_string(),
        };
    }
    let result = map_sandbox_paths(original, |path| {
        Ok(format!(r"C:\mapped{}", path.replace('/', "\\")))
    });
    assert!(result.is_err());
    Ok(())
}

#[test]
fn mapping_failure_does_not_emit_partial_sandbox_state() -> anyhow::Result<()> {
    let result = map_sandbox_paths(state()?, |path| {
        if path.ends_with("secret") {
            anyhow::bail!("cannot translate deny path");
        }
        Ok(format!(r"C:\mapped{}", path.replace('/', "\\")))
    });
    assert_eq!(
        result
            .expect_err("deny conversion must fail closed")
            .to_string(),
        "cannot translate deny path"
    );
    Ok(())
}

#[test]
fn already_windows_uris_remain_unchanged() -> anyhow::Result<()> {
    let original = SandboxState {
        permission_profile: PermissionProfile::Disabled,
        sandbox_cwd: PathUri::parse("file:///C:/Alice%20Smith/repo")?,
        codex_linux_sandbox_exe: None,
        use_legacy_landlock: false,
    };
    assert_eq!(
        map_sandbox_paths(original.clone(), |_| anyhow::bail!("unexpected conversion"))?,
        original,
    );
    Ok(())
}

#[test]
fn native_and_remote_servers_keep_their_paths() -> anyhow::Result<()> {
    let linux: McpServerConfig =
        serde_json::from_value(serde_json::json!({"command":"/usr/bin/node"}))?;
    let windows: McpServerConfig =
        serde_json::from_value(serde_json::json!({"command":"/mnt/c/runtime/node_repl.exe"}))?;
    let original = state()?;
    assert_eq!(
        sandbox_state(original.clone(), Some(&linux.transport), "local")?,
        original
    );
    assert_eq!(
        sandbox_state(original.clone(), Some(&windows.transport), "remote")?,
        original
    );
    assert_eq!(
        sandbox_state(original.clone(), /*transport*/ None, "local")?,
        original
    );
    Ok(())
}

#[test]
fn detects_only_direct_windows_stdio_commands() -> anyhow::Result<()> {
    for (command, expected) in [
        ("/mnt/c/runtime/node_repl.exe", true),
        (r"C:\runtime\node_repl.EXE", true),
        ("/usr/bin/node", false),
        ("/bin/sh", false),
        ("relative.exe", false),
    ] {
        let server: McpServerConfig =
            serde_json::from_value(serde_json::json!({"command":command}))?;
        assert_eq!(windows_stdio_command(Some(&server.transport)), expected);
    }
    Ok(())
}

#[test]
fn local_input_keeps_native_paths_and_other_user_input() {
    let image = UserInput::LocalImage {
        path: PathBuf::from("relative/image.png"),
        detail: None,
    };
    let text = UserInput::Text {
        text: r"C:\not-an-attachment".to_string(),
        text_elements: Vec::new(),
    };
    assert_eq!(local_image_input(image.clone()), image);
    assert_eq!(local_image_input(text.clone()), text);
}

#[test]
fn translated_uri_preserves_unicode_and_reserved_characters() -> anyhow::Result<()> {
    let original = SandboxState {
        permission_profile: PermissionProfile::Disabled,
        sandbox_cwd: PathUri::parse("file:///mnt/c/Alice%20Smith/%D8%B5%D9%88%D8%B1/%23%25")?,
        codex_linux_sandbox_exe: None,
        use_legacy_landlock: false,
    };
    let mapped = map_sandbox_paths(original, |path| {
        assert_eq!(path, "/mnt/c/Alice Smith/صور/#%");
        Ok(r"C:\Alice Smith\صور\#%".to_string())
    })?;
    assert_eq!(
        mapped.sandbox_cwd,
        PathUri::parse("file:///C:/Alice%20Smith/%D8%B5%D9%88%D8%B1/%23%25")?
    );
    Ok(())
}
