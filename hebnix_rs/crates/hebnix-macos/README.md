# hebnix-macos

The macOS platform layer of this Hebnix fork, kept in its own crate so it can be
built and tested before `hebnix-app` (whose every platform module is gated on
`target_os = "windows"` or `"linux"`) is taught about a third target.

Fork of [HebnixRL/Hebnix-Public](https://github.com/HebnixRL/Hebnix-Public),
Apache-2.0, same as upstream.

## What is here

| Module | Job |
|---|---|
| `obelisk` | Find Rocket League inside Obelisk's library. Locates installs by contents (`TAGame/CookedPCConsole`), since Obelisk owns the folder names. Derives the Wine prefix from the install folder and reports the store when the binaries make it obvious. |
| `patcher` | Apply and revert file swaps with backups and a hash manifest, and tell "still swapped" apart from "reverted by an update or Verify game files". |
| `launch` | Hand the launch to Obelisk. Deliberately cannot start the game: Obelisk owns the prefix, the Voile runtime and the EAC bootstrapper. |
| `process` | Is the game running. macOS has no `/proc`, so this reads `ps` and matches the game *and* the runtime that presents it (`fexdarwin-run`, `wsd`). |

Everything in it is dependency-free and covered by `cargo test -p hebnix-macos`.

## Port order (what is not here yet)

Upstream has 26 `*_linux.rs` counterparts. In rough dependency order:

1. `sys/macos.rs` — what `sys/windows.rs` and `sys/linux.rs` provide (paths, config dirs, process lookup, open-in-file-manager).
2. `patcher/game_path.rs` — already `cfg`-split; macOS only needs the plain case-insensitive join, so a `target_os = "macos"` arm.
3. `rl_launch` — replaced by `launch` here, since Obelisk launches.
4. `hotkey`, `monitor`, `window_state`, `visibility`, `dpi_fix` — small; `scale_factor` replaces DPI.
5. `webview` — WKWebView instead of WebView2/WebKitGTK.
6. `overlay` — an `NSPanel` above `wsd`'s surface, replacing DirectComposition and layer-shell.
7. `screen` capture — ScreenCaptureKit.
8. `spoofer` — a local HTTPS proxy; portable, but needs a trusted root CA and `/etc/hosts`, so it stays opt-in and last.
9. `plugins/media_session`, `plugins/window_capture`, `replay_process_memory`, `multiplayer_lan`, `recovery_tasks`, `watchdog`.

## Build

```sh
cargo test -p hebnix-macos
```

Nothing here can be verified from Linux; it targets macOS only.
