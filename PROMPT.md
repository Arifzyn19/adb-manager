ADB Manager — Windows Desktop Android Device Management Tool

ROLE

You are a senior Rust desktop application engineer, Windows developer, Android tooling engineer, and UI/UX designer.

Build a production-quality Windows desktop application called ADB Manager.

The application is a graphical Android device management and debugging toolkit built entirely with Rust.

The application must NOT use:

- React
- TypeScript
- JavaScript
- Electron
- Tauri
- HTML/CSS web UI
- Python
- .NET/C#
- Java/Kotlin for the desktop application

Use Rust for the application and a native Rust GUI framework.

---

1. CORE TECHNOLOGY

Use:

- Rust stable
- "eframe" + "egui" for the desktop GUI
- Tokio for asynchronous/background tasks where appropriate
- Serde for serialization
- "tracing" for application logging
- "thiserror" / "anyhow" for error handling
- "rfd" for native Windows file/folder dialogs
- "image" for image handling where necessary
- A Rust QR decoding library for QR pairing
- SQLite only if persistent structured data becomes necessary; otherwise prefer a simple JSON configuration file

The application targets:

Windows 10/11 x64

Design the architecture so ARM64 Windows could be supported later.

---

2. APPLICATION PURPOSE

ADB Manager is an all-in-one Windows GUI for Android developers and power users.

The application should allow users to:

1. Connect Android devices through USB ADB
2. Pair Android devices through Wireless Debugging
3. Pair using QR code
4. Pair using pairing code + IP + port
5. Connect using IP + ADB port
6. Manage multiple Android devices
7. View device information
8. Manage installed applications
9. Inspect running processes
10. View realtime Logcat
11. Detect application crashes
12. Analyze stack traces
13. Install APKs
14. Inspect APK metadata
15. Browse Android files
16. Push and pull files
17. Run ADB shell commands
18. Take screenshots
19. Record the Android screen
20. Reboot devices
21. Inspect battery, memory and storage
22. Manage saved devices
23. Automatically reconnect known devices

The application must feel like a serious developer tool rather than a simple GUI wrapper around a terminal.

---

3. IMPORTANT ARCHITECTURE RULE

Do NOT scatter calls to "adb.exe" throughout the UI code.

Create a centralized ADB abstraction.

Recommended architecture:

UI
  ↓
Application State
  ↓
Feature Services
  ↓
ADB Client
  ↓
ADB Command Builder
  ↓
adb.exe
  ↓
Android Device

All ADB operations must go through the ADB layer.

The UI must never directly construct arbitrary ADB commands.

---

4. PROJECT STRUCTURE

Use a clean modular structure similar to:

adb-manager/
│
├── Cargo.toml
├── Cargo.lock
├── README.md
├── LICENSE
│
├── assets/
│
└── src/
    │
    ├── main.rs
    ├── app.rs
    ├── state.rs
    ├── events.rs
    ├── config.rs
    ├── errors.rs
    │
    ├── adb/
    │   ├── mod.rs
    │   ├── client.rs
    │   ├── command.rs
    │   ├── parser.rs
    │   ├── device.rs
    │   └── errors.rs
    │
    ├── device/
    │   ├── mod.rs
    │   ├── manager.rs
    │   ├── discovery.rs
    │   ├── info.rs
    │   ├── wireless.rs
    │   └── saved.rs
    │
    ├── pairing/
    │   ├── mod.rs
    │   ├── qr.rs
    │   ├── camera.rs
    │   └── pairing.rs
    │
    ├── apps/
    │   ├── mod.rs
    │   ├── manager.rs
    │   ├── package.rs
    │   └── parser.rs
    │
    ├── processes/
    │   ├── mod.rs
    │   ├── manager.rs
    │   └── parser.rs
    │
    ├── logcat/
    │   ├── mod.rs
    │   ├── stream.rs
    │   ├── parser.rs
    │   ├── filter.rs
    │   └── crash.rs
    │
    ├── apk/
    │   ├── mod.rs
    │   ├── inspector.rs
    │   ├── manifest.rs
    │   ├── permissions.rs
    │   └── certificate.rs
    │
    ├── files/
    │   ├── mod.rs
    │   ├── manager.rs
    │   └── parser.rs
    │
    ├── shell/
    │   ├── mod.rs
    │   └── session.rs
    │
    ├── tools/
    │   ├── mod.rs
    │   ├── screenshot.rs
    │   ├── recording.rs
    │   ├── reboot.rs
    │   └── system.rs
    │
    └── ui/
        ├── mod.rs
        ├── theme.rs
        ├── sidebar.rs
        ├── header.rs
        ├── status_bar.rs
        ├── dashboard.rs
        ├── devices.rs
        ├── apps.rs
        ├── processes.rs
        ├── logcat.rs
        ├── apk.rs
        ├── files.rs
        ├── shell.rs
        ├── tools.rs
        ├── settings.rs
        └── dialogs.rs

