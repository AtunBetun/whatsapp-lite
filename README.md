# WhatsApp Lite

A minimal Tauri desktop shell that wraps https://web.whatsapp.com with the comforts you expect from a native app: standard window chrome, tray controls, login persistence, global shortcuts, badges, and start-on-login support.

## Highlights

- Loads the official WhatsApp Web experience directly inside a hardened Tauri WebView (no UI reimplementation).
- Native window chrome with persistent cookies, window size/position restore, and hidden/maximized state memory.
- Tray icon with dynamic unread badge, quick reload/clear-cache actions, show/hide toggle, and a Preferences submenu with a start-on-login checkbox.
- Global shortcuts: `Cmd/Ctrl+R` reloads, `Cmd/Ctrl+Shift+R` clears cache + reloads, `Cmd/Ctrl+W` hides the window.
- Blocks navigation to other domains (opens them in the system browser instead) and proxies Notification API calls through the native OS even when the window is hidden.
- Enforced CSP, drag & drop disabled, devtools disabled in production, and all IPC access limited through capabilities.

## Prerequisites

- **Rust** (stable toolchain via [rustup](https://rustup.rs)).
- **Node.js 18+** with **pnpm** (Corepack can enable pnpm via `corepack enable pnpm`).
- Platform-specific requirements for Tauri (e.g., WebKitGTK on Linux, Xcode CLT on macOS). See the [Tauri setup guide](https://tauri.app/start/prerequisites/) for detailed instructions.

Install JS dependencies once:

```bash
pnpm install
```

## Running in Development

```bash
pnpm tauri dev
```

This launches the Tauri shell and immediately points the single WebView at `https://web.whatsapp.com`. No local Vite dev server is required; the React bundle only provides a fallback placeholder.

## Building Release Bundles

```bash
pnpm tauri build
```

Output installers/binaries are written to `src-tauri/target/{platform}/release` with platform-appropriate icons (`.icns`, `.ico`, and PNG assets are pre-generated).

## Platform Notes

- **macOS**: Grant notification permissions the first time you are prompted so WhatsApp can continue to alert you when hidden. Start-at-login uses a LaunchAgent entry.
- **Windows**: Start-at-login writes to the Run registry key. Notifications use WinRT to surface to Action Center.
- **Linux**: Requires `libwebkit2gtk-4.1` + tray support in your desktop environment. Notifications use `notify-rust`.

## Available Controls

- **Tray menu**: Show/Hide window, Reload, Clear Cache & Reload, Preferences → “Start at Login”, Quit.
- **Tray click**: Left-click toggles visibility.
- **Shortcuts**: `Cmd/Ctrl+R`, `Cmd/Ctrl+Shift+R`, `Cmd/Ctrl+W`.
- **Native bridge**: WhatsApp’s Notification API, favicons/title badge, and external links are all forwarded through the Tauri backend for OS-native handling.

Enjoy WhatsApp Web with desktop conveniences.
