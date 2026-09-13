# ADB Manager

A production-quality **Windows desktop Android device management and debugging toolkit**,
built entirely in **Rust** with a native GUI (`eframe` + `egui`).

No Electron. No web UI. No Node.js. No Python. Just Rust.

> **Status: all 11 phases implemented.** Device management, wireless
> pairing (code + QR), apps, processes, logcat + crash analyzer, APK
> inspector/installer, file browser, interactive shell, device tools
> (screenshot, recording, battery/memory/storage/properties, reboot) and
> polish (command palette, shortcuts, diagnostics) are working.
>
> The interface is a centralized dark design system (`ui/theme` +
> `ui/components`: #0D1117 backgrounds, accent/semantic colors, shared
> buttons, badges, panels, tables, empty/loading/error states) applied
> consistently across the sidebar, header, status bar and every page.

## Features (Phase 1)

- Native dark developer-tool UI: sidebar, global header with device selector,
  bottom status bar, toasts, first-run ADB onboarding
- Centralized ADB layer — the UI never spawns `adb.exe` directly:
  `UI → AppState → DeviceManager → AdbClient → AdbCommandBuilder → adb.exe`
- ADB executable auto-detection (PATH + common Windows install locations),
  manual browse (`rfd` native dialog), `adb version` validation
- Robust `adb devices -l` parsing: USB / wireless / emulator, unauthorized /
  offline / unknown states, multi-device
- Background device polling (2 s default) over channels — the UI thread never
  blocks on a subprocess
- Per-device details (`getprop`, `wm size`/`wm density`) fetched in background
  and cached per serial
- `adb connect` / `adb disconnect` with output-aware success detection
- Saved wireless devices (`devices.json`) with startup auto-reconnect
- Wireless Debugging pairing: pairing-code form, live camera QR scan with
  local-only decoding, QR-screenshot fallback, explicit pairing state
- Structured errors (`AdbError`) with user guidance + expandable technical details
- Config in `%APPDATA%\ADBManager\config.json`, file logging via `tracing`
- Mock mode for UI development without hardware: `ADB_MANAGER_MOCK=1`
- 13 unit tests covering parsers, commands, config and event diffing

## Requirements

- Windows 10/11 x64 (ARM64-ready architecture)
- Rust stable (1.88+; tested on 1.98)
- Android Platform Tools (`adb.exe`) — auto-detected, or pick it in
  **Settings → ADB Environment**

## Building

```powershell
cargo build --release
```

Run:

```powershell
cargo run --release
```

The binary is `target\release\adb-manager.exe` (packaged as `ADBManager.exe`).

Development without a device:

```powershell
$env:ADB_MANAGER_MOCK=1; cargo run
```

## Keyboard shortcuts

| Shortcut | Action |
|---|---|
| `Ctrl+K` | Command palette (pages, Connect Device, Refresh) |
| `Ctrl+Shift+L` / `A` / `S` | Logcat / Apps / Shell |
| `Ctrl+R` | Refresh device list |
| `Esc` | Close palette / dialogs |
| `↑` / `↓` in Shell input | Command history |

## Packaging (Windows)

```powershell
cargo build --release   # → target\release\adb-manager.exe
```

Rename to `ADBManager.exe` for distribution. The app is portable: config
lives in `%APPDATA%\ADBManager\` (`config.json`, `devices.json`, `logs/`)
and is created on first launch — no installer or registry keys needed.
Optional: ship `platform-tools\adb.exe` next to the binary; the app also
auto-detects SDK installs and `PATH`. An MSI/innoSetup installer is future
work, not required to run the app.

## Wireless ADB pairing

Phase 1 ships the **Manual** connect tab (`adb connect IP:PORT`).
QR scanning and pairing-code flows land in Phase 3; the dialog tabs already
exist as placeholders. Remember: the **pairing port** and the **ADB connection
port** are different — the app always tells you which one it needs.

## Development mode

```powershell
$env:ADB_MANAGER_MOCK=1; cargo run   # 3 fake devices (USB, wireless, unauthorized)
$env:RUST_LOG="adb_manager=debug"; cargo run
```

Logs: `%APPDATA%\ADBManager\logs\adb-manager.log`

## Project architecture

```
src/
  main.rs        eframe entry point
  app.rs         eframe::App shell (owns state, workers, channels)
  state.rs       AppState — mutated only via AppEvent
  events.rs      background-worker → UI events + toasts
  config.rs      %APPDATA%\ADBManager\config.json
  errors.rs      AppError
  logging.rs     tracing → rotating log file
  adb/           AdbClient / AdbCommandBuilder / parser / Device / AdbError
  device/        DeviceManager (poll thread) / discovery / saved devices
  ui/            theme / sidebar / header / status_bar / dashboard /
                 devices / settings / dialogs / placeholders
  apps|processes|logcat|apk|files|shell|tools|pairing/
                 Phase 2+ feature modules (stubs + placeholder pages)
```

Rules enforced from day one:

- every device operation takes an explicit serial (`adb -s SERIAL …`)
- no `cmd.exe /C`, no shell-string commands — `std::process::Command` + argv
- no `unwrap()`/`expect()` in production paths
- every long-running worker has a stop flag and cleans up its child process

## Troubleshooting

| Symptom | Fix |
|---|---|
| `ADB unavailable` | Settings → Detect ADB, or Browse to `adb.exe`, then Test ADB |
| Device shows `unauthorized` | Accept the USB-debugging prompt on the phone, then Refresh |
| Wireless connect refused | Use the **connection** port from Wireless Debugging, not the pairing port |
| Empty device list | `adb devices` in a terminal to confirm the driver/USB cable |

## License

MIT — see [LICENSE](LICENSE).