You may adjust the structure if a better architecture is appropriate.

Keep responsibilities separated.

---

5. MAIN APPLICATION WINDOW

The main window should use a professional dark developer-tool layout.

Overall layout:

┌──────────────────────────────────────────────────────────────┐
│ ADB Manager                         [Device ▼] [Settings ⚙] │
├──────────────┬───────────────────────────────────────────────┤
│              │                                               │
│ Dashboard    │                                               │
│ Devices      │                                               │
│              │                                               │
│ MANAGEMENT   │                                               │
│ Apps         │                                               │
│ Processes    │                                               │
│ Files        │                                               │
│              │                                               │
│ DEBUG        │                                               │
│ Logcat       │                                               │
│ APK          │                                               │
│ Shell        │                                               │
│              │                                               │
│ TOOLS        │                                               │
│ Tools        │                                               │
│              │                                               │
│              │                                               │
├──────────────┴───────────────────────────────────────────────┤
│ ● Connected — vivo I2219                           ADB 1.0.41│
└──────────────────────────────────────────────────────────────┘

Sidebar:

- Dashboard
- Devices

Management:

- Apps
- Processes
- Files

Debug:

- Logcat
- APK
- Shell

Tools:

- Device Tools
- Settings

The sidebar should visually indicate the current page.

---

6. DEVICE SYSTEM

Create a central device model.

Example concept:

Device {
    serial
    state
    transport
    model
    manufacturer
    android_version
    sdk_version
    architecture
    product
    battery
    memory
    storage
    screen_resolution
    density
}

Possible states:

- Connected
- Unauthorized
- Offline
- Disconnected
- Connecting
- Pairing
- Error

Possible transports:

- USB
- Wireless

Support multiple simultaneously connected devices.

Never assume only one device exists.

---

7. ADB DISCOVERY

Periodically execute the equivalent of:

adb devices -l

Parse the result robustly.

Handle:

- normal devices
- unauthorized devices
- offline devices
- emulator devices
- wireless devices
- multiple devices

Do not use fragile string parsing when a more structured approach is possible.

The device manager should detect device changes and emit events.

Example events:

DeviceConnected
DeviceDisconnected
DeviceStateChanged
DeviceInfoUpdated

---

8. ADB PATH MANAGEMENT

The application must NOT assume "adb.exe" is already in PATH.

Create an ADB Environment page.

Display:

ADB Status       ✓ Ready
ADB Version      1.0.41
ADB Path         C:\...\adb.exe

Actions:

[ Detect ADB ]
[ Browse ]
[ Test ADB ]

Search common Windows locations where reasonable.

Allow the user to manually select "adb.exe".

Validate that the selected executable actually works.

Store the selected path in configuration.

---

9. DEVICE CONNECTION UI

Create a "Connect Device" dialog.

Tabs:

USB
Wireless
Manual

USB:

Available Devices

○ Unauthorized device
🟢 Connected device

Wireless:

[ Scan QR Code ]

OR

IP Address: [____________]
Pairing Port: [__________]
Pairing Code: [__________]

[ Pair Device ]

Manual:

IP Address: [____________]
Port: [____________]

[ Connect ]

Provide clear status messages.

---

10. WIRELESS ADB PAIRING

Support Android Wireless Debugging.

Implement:

1. Pairing using pairing code
2. Pairing using QR code
3. Connecting to the device after pairing
4. Saving known devices
5. Reconnecting known devices

Pairing flow:

User starts Wireless Debugging on Android
      ↓
User chooses "Pair device with pairing code"
      ↓
User enters IP + pairing port + code
      ↓
ADB Manager executes the appropriate ADB pairing operation
      ↓
Pair success
      ↓
Discover/connect device
      ↓
Add to known devices

Never assume the pairing port is the same as the ADB connection port.

