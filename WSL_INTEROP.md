# WSL interoperability branch

This branch is based on OpenAI's `main` at `1715e55076737158ba61d43158ede504de6d4ce1`
(2026-09-13), with the local fixes carried forward from `rust-v0.153.3`. It is
a maintenance fork, not an official Codex release or a complete implementation
of Desktop browser support in WSL.

## Changes

- Project create, import, and update accept Windows UNC roots for the current
  WSL distribution before the native absolute-path decoder runs. Both `wsl$`
  and `wsl.localhost` aliases map to Linux roots; a different distribution is
  rejected. This fixes Desktop requests that previously failed with
  `AbsolutePathBuf deserialized without a base path`. The app-server API
  [documents the boundary](codex-rs/app-server/README.md#wsl-project-roots-maintenance-build).
- Windows image attachment paths are translated with the distribution's
  `wslpath` before local image preparation. This also covers `view_image` when
  the selected executor is the local WSL environment.
- Sandbox metadata for a directly configured local Windows `.exe` MCP peer
  uses Windows file URIs. Literal filesystem rules and absolute glob prefixes
  are translated along with the working directory. Access modes, deny rules,
  network policy, and rule options are retained. Ambiguous glob translations
  fail closed. A Linux sandbox executable is not sent to a Windows peer.
- Native Linux servers, remote executors, and non-WSL hosts retain their
  existing path handling.

The image/MCP boundary conversion lives in `codex-rs/core/src/wsl_paths.rs`;
project input conversion lives in `codex-rs/app-server/src/project_paths.rs`.
Neither changes the MCP wire schema, model confirmation policies, or approval decisions.

## Installed-plugin startup

`plugin/installed` selects installed identities and explicit suggestions before
hydrating catalog entries from local plugin manifests. Previously it hydrated
all entries and discarded unrelated plugins afterward. This matters on WSL's
Windows filesystem and for large catalogs; the fix applies on every platform.
Catalog JSON admission, policy restrictions, scope precedence, installed state,
and remote plugin reconciliation remain in their existing owners. Catalog
browsing and installation still use the complete catalog. No persistent metadata
cache or stale-data fallback is introduced.

On the same NixOS/Windows data, two isolated `plugin/installed` requests under
`strace -f -c` fell from 7.817/9.316 seconds to 1.440/1.069 seconds. Both runs
returned the same marketplace/plugin counts (3 marketplaces, 12 then 13 plugins
as background reconciliation completed), with no catalog errors. `statx` calls
fell from 2,918 to 321. These measurements isolate the server query; they do not
establish the total Desktop conversation-open latency. The selected 57 API
regressions passed, including catalog browsing, suggestions, policy restrictions,
remote reconciliation, and plugin state merged across project scopes.

## Shared Windows/WSL configuration

Both the WSL agent and native Windows tool helpers may load the same Codex
configuration. A Linux-only absolute `model_catalog_json` path makes the
Windows helper fail during configuration loading. Prefer a path relative to
the configuration file, for example:

```toml
model_catalog_json = "model-catalogs/custom.json"
```

Keep the native Windows helper on a Windows executable. Pointing its
`CODEX_CLI_PATH` at an ELF binary or a Linux shell script cannot work.

## Validation

### Latest main (2026-09-13)

All 39 selected app-server/core regressions passed on upstream `1715e550` with
both maintenance fixes applied. These cover project create/import/update and
persistence, UNC alias admission and rejection, image paths, and local Windows
MCP sandbox metadata. The initial combined run needed the repository's
`test_stdio_server` fixture built explicitly; all 15 affected cases passed on
rerun after building it.

```sh
cargo build --locked --profile dev-small -p codex-rmcp-client --bin test_stdio_server
just test -p codex-app-server -p codex-core --cargo-profile dev-small \
  -E 'test(project_paths) | test(project_wsl_paths) | test(v2::projects) | test(wsl_paths) | test(wsl_windows_image) | test(wsl_stdio_exe) | test(stdio_mcp_tool_call_includes_sandbox_state_meta) | test(view_image::view_image_routes_to_selected_local_environment)'
```

The latest standalone app server and complete CLI each passed the ten native WSL
project smoke checks described below. The matching code-mode host passed its two
native TCP/stdio integration tests for persistent values, delegated tools,
notifications, cell control, and session closure. Main retains its upstream development version `0.0.0`;
no release-version lockfile rewrite is applied.

For the complete runtime build, use the repository's package builder or its
`scripts/codex_package/v8.py` provisioning helper. Both `RUSTY_V8_ARCHIVE` and
`RUSTY_V8_SRC_BINDING_PATH` must refer to the matching Codex-built sandbox V8
artifacts verified against the repository's trusted checksum manifest. The
crate's default denoland archive URL does not provide this sandbox artifact.
Keep the code-mode host built from the same source as the CLI.

### Project roots on the previous 0.153.3 base (2026-09-13)

The installed pre-patch executable rejected both WSL UNC aliases with
`Invalid request: AbsolutePathBuf deserialized without a base path`; the same
request with a Linux root succeeded. The patched app server passed 12 selected
tests, including the existing project persistence, import atomicity, assignment,
fork, deletion, and validation scenarios:

```sh
just test -p codex-app-server --cargo-profile dev-small --test-threads 3 \
  -E 'test(project_paths) | test(project_wsl_paths) | test(v2::projects)'
```

An isolated native WSL smoke check also passed create/retry through the two UNC
aliases, import, update, metadata preservation, rejection of foreign distributions,
duplicate aliases and relative paths, persistence across a server restart, and
deletion without removing the project directory. This verifies server behavior;
Desktop's sidebar still needs its normal project-creation flow. The server does
not modify `.codex-global-state.json`.

### Previous image and MCP fixes

On a real NixOS WSL installation, 26 focused regression tests passed, covering
image ingestion, `view_image`, Windows MCP metadata, unchanged remote/native
behavior, and model-supplied confirmation policies:

```sh
just test -p codex-core --test-threads 4 -E 'test(wsl_paths) | test(wsl_windows_image) | test(wsl_stdio_exe) | test(stdio_mcp_tool_call_includes_sandbox_state_meta) | test(view_image::view_image_routes_to_selected_local_environment)'
```

Separate ephemeral app-server tests used a loopback model fixture and the
official Windows Node REPL server. They verified a Windows-drive PNG
attachment, native JavaScript execution from a WSL workspace, and
`@oai/sky.list_apps()` under a read-only permission profile.

The broader core run completed 4,085 tests: 4,056 passed, 28 failed, and one
timed out; nine other tests were skipped. That run was not clean. Failures
included local runtime-library loading, shell fixtures, and timing-sensitive
tests. `just fix -p codex-core`, `just fmt`, and `just bazel-lock-update` also ran.
No claim is made that the entire workspace test suite passes.

Run tests with an isolated `CODEX_HOME`, without an inherited
`CODEX_SQLITE_HOME` or API credentials. On NixOS, sandbox child processes need
their runtime libraries in the executable's RPATH; an inherited
`LD_LIBRARY_PATH` alone is insufficient. The source release's Cargo lockfile
needed its workspace package versions synchronized to `0.153.3`; that historical
release adjustment does not apply to the current main-based branch.

## Desktop browser limitation

Opening a page in the Desktop browser and letting the agent control that
browser are separate capabilities. Desktop version `26.901.5003.0` returns
`wsl-disabled` for agent browser control while running in WSL. Its browser
pane can still display pages.

This branch does not modify Desktop bundles, enable unavailable browser
backends, or claim successful browser automation. Desktop browser control in
WSL remains unresolved.

## Local Desktop integration

The tested Desktop build recognizes the `CODEX_ELECTRON_RESOURCES_PATH`
development override. A separate resource directory can supply the patched
Linux `codex` and the matching official Windows `codex.exe`, with their
required helpers and original resource assets. This avoids assigning a Linux
executable to a Windows helper through the global `CODEX_CLI_PATH` override.

The local resource directory preserves the original `app.asar` and Windows
executables. Desktop's integrity checks remain enabled. A complete app restart
is required to load the resource override; runtime smoke tests are separate
from verification after that restart.

This is a version-specific development installation. Before upgrading
Desktop, resynchronize and validate the resources or remove the override to
return to the bundled runtime. No modified Desktop binary or proprietary
resource bundle is distributed by this repository.
