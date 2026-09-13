# ADB Manager — Implementation Phases

Single source of truth for build progress. Updated at the end of every phase.
Spec: `PROMPT.md` (§60 Development Order).

| Phase | Name | Status | Notes |
|---|---|---|---|
| 1 | Foundation | ✅ Done | Rust + egui shell, sidebar/header/status bar, theme, config, ADB detect/version, `adb devices -l` discovery, selection, error handling, 13 tests |
| 2 | Device Management | ✅ Done | `getprop`/`wm` device details, per-device disconnect, saved wireless devices + auto-reconnect, USB details, 24 tests |
| 3 | Wireless ADB | ✅ Done | Pairing code + state machine, QR camera scan + image-file fallback, local-only decode, 34 tests |
| 4 | Apps | ✅ Done | pm/dumpsys parsers, filtered+searchable list, 6-tab details, launch/stop/clear/uninstall/extract, confirms, 44 tests |
| 5 | Logcat | ✅ Done | Streaming worker + ring buffer, search/level/package filters, export, crash detection + analyzer, 58 tests |
| 6 | Processes | ✅ Done | Process list (`ps -A` + `top` CPU%), search, sorting, auto/manual refresh, kill/force-stop, 64 tests |
| 7 | APK | ✅ Done | Pure-Rust inspector (ZIP + binary AXML + signatures), 6-tab page, installer with split-APK support, 84 tests |
| 8 | Files | ✅ Done | Storage browser (`ls -la` + upload/download/delete/rename/mkdir), drag-and-drop upload, confirms, 99 tests |
| 9 | Shell | ✅ Done | Persistent `adb shell` session, marker-delimited blocks, history, save/copy/clear, cancellation, 104 tests |
| 10 | Tools | ✅ Done | Screenshot + recording, battery/memory/storage/properties, reboot, ADB maintenance, 115 tests |
| 11 | Polish | ✅ Done | Ctrl+K palette, Ctrl+Shift shortcuts, copy-diagnostics, packaging docs, 119 tests |
| 12 | UI redesign | ✅ Done | Centralized #0D1117 theme + component kit (buttons, badges, panels, tables, states, dialogs); all pages + chrome restyled, ADB backend untouched |

## Done in UI redesign