---

11. QR PAIRING

Provide a QR pairing interface.

UI:

┌─────────────────────────────────┐
│       Wireless ADB QR          │
│                                 │
│       [ Camera Preview ]        │
│                                 │
│     Point Android QR code       │
│     inside this area.           │
│                                 │
│     Waiting for QR code...      │
└─────────────────────────────────┘

Requirements:

- Access Windows camera
- Show camera preview
- Decode QR codes locally
- Never upload camera frames
- Validate decoded data
- Display parsing errors
- Attempt pairing when valid
- Stop camera after successful pairing
- Provide manual pairing fallback

If camera permissions are unavailable, explain how to use manual pairing.

---

12. DASHBOARD

Dashboard should display information for the selected device.

Cards:

DEVICE

vivo I2219
Connected
Wireless / USB

SYSTEM

Android 16
API 36
arm64-v8a

MEMORY

3.2 GB / 8 GB

STORAGE

71 GB / 128 GB

BATTERY

78%
Charging

PERFORMANCE

CPU 12%
Temperature 34°C

Also provide quick actions:

[ Apps ]
[ Logcat ]
[ Files ]
[ Shell ]
[ Screenshot ]

Data should refresh in the background.

---

13. APP MANAGER

Display installed applications.

Tabs/filters:

All
User Apps
System Apps
Running

Search by:

- app name
- package name

Table:

Icon | Application | Package | Version | State

Example:

TikTok       com.example.tiktok       Running
Chrome       com.android.chrome       Stopped

Application detail page:

Application
Package
Version
Version Code
UID
Installation Type
State

Actions:

[ Launch ]
[ Force Stop ]
[ Clear Cache ]
[ Clear Data ]
[ Extract APK ]
[ Uninstall ]

Dangerous actions must require confirmation.

Do not automatically execute destructive actions.

---

14. APP DETAILS

Provide tabs:

Overview
Permissions
Activities
Services
Receivers
Providers

Where supported, obtain information through appropriate Android package-manager commands.

Make parsing robust across Android versions.

---

15. PROCESS MANAGER

Show running processes.

Columns:

PID
Process
Package
CPU
Memory

Provide:

- Search
- Sorting
- Auto refresh
- Manual refresh

Actions:

Force Stop
Kill Process
Copy PID
Copy Package

Do not claim CPU/RAM values are exact if the Android command provides only approximate information.

---

16. LOGCAT

This is one of the core features.

Create a realtime Logcat viewer.

Layout:

┌───────────────────────────────────────────────────────────────┐
│ LOGCAT                                                        │
├───────────────────────────────────────────────────────────────┤
│ [Search] [Level ▼] [Package ▼] [Pause] [Clear] [Export]      │
├───────────────────────────────────────────────────────────────┤
│ 20:42:31 I ActivityManager                                   │
│ Start proc com.example.app                                   │
│                                                               │
│ 20:42:32 D NetworkManager                                    │
│ Network connected                                            │
│                                                               │
│ 20:42:37 E AndroidRuntime                                    │
│ FATAL EXCEPTION: main                                        │
│                                                               │
└───────────────────────────────────────────────────────────────┘

Features:

- Realtime streaming
- Pause/resume
- Clear display
- Search
- Level filtering
- Package filtering
- Export
- Auto-scroll
- Copy selected log
- Copy all visible logs
- Maximum buffer size

Log levels:

- Verbose
- Debug
- Info
- Warning
- Error
- Fatal

Never freeze the UI when thousands of logs arrive.

Use a background task and channel/event system.

---

17. LOGCAT PERFORMANCE

Do NOT store unlimited logs in memory.

Implement a bounded ring buffer.

Default:

10,000 lines

Allow configuration.

If the buffer is full, remove the oldest entries.

The UI should virtualize large log lists where possible.

Avoid rebuilding every row on every incoming log line.

---

18. LOGCAT PACKAGE FILTER

Allow selecting a specific package.

Example:

Package:
[ com.example.app ▼ ]

The app may determine relevant process/package information using ADB.

Also allow manual package entry.

---

19. CRASH DETECTION

Monitor Logcat for common crash patterns.

Examples:

FATAL EXCEPTION
AndroidRuntime
Process ... has died
ANR
Force finishing activity

When detected, create a crash event.

UI notification:

⚠ Crash Detected

com.example.app

java.lang.NullPointerException

