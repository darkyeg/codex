//! Admit Windows Desktop's local WSL project roots before deserializing native
//! absolute paths. Project storage and responses continue to use Linux paths.

use codex_app_server_protocol::JSONRPCErrorError;
use codex_app_server_protocol::JSONRPCRequest;
use codex_utils_absolute_path::normalize_windows_device_path;

use crate::error_code::invalid_params;

pub(crate) fn normalize_local_wsl_roots(
    request: &mut JSONRPCRequest,
) -> Result<(), JSONRPCErrorError> {
    let distribution = if cfg!(target_os = "linux") {
        std::env::var("WSL_DISTRO_NAME")
            .ok()
            .filter(|name| !name.is_empty())
    } else {
        None
    };
    normalize_project_roots(request, distribution.as_deref())
}

fn normalize_project_roots(
    request: &mut JSONRPCRequest,
    distribution: Option<&str>,
) -> Result<(), JSONRPCErrorError> {
    if !matches!(
        request.method.as_str(),
        "project/create" | "project/import" | "project/update"
    ) {
        return Ok(());
    }
    let Some(distribution) = distribution else {
        return Ok(());
    };
    let Some(roots) = request
        .params
        .as_mut()
        .and_then(|params| params.get_mut("roots"))
        .and_then(serde_json::Value::as_array_mut)
    else {
        // Let the typed request decoder diagnose missing or malformed fields.
        return Ok(());
    };
    for root in roots {
        let Some(path) = root.get_mut("path") else {
            continue;
        };
        let Some(raw_path) = path.as_str() else {
            continue;
        };
        if let Some(local_path) = local_wsl_path(raw_path, distribution)? {
            *path = local_path.into();
        }
    }
    Ok(())
}

fn local_wsl_path(path: &str, distribution: &str) -> Result<Option<String>, JSONRPCErrorError> {
    let namespace_path = normalize_windows_device_path(path);
    let path = namespace_path.as_deref().unwrap_or(path).replace('\\', "/");
    let Some(unc) = path.strip_prefix("//") else {
        return Ok(None);
    };
    let (host, tail) = unc.split_once('/').unwrap_or((unc, ""));
    if !host.eq_ignore_ascii_case("wsl$") && !host.eq_ignore_ascii_case("wsl.localhost") {
        return Ok(None);
    }
    let (requested_distribution, relative_path) = tail.split_once('/').unwrap_or((tail, ""));
    if !requested_distribution.eq_ignore_ascii_case(distribution) {
        return Err(invalid_params(format!(
            "project root must name the current WSL distribution ({distribution})"
        )));
    }
    Ok(Some(format!("/{relative_path}")))
}

#[cfg(test)]
#[path = "project_paths_tests.rs"]
mod tests;
