use anyhow::Result;
use app_test_support::MockResponsesConfig;
use app_test_support::TestAppServer;
use app_test_support::create_mock_responses_server_repeating_assistant;
use codex_app_server_protocol::ProjectCreateResponse;
use codex_app_server_protocol::ProjectImportResponse;
use codex_app_server_protocol::ProjectListResponse;
use codex_app_server_protocol::ProjectReadResponse;
use codex_app_server_protocol::ProjectUpdateResponse;
use codex_app_server_protocol::RequestId;
use codex_features::Feature;
use codex_utils_absolute_path::AbsolutePathBuf;
use pretty_assertions::assert_eq;
use serde_json::json;
use tempfile::TempDir;

#[tokio::test]
async fn wsl_project_roots_survive_create_import_update_and_server_restart() -> Result<()> {
    let responses = create_mock_responses_server_repeating_assistant("Done").await;
    let codex_home = TempDir::new()?;
    let workspace = TempDir::new()?;
    let native_path = workspace.path().join("Mixed Case Project");
    std::fs::create_dir(&native_path)?;
    let native_root = AbsolutePathBuf::from_absolute_path(&native_path)?;
    let tail = native_path.to_string_lossy().replace('/', r"\");
    let windows_root = format!(r"\\wsl$\NixOS{tail}");
    let localhost_root = format!(r"\\wsl.localhost\nixos{tail}");
    MockResponsesConfig::new(&responses.uri())
        .enable_feature(Feature::Sqlite)
        .write(codex_home.path())?;
    let mut server = TestAppServer::builder()
        .with_codex_home(codex_home.path())
        .with_env_overrides(&[("WSL_DISTRO_NAME", Some("NixOS"))])
        .build_initialized()
        .await?;

    let request = server
        .send_raw_request(
            "project/create",
            Some(json!({
                "name": "created",
                "roots": [{"path": windows_root}],
                "metadata": {"originalPath": windows_root},
                "idempotencyKey": "create-wsl-project"
            })),
        )
        .await?;
    let created: ProjectCreateResponse = server.read_response(request).await?;
    assert_eq!(created.project.roots[0].path, native_root);
    assert_eq!(created.project.metadata["originalPath"], windows_root);

    let request = server
        .send_raw_request(
            "project/create",
            Some(json!({
                "name": "created",
                "roots": [{"path": localhost_root}],
                "idempotencyKey": "create-wsl-project"
            })),
        )
        .await?;
    let retry: ProjectCreateResponse = server.read_response(request).await?;
    assert_eq!(retry.project, created.project);

    let request = server
        .send_raw_request(
            "project/import",
            Some(json!({
                "name": "imported",
                "roots": [{"path": localhost_root}],
                "idempotencyKey": "import-wsl-project"
            })),
        )
        .await?;
    let imported: ProjectImportResponse = server.read_response(request).await?;
    assert_eq!(imported.project.roots, created.project.roots);

    let request = server
        .send_raw_request(
            "project/create",
            Some(json!({
                "name": "duplicate aliases",
                "roots": [{"path": windows_root}, {"path": localhost_root}],
                "idempotencyKey": "duplicate-wsl-project"
            })),
        )
        .await?;
    let error = server
        .read_stream_until_error_message(RequestId::Integer(request))
        .await?;
    assert_eq!(error.error.code, -32602);

    let updated_root = native_root.join("updated");
    let request = server
        .send_raw_request(
            "project/update",
            Some(json!({
                "projectId": created.project.id,
                "roots": [{"path": format!(r"{localhost_root}\updated")}]
            })),
        )
        .await?;
    let updated: ProjectUpdateResponse = server.read_response(request).await?;
    let mut expected = created.project;
    expected.roots[0].path = updated_root;
    expected.updated_at = updated.project.updated_at;
    assert_eq!(updated.project, expected);

    let request = server
        .send_raw_request(
            "project/update",
            Some(json!({
                "projectId": expected.id,
                "roots": [{"path": r"\\wsl$\DifferentDistro\home\user\project"}]
            })),
        )
        .await?;
    let error = server
        .read_stream_until_error_message(RequestId::Integer(request))
        .await?;
    assert_eq!(error.error.code, -32602);

    server.shutdown_gracefully().await?;
    let mut restarted = TestAppServer::builder()
        .with_codex_home(codex_home.path())
        .with_env_overrides(&[("WSL_DISTRO_NAME", Some("NixOS"))])
        .build_initialized()
        .await?;
    let request = restarted
        .send_raw_request(
            "project/read",
            Some(json!({
                "projectId": expected.id
            })),
        )
        .await?;
    let read: ProjectReadResponse = restarted.read_response(request).await?;
    assert_eq!(read.project, expected);
    let request = restarted
        .send_raw_request("project/list", Some(json!({})))
        .await?;
    let listed: ProjectListResponse = restarted.read_response(request).await?;
    assert_eq!(listed.data, vec![expected, imported.project]);
    Ok(())
}

#[tokio::test]
async fn windows_project_roots_are_not_reinterpreted_without_a_wsl_distribution() -> Result<()> {
    let mut server = TestAppServer::builder()
        .with_env_overrides(&[("WSL_DISTRO_NAME", None)])
        .build_initialized()
        .await?;
    let request = server
        .send_raw_request(
            "project/create",
            Some(json!({
                "name": "not-local",
                "roots": [{"path": r"\\wsl$\NixOS\home\user\project"}],
                "idempotencyKey": "not-local"
            })),
        )
        .await?;
    let error = server
        .read_stream_until_error_message(RequestId::Integer(request))
        .await?;
    assert_eq!(error.error.code, -32600);
    Ok(())
}