Thread: main

[ View Crash ]

Do not treat every error log as a crash.

Use appropriate patterns and context.

---

20. CRASH ANALYZER

Crash details page:

Application
Process
Exception
Thread
Timestamp

Display stack trace.

Actions:

[ Copy Stacktrace ]
[ Export ]
[ Filter System Frames ]

Implement "Filter System Frames" carefully.

Do not remove application frames.

Make application frames visually prominent.

---

21. APK INSPECTOR

Allow:

- Open APK
- Drag & drop APK

Display:

Package
Version
Version Code
Min SDK
Target SDK
Architecture
File Size

Tabs:

Overview
Manifest
Permissions
Activities
Services
Receivers
Providers
Files
Certificate

If external Android SDK tools are needed for certain APK inspection operations, provide configurable paths.

Do not silently download third-party tools.

---

22. APK INSTALLER

Allow:

- Single APK installation
- Reinstallation
- Optional APK selection

For split APKs, support installing multiple APK files when practical.

Show installation progress.

Handle common errors:

- INSTALL_FAILED_...
- device unauthorized
- insufficient storage
- incompatible SDK
- signature conflict

Display a human-readable explanation when possible while preserving the original ADB error.

---

23. APK EXTRACTION

From App Manager:

[ Extract APK ]

Retrieve APK from the device when permitted.

Allow selecting a Windows destination.

Show progress.

---

24. FILE MANAGER

Provide Android storage browsing.

Default root:

/sdcard/

Display folders/files.

Actions:

Upload
Download
Delete
Rename
Create Folder
Refresh

Support drag and drop from Windows into Android.

Support downloading from Android into Windows.

For destructive operations require confirmation.

Handle permission failures gracefully.

---

25. SHELL

Provide an interactive ADB shell.

UI:

┌─────────────────────────────────────────────┐
│ ADB Shell                                   │
├─────────────────────────────────────────────┤
│ vivo:/ $ getprop ro.build.version.release  │
│ 16                                          │
│                                             │
│ vivo:/ $ pm list packages                  │
│ ...                                         │
│                                             │
│ vivo:/ $ _                                  │
└─────────────────────────────────────────────┘

Features:

- command input
- command history
- clear
- copy
- save output
- auto-scroll

Do not block the main UI thread.

---

26. DEVICE TOOLS

Create a Tools page.

Categories:

SCREEN

[ Screenshot ]
[ Screen Recording ]

SYSTEM

[ Reboot ]
[ Recovery ]
[ Bootloader ]

INFO

[ Battery ]
[ Memory ]
[ Storage ]
[ Properties ]

ADB

[ Restart ADB ]
[ Clear Logcat ]

For reboot operations, require confirmation.

---

27. SCREENSHOT

Allow capturing a screenshot through ADB.

Options:

- Save directly to Windows
- Preview
- Copy/save

Use a native Windows file dialog.

---

28. SCREEN RECORDING

Provide:

[ Start Recording ]

Show:

Recording...
Duration: 00:12
[ Stop ]

After stopping, pull the recording to Windows.

Allow choosing destination.

If device limitations exist, show them clearly.

---

29. DEVICE PROPERTIES

Display useful Android properties:

- Manufacturer
- Model
- Device
- Product
- Android version
- SDK
- Build ID
- Build fingerprint
- CPU ABI
- Bootloader state where available
- Security patch level
- Kernel version where available

Use structured parsing where possible.

---

30. BATTERY

Display:

- percentage
- charging state
- health
- temperature
- voltage
- current where available
- battery technology

Use "dumpsys battery" or equivalent.

Gracefully handle unavailable fields.

---

31. STORAGE

Display:

- total
- used
- available

Provide a visual usage indicator.

Do not imply perfect accuracy if the source command provides filesystem-level estimates.

---

32. MEMORY

Display:

- total RAM
- available RAM
- used RAM where derivable

Use Android memory information.

---

33. SETTINGS

Settings sections:

ADB

ADB executable path
Auto detect
Test ADB

Devices

Auto refresh
Auto reconnect
Remember devices

Logcat

Buffer size
Auto-scroll
Pause on crash

Appearance

Theme
UI scale

Behavior

Confirm destructive actions
Start minimized

---

34. CONFIGURATION

Store configuration locally.

Suggested Windows location:

%APPDATA%\ADBManager\

Possible files:

