# WSL interoperability branch

This branch is based on OpenAI's `rust-v0.153.3` tag. It is a local maintenance
fork, not an official Codex release or a complete implementation of Desktop
browser support in WSL.

## Changes

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

The boundary conversion lives in `codex-rs/core/src/wsl_paths.rs`. It does not
change the MCP wire schema, model confirmation policies, or approval decisions.

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
also needs its workspace package versions synchronized to `0.153.3`; this
does not update external dependencies.

## Desktop browser limitation

Opening a page in the Desktop browser and letting the agent control that
browser are separate capabilities. Desktop version `26.901.5003.0` returns
`wsl-disabled` for agent browser control while running in WSL. Its browser
pane can still display pages.

This branch does not modify Desktop bundles, enable unavailable browser
backends, or claim successful browser automation. Desktop browser control in
WSL remains unresolved.