- `ui/theme`: exact palette (BG #0D1117, panel #161B22, border #273142,
  accent #3B82F6, semantic + log-level colors), full widget-state styling
  (hover/pressed/active/disabled), compact typography, flat chrome
- `ui/components`: primary/secondary/danger/icon buttons, status badges,
  pills, bordered/sunken panels, stat blocks, search fields, segmented
  tabs, table headers, empty/loading/error states, key/value grids,
  generic confirm modal
- Chrome: icon sidebar with active highlight + device footer, compact
  header with state-aware device selector, segmented status bar, tinted
  toasts
- Pages: dashboard (specs + live snapshots), devices (badges + saved),
  apps (stats + table + side detail panel), processes (state column +
  context menu), logcat (level-coded rows), APK (drop zone + 7 tabs),
  files (breadcrumb explorer), shell (terminal panel + distinct input),
  tools (sectioned cards), settings (sectioned + shortcuts reference),
  connect dialog (segmented tabs + scan-area frame)
- Backend untouched; green CI (fmt, clippy, 119 tests, release build)

## Done in Phase 3

## Phase 3 scope (from PROMPT.md §10–11, §60)

- [x] Pairing using pairing code (`adb pair IP:PAIR_PORT CODE`)
- [x] Explicit pairing state machine (idle → pairing → paired/failed)
- [x] QR scanner: Windows camera preview, local decode, never uploads frames
- [x] QR `WIFI:T:ADB;S:…;P:…;;` parsing → auto-fill pairing password
- [x] Camera-unavailable fallback (manual form + load-QR-image)
- [x] Stop camera after successful pairing (also on dialog close)
- [x] Manual `adb connect IP:ADB_PORT` retained; pairing port ≠ connection port called out in UI
- [x] Unit tests: QR parsing, pairing validation, pair-output detection

## Done in Phase 6

- `processes/` module: `process` (ProcessInfo/package_guess/rss+CPU display,
  SortColumn), `parser` (header-indexed `ps -A` + `top -n 1 -b` CPU%),
  `manager` (fetch with optional CPU attach, `kill -9`, mock data)
- `AdbClient::ps/top/kill` builders, per-device process cache, 5 s
  auto-refresh, search + sortable PID/Process/CPU/Memory columns
- UI: copy-PID/package helpers, Force Stop (package) / Kill (SIGKILL) with
  permission-denied guidance, "snapshots are approximate" disclaimer

## Done in Phase 7

- `apk/` module: `manifest` (binary-AXML decoder with UTF-8/UTF-16 pools +
  text-XML fallback, header-indexed attributes), `permissions` (dangerous /
  signature / normal classification, dangerous-first rows), `certificate`
  (v1 JAR summary, SHA-256 fingerprints, best-effort DN/validity DER scan,
  v2/v3 signing-block heuristic), `inspector` (ZIP listing, arch detection,
  `human_size`), `manager` (path validation, install orchestration)
- `AdbCommandBuilder::install/install_multiple` + `AdbClient::install` with
  `is_install_success` and humanized errors (already-exists→`-r` hint,
  test-only, SDK mismatch, signature conflict, storage, offline)
- UI: Open-file + drag-and-drop, Overview/Permissions/Components/Manifest/
  Files/Certificate tabs, reinstall toggle, split-APK sibling auto-detect,
  per-device install workers; Settings gains an optional `aapt2` path
  (inspection itself never needs it — no downloads, no uploads)

## Done in Phase 8

- `files/` module: `entry` (FileEntry/rollup, join/parent/normalize,
  mutable-path + child-name gates), `parser` (header-free `ls -la` rows,
  spaces/symlinks/year-timestamps, dirs-first sort), `manager` (list with
  empty-vs-missing distinction, mkdir/rm/mv guards, multi-file upload,
  file-or-folder download, mock data)
- `AdbCommandBuilder::ls_long/mkdir_p/rm_rf/mv_path` + `AdbClient::push/
  ls_raw/mkdir/remove/rename` with `humanize_file_error` (denied, read-only,
  missing, not-empty, no-space)
- UI: breadcrumb + Up/Home, search, double-click/Open folders, inline
  rename, two-step delete modal (honors `confirm_destructive`), new-folder
  row, file-picker + drag-and-drop upload, folder-picker download, per-device
  listing with stale-result dropping; Settings gains a browser-root field

## Done in Phase 9

- `shell/` module: `session` (persistent `adb shell` child via
  `AdbClient::spawn_interactive`, `__ADBMGR_END_<id>:<code>` marker protocol,
  `BlockAssembler`, one in-flight command, kill-to-cancel, mock responder)
- `AdbCommandBuilder::interactive_shell` + `AdbClient::spawn_interactive`
  (stdin + streaming stdout + killer; the ONLY raw spawn outside `run()`)
- UI: terminal transcript with exit codes, Send/Stop, Up/Down history,
  Clear / Copy all / Save transcript, auto-scroll, shell-user warning

## Done in Phase 10

- `tools/` module: `system` (dumpsys-battery with status/health codes,
  meminfo with MemFree fallback, `df` human + 1K-block forms, PNG check,
  safe recording paths, `RebootMode`), `manager` (fetchers, validated
  screenshot, blocking record + SIGINT stop + pull + cleanup, reboot,
  server restart, clear-logcat, mock data)
- `AdbClient::screencap/screenrecord/interrupt_screenrecord/reboot/
  restart_server/clear_logcat` + matching builders
- UI: screenshot capture + preview + Save-as, recording with duration
  presets + live timer + Stop + auto-pull, always-confirm reboots, battery /
  memory / storage bars, searchable properties, ADB maintenance

## Done in Phase 11

- Ctrl+K command palette (AND-word filter over pages + Connect + Refresh,
  keyboard-first with cursor + Enter/Esc)
- Ctrl+Shift+L/A/S page shortcuts (Ctrl+R kept; text-editing keys untouched)
- Settings → Copy diagnostics (versions, ADB, devices, paths — safe to paste)
- README: shortcuts + Windows packaging notes

## Done in Phase 3

- `pairing/` module: `qr` (WIFI:T:ADB parse with escapes, rqrr local decode,
  file + frame paths), `pairing` (request validation, `PairingState`),
  `camera` (nokhwa preview, tries indices 0–2, Drop releases hardware)
- `AdbClient::pair` with "Successfully paired" detection + `PairingFailed` errors
- Wireless tab: live preview texture, throttled decode, auto-fill code,
  pairing form, paired → Manual-tab handoff; `adb pair` on worker thread
- Deps: `nokhwa` (input-native + camera-sync-impl), `rqrr`, `image`;
  dev-dep `qrcode` for a generate→decode roundtrip test

## Done in Phase 4

- `apps/` module: `package` (AppInfo/PermissionStatus/AppFilter), `parser`
  (`pm list`, `dumpsys package` version block + install/runtime permissions +
  activity/service/receiver/provider resolver tables, `pm path`, `ps -A`),
  `manager` (two-pass fetch, launch/force-stop/clear/uninstall/pull)
- `AdbClient::uninstall/pull`, `humanize_install_error` (INSTALL_FAILED_*,
  DELETE_FAILED_*), `FileTransferFailed` error variant
- UI: filter tabs (All/User/System/Running), search, sortable label list with
  state badges, 6-tab details (Overview/Permissions/Activities/Services/
  Receivers/Providers), confirm dialogs for Clear Data + Uninstall, system-app
  uninstall guard, split-APK extraction to a chosen folder
- Per-device app cache, background resolve with progress, refresh markers

## Done in Phase 5

- `logcat/` module: `parser` (threadtime lines, V/D/I/W/E/F + Unknown),
  `filter` (search, min level, package via pid→package learning from
  `Start proc`), `crash` (FATAL EXCEPTION blocks with package/exception/stack,
  ANR / has-died / force-finish one-liners, system-frame classification),
  `stream` (batched worker, killable child, bounded `LogBuffer`, mock stream)
- `AdbClient::spawn_streaming` + `ChildKiller` (exactly-once kill, §57 safe)
- One stream per selected device, background crash monitoring, pause-on-crash
  setting, per-device buffers/filters/crashes, Settings Logcat section
- UI: toolbar (search/level/package-discovered+manual/pause/clear/export),
  capped virtualized list with stick-to-bottom, crash banner + analyzer
  (copy/export/filter-system-frames, app frames emphasized)

## Done in Phase 1

- Project scaffold, `eframe` 0.32 shell, dark theme, navigation, toasts
- Central ADB layer (`AdbClient` + `AdbCommandBuilder`), no shell strings
- ADB auto-detect / browse / validate, version display, first-run onboarding
- Background `adb devices -l` polling with connect/disconnect/state events
- `config.json`, file logging, `ADB_MANAGER_MOCK=1`, README

## Done in Phase 2

- `AdbClient::shell_text/connect/disconnect` + device-state error mapping
- `getprop` / `wm size` / `wm density` builders, parsers and `DeviceInfo` cache
- Dashboard SYSTEM card + Devices → Details grid, per-device refresh/retry
- Wireless disconnect, saved `devices.json` (transport/nickname/last-seen)
- Startup auto-reconnect behind Settings toggles, Reconnect/Forget per entry