config.json
devices.json
logs/

Never store passwords or sensitive credentials.

Wireless device information should only be stored if necessary.

Allow users to forget saved devices.

---

35. ERROR HANDLING

Errors must be user-friendly.

Create structured error types.

Example:

AdbNotFound
AdbExecutionFailed
DeviceUnauthorized
DeviceOffline
DeviceNotFound
PairingFailed
ConnectionFailed
PermissionDenied
InvalidApk
FileTransferFailed

UI example:

Unable to connect

Device is unauthorized.

Unlock your Android device and accept
the USB debugging authorization prompt.

[ Retry ]

Always preserve useful technical details behind an expandable "Details" section.

---

36. MULTI-DEVICE SUPPORT

This is mandatory.

Never hardcode a single global device.

Every operation must explicitly know its target device.

For example:

adb -s SERIAL shell ...

The currently selected device is stored in application state.

If the selected device disconnects:

Device disconnected

Provide:

[ Select another device ]

Do not crash.

---

37. BACKGROUND TASK ARCHITECTURE

Separate UI and long-running work.

Use background tasks for:

- device polling
- logcat
- process monitoring
- battery updates
- storage updates
- memory updates
- file transfers
- APK inspection
- screenshot pulling
- screen recording

Communicate through channels/events.

Conceptual architecture:

UI Thread
   │
   │ Events
   ▼
Application State
   │
   ├── Device Worker
   ├── Logcat Worker
   ├── Process Worker
   ├── File Worker
   └── APK Worker

No blocking subprocess execution on the UI thread.

---

38. ADB PROCESS MANAGEMENT

Create a reusable process runner.

Requirements:

- capture stdout
- capture stderr
- exit code
- cancellation
- timeout
- streaming output
- child process cleanup

Avoid shell injection.

Do NOT construct commands through:

cmd.exe /C "..."

when direct process invocation can be used.

Use "std::process::Command" or Tokio process APIs with explicit arguments.

---

39. SECURITY

Security requirements:

- Never execute user input through a Windows shell unnecessarily.
- Never concatenate arbitrary user strings into shell commands.
- Validate paths.
- Prevent path traversal where applicable.
- Do not upload device data.
- Do not collect telemetry by default.
- Do not silently install software.
- Do not execute destructive operations without confirmation.
- Clearly distinguish read-only and destructive operations.

---

40. UI DESIGN

Visual direction:

Modern dark developer tool.

Do not make it look like a generic business dashboard.

Characteristics:

- dark background
- subtle panels
- compact spacing
- clear hierarchy
- professional typography
- minimal animations
- strong status indicators
- keyboard-friendly
- resizable panels
- context menus
- tooltips
- responsive layout

Use monospace font for:

- Logcat
- Shell
- Stack traces
- Technical values

Use normal UI font for navigation and controls.

Avoid excessive rounded cards.

Avoid giant empty spaces.

---

41. COLOR SYSTEM

Use semantic colors rather than decorative colors.

Examples:

Green:

- connected
- success

Yellow:

- warning
- unauthorized

Red:

- error
- destructive action

Blue/accent:

- active controls
- information

Do not rely solely on color to communicate status.

Use icons/text too.

---

42. GLOBAL HEADER

Header should contain:

Left:

ADB Manager

Center/right:

Current device selector

Example:

🟢 vivo I2219 ▼

Clicking opens:

🟢 vivo I2219
🟢 Pixel 8
⚪ Samsung A54

+ Connect Device

Right:

Settings

---

43. GLOBAL STATUS BAR

Bottom status bar:

● Connected
Device: vivo I2219
Transport: USB
ADB: 1.0.41

When no device:

○ No device connected

When ADB is unavailable:

⚠ ADB unavailable

---

44. NOTIFICATIONS

Implement a small notification/toast system.

Examples:

✓ Device connected
✓ APK installed
✓ File transferred
⚠ Device unauthorized
✕ Installation failed

Notifications should disappear automatically unless they require action.

---

45. CONFIRMATION DIALOGS

Dangerous actions:

- uninstall
- clear data
- delete file
- kill process
- reboot

Example:

Confirm Uninstall

Are you sure you want to uninstall:

com.example.app

This action cannot be undone.

[ Cancel ] [ Uninstall ]

---

46. KEYBOARD SHORTCUTS

Implement useful shortcuts where practical.

Examples:

