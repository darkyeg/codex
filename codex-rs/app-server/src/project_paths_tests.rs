use super::normalize_project_roots;
use crate::error_code::INVALID_PARAMS_ERROR_CODE;
#[cfg(unix)]
use codex_app_server_protocol::ClientRequest;
use codex_app_server_protocol::JSONRPCRequest;
use pretty_assertions::assert_eq;
use serde_json::json;

#[test]
fn local_wsl_aliases_decode_as_native_project_roots() {
    for path in [
        r"\\wsl$\NixOS\home\darky\work\Example Project",
        r"\\WSL.LOCALHOST\nixos\home\darky\work\Example Project",
        r"\\?\UNC\wsl.localhost\NixOS\home\darky\work\Example Project",
        r"\\.\UNC\wsl$\NixOS\home\darky\work\Example Project",
        "//wsl.localhost/NixOS/home/darky/work/Example Project",
    ] {
        for method in ["project/create", "project/import", "project/update"] {
            let mut request: JSONRPCRequest = serde_json::from_value(json!({
                "id": 1,
                "method": method,
                "params": {
                    "name": "example",
                    "projectId": "existing",
                    "idempotencyKey": "example",
                    "roots": [{"path": path}, {"path": "/native/root"}],
                    "metadata": {"reference": path}
                }
            }))
            .unwrap();
            let mut expected = request.clone();
            expected.params.as_mut().unwrap()["roots"][0]["path"] =
                json!("/home/darky/work/Example Project");

            normalize_project_roots(&mut request, Some("NixOS")).unwrap();

            assert_eq!(request, expected);
            #[cfg(unix)]
            assert!(ClientRequest::try_from(request).is_ok());
        }
    }
}

#[test]
fn conversion_is_limited_to_project_roots_on_a_known_wsl_distribution() {
    for (method, distribution) in [
        ("project/create", None),
        ("thread/start", Some("NixOS")),
        ("config/value/write", Some("NixOS")),
    ] {
        let mut request: JSONRPCRequest = serde_json::from_value(json!({
            "id": 1,
            "method": method,
            "params": {"roots": [{"path": r"\\wsl$\NixOS\home\user"}]}
        }))
        .unwrap();
        let expected = request.clone();
        normalize_project_roots(&mut request, distribution).unwrap();
        assert_eq!(request, expected);
    }
}

#[test]
fn a_wsl_root_must_name_the_current_distribution() {
    for path in [
        r"\\wsl$\Ubuntu\home\user",
        "//wsl.localhost/Ubuntu/home/user",
        r"\\wsl$",
        "//wsl.localhost/",
    ] {
        let mut request: JSONRPCRequest = serde_json::from_value(json!({
            "id": 1,
            "method": "project/create",
            "params": {"roots": [{"path": path}]}
        }))
        .unwrap();
        let error = normalize_project_roots(&mut request, Some("NixOS")).unwrap_err();
        assert_eq!(error.code, INVALID_PARAMS_ERROR_CODE);
    }
}

#[test]
fn native_paths_other_shares_and_malformed_fields_retain_normal_validation() {
    for roots in [
        json!([{"path": "/home/user/near0"}]),
        json!([{"path": r"\\server\share\project"}]),
        json!([{"path": r"C:\work\project"}]),
        json!([{"path": "relative/project"}]),
        json!([{"path": 42}, {}]),
        json!(null),
        json!({"path": r"\\wsl$\NixOS\home\user"}),
    ] {
        let mut request: JSONRPCRequest = serde_json::from_value(json!({
            "id": 1,
            "method": "project/update",
            "params": {"roots": roots}
        }))
        .unwrap();
        let expected = request.clone();
        normalize_project_roots(&mut request, Some("NixOS")).unwrap();
        assert_eq!(request, expected);
    }
}