Ctrl+K
    Global search

Ctrl+Shift+L
    Logcat

Ctrl+Shift+A
    Apps

Ctrl+Shift+S
    Shell

Ctrl+R
    Refresh

Ctrl+Shift+C
    Copy selected technical information

Do not conflict with normal text editing behavior.

---

47. GLOBAL SEARCH

Eventually implement a command/search palette.

Example:

Ctrl+K

Search:

Logcat
Apps
Connect Device
Screenshot
Shell
Settings

Also allow actions:

Connect Device
Start Logcat
Take Screenshot

---

48. ACCESSIBILITY

Provide:

- keyboard navigation
- visible focus states
- tooltips
- readable text
- sufficient contrast
- non-color status indicators

---

49. PERFORMANCE REQUIREMENTS

The application must remain responsive when:

- 10,000+ log lines arrive
- multiple devices are connected
- process monitoring is active
- large APKs are inspected
- large files are transferred

Avoid unnecessary allocations.

Use streaming where possible.

Do not clone large buffers unnecessarily.

---

50. TESTING

Create unit tests for:

ADB output parsing

- "adb devices -l"
- package lists
- process output
- battery output
- memory output
- storage output
- logcat lines

Wireless pairing parsing.

APK metadata parsing.

Crash detection.

Configuration serialization.

Also create integration tests where practical.

---

51. MOCK ADB MODE

Create a development/mock mode.

When enabled, the application can simulate:

- devices
- logcat
- apps
- processes
- battery
- storage

This allows UI development without a physical Android device.

Example:

ADB_MANAGER_MOCK=1

Do not mix mock data with real device state accidentally.

---

52. LOGGING

Use "tracing".

Application logs should go to an application log file.

Never spam stdout in production.

Provide a setting or diagnostics page to open the log directory.

Do not log:

- sensitive data
- unnecessary device file contents
- credentials

---

53. APPLICATION STARTUP

Startup sequence:

Start application
    ↓
Load configuration
    ↓
Locate ADB
    ↓
Validate ADB
    ↓
Start device discovery
    ↓
Load saved devices
    ↓
Build UI
    ↓
Show dashboard/device state

Startup must not freeze while ADB detection occurs.

---

54. FIRST-RUN EXPERIENCE

If ADB is not found:

Show:

Welcome to ADB Manager

ADB / Android Platform Tools were not found.

Select your adb.exe to continue.

[ Browse for adb.exe ]

[ Continue without device ]

Do not force the user to configure ADB if they only want to explore the UI.

---

55. EMPTY STATES

Every page needs a useful empty state.

No device:

No Android device connected.

Connect a device using USB or Wireless ADB.

[ Connect Device ]

No apps:

No applications found.

No logs:

Waiting for Logcat...

No files:

This directory is empty.

---

56. OFFLINE / DISCONNECTED HANDLING

If device disconnects during:

- APK installation
- file transfer
- logcat
- shell
- screenshot

The app must:

1. Detect disconnect
2. Cancel/terminate related worker
3. Clean resources
4. Show an error
5. Keep the application alive
6. Allow retry after reconnection

Never panic because an ADB subprocess disappeared.

---

57. RESOURCE CLEANUP

Every long-running operation must have a clear shutdown path.

Especially:

- logcat process
- shell process
- screen recording
- camera capture
- file transfer
- process monitoring

When a device is removed, clean device-specific workers.

When the application exits, terminate child processes cleanly.

---

58. README

Create a comprehensive README covering:

- What ADB Manager is
- Features
- Requirements
- Rust version
- Building
- Running
- ADB installation
- Wireless ADB pairing
- QR pairing
- Development mode
- Packaging
- Troubleshooting
- Project architecture

---

59. BUILD REQUIREMENTS

The application must build with:

cargo build --release

and run with:

cargo run --release

For Windows packaging, produce:

ADBManager.exe

Optionally provide an installer later.

Do not require Node.js.

Do not require Python.

Do not require a web runtime.

---

60. DEVELOPMENT ORDER

Do NOT attempt to implement every feature simultaneously.

Implement in phases.

Phase 1 — Foundation

Implement:

- Rust project
- egui/eframe
- application shell
- sidebar
- header
- status bar
- theme
- configuration
- ADB path detection
- ADB version detection
- device discovery

At the end of Phase 1, the application should compile and display real devices.

Phase 2 — Device Management

Implement:

- device selector
- device details
- USB support
- multiple devices
- connect/disconnect
- device refresh
- saved devices

Phase 3 — Wireless ADB

Implement:

- manual wireless connection
- pairing code
- QR scanner
- pairing state
- reconnect

Phase 4 — Apps

Implement:

- installed apps
- search
- system/user filters
- app details
- launch
- force stop
- clear cache
- clear data
- uninstall
- APK extraction

Phase 5 — Logcat

Implement:

- realtime stream
- parser
- filters
- search
- pause
- export
- ring buffer
- crash detection
- crash analyzer

Phase 6 — Processes

Implement:

- process list
- CPU
- memory
- search
- refresh
- kill/force stop

Phase 7 — APK

Implement:

- APK selection
- metadata
- manifest
- permissions
- certificate
- installation

Phase 8 — Files

Implement:

- directory browsing
- upload
- download
- delete
- rename
- folder creation

Phase 9 — Shell

Implement:

- interactive shell
- history
- output
- cancellation

Phase 10 — Tools

Implement:

- screenshot
- recording
- battery
- storage
- memory
- properties
- reboot

Phase 11 — Polish

Implement:

- shortcuts
- notifications
- animations
- improved error handling
- diagnostics
- packaging
- documentation

---

61. CODING RULES

Follow these rules strictly:

1. Write idiomatic Rust.
2. Avoid unnecessary "unwrap()".
3. Avoid "expect()" in production paths.
4. Handle all subprocess errors.
5. Use structured errors.
6. Avoid blocking the UI thread.
7. Keep UI code separate from business logic.
8. Keep ADB code separate from UI.
9. Write tests for parsers.
10. Keep functions reasonably small.
11. Prefer explicit types over excessive cleverness.
12. Comment complex logic.
13. Avoid unsafe Rust unless absolutely necessary.
14. If unsafe code is necessary, isolate and document it.
15. Never silently ignore errors.
16. Never silently execute destructive commands.
17. Never assume only one device.
18. Never assume a particular Android version.
19. Never assume Wireless Debugging is available.
20. Never assume root access.

---

62. IMPORTANT ANDROID COMPATIBILITY

Android behavior varies across versions and manufacturers.

The implementation must be defensive.

When a command is unavailable:

Display:

This feature is not available on this device.

Do not crash.

Do not assume:

- root
- unlocked bootloader
- OEM-specific commands
- fixed filesystem locations
- fixed package output format
- fixed Android version

Prefer standard ADB and Android shell commands.

---

63. UX PRINCIPLE

Every operation should clearly communicate:

What is happening?
Which device is targeted?
Is it read-only or destructive?
Is it still running?
Did it succeed?
If it failed, why?

Example:

Installing APK...

Device:
vivo I2219

File:
app-release.apk

Progress:
███████████░░░ 82%

[ Cancel ]

---

64. FINAL QUALITY BAR

The finished application should feel comparable to a professional developer utility.

It must NOT feel like:

- a prototype
- a toy
- a collection of random buttons
- a terminal wrapped in a GUI

It should have:

- coherent navigation
- consistent UI
- reliable device handling
- strong error handling
- responsive background processing
- clean architecture
- testable components
- polished empty states
- useful diagnostics

---

65. IMPLEMENTATION INSTRUCTION

Start by implementing Phase 1 only.

Before adding advanced features:

1. Create the Rust project.
2. Configure Cargo dependencies.
3. Implement the egui application shell.
4. Implement theme.
5. Implement sidebar/header/status bar.
6. Implement configuration.
7. Implement ADB executable detection.
8. Implement ADB version detection.
9. Implement "adb devices -l".
10. Parse devices into strongly typed Rust structures.
11. Implement automatic device refresh.
12. Display real connected devices.
13. Implement device selection.
14. Implement proper error handling.
15. Add unit tests for ADB device parsing.
16. Ensure "cargo check" passes.
17. Ensure "cargo test" passes.
18. Ensure "cargo build --release" passes.

Do not move to Phase 2 until Phase 1 is stable.

When implementing subsequent phases, preserve the architecture and avoid rewriting existing working modules unnecessarily.

At every phase, keep the project compilable.

If a dependency or API is uncertain, verify its current Rust API before implementing it rather than inventing APIs.

The final goal is a maintainable, production-quality pure Rust Windows ADB Manager. 