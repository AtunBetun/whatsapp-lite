# WhatsApp Lite

A hardened Tauri wrapper around [web.whatsapp.com](https://web.whatsapp.com) that behaves like a first-party desktop app: native title bar, tray controls, unread badges, drag-and-drop sharing, and start-on-login support. The Rust backend embeds the compiled React bundle so every release ships as a single native binary plus platform packaging.

## Features

- **Native shell** – Standard window chrome (restored size/position, hidden/maximized memory) with badges, shortcuts (`Cmd/Ctrl+R`, `Cmd/Ctrl+Shift+R`, `Cmd/Ctrl+W`), and safe navigation that forces non-WhatsApp URLs into the system browser.
- **Tray + autostart** – Dynamic unread badge, show/hide toggle, reload/clear-cache actions, Preferences submenu with a “Start at Login” checkbox, and tray click-to-toggle.
- **Desktop integrations** – OS notifications via a proxied `Notification` API, drag & drop directly into chats, and global shortcuts wired through Tauri plugins.
- **Security posture** – CSP locked to WhatsApp, devtools disabled outside debug builds, navigation filtering, IPC scoped through Tauri capabilities, and cookies/state persisted via the built-in window-state plugin.

## Requirements

1. **Rust** (stable) via [rustup](https://rustup.rs).
2. **Node.js 18+** with **pnpm** (enable with `corepack enable pnpm` if needed).
3. Platform-specific Tauri prerequisites (WebKitGTK on Linux, Xcode CLT on macOS, Visual Studio Build Tools on Windows). Follow the [official setup guide](https://tauri.app/start/prerequisites/).

Install JS deps once:

```bash
pnpm install
```

## Development Workflow

```bash
pnpm tauri dev
```

This command spawns the Tauri shell and immediately loads the live WhatsApp Web site inside the WebView. The React code in `src/` is only a placeholder for when the WebView can’t be shown (e.g., if WhatsApp is unreachable).

## Building Release Bundles

```bash
pnpm tauri build
```

`pnpm tauri build` performs three steps automatically:

1. `pnpm build` – compiles the Vite bundle into `dist/`.
2. `cargo build --release` – compiles the Tauri backend, embedding everything under `dist/`.
3. Packages artifacts per platform inside `src-tauri/target/{triple}/release/`.

Artifacts you should see:

- **macOS**: `WhatsApp Lite.app`, `.dmg`, and a zipped app bundle.
- **Windows**: `WhatsApp Lite.exe` plus an NSIS installer (`WhatsApp Lite_*.msi`/`.exe`). Build this on Windows for the fastest path; cross-compiling requires the MSVC toolchain, WebView2 redist, and NSIS installed.
- **Linux**: Binary plus AppImage (depends on `libwebkit2gtk-4.1` at runtime).

All assets (icons, React bundle, bridge scripts) are embedded in the binary, so you only need to distribute the generated artifacts.

## Packaging Tips

- **Testing**: Use `pnpm tauri build --debug` for faster builds before shipping release bits.
- **Code signing**: Configure `tauri.conf.json` with your signing identities if you plan to distribute outside your own machines.
- **Windows sharing**: After running `pnpm tauri build` on Windows, zip the `WhatsApp Lite.exe` in `src-tauri/target/release/` or send the NSIS installer in the same directory to your recipient.

## Controls Cheat Sheet

- **Tray menu**: Show/Hide, Reload, Clear Cache & Reload, Preferences → Start at Login, Quit.
- **Tray left click**: Toggle visibility.
- **Shortcuts**: `Cmd/Ctrl+R`, `Cmd/Ctrl+Shift+R`, `Cmd/Ctrl+W`.
- **Drag & drop**: Drop files directly from Finder/Explorer into a chat—Tauri forwards paths automatically.

## Troubleshooting

- **External Safari/Browser tabs opening**: Allowed hosts now include WhatsApp’s cache-management endpoints. If you see new domains popping up, add them to `ALLOWED_WEBVIEW_HOSTS` in `src-tauri/src/lib.rs`.
- **Missing Windows binary**: Ensure you run `pnpm tauri build` on Windows (or set up a full cross-compilation environment). The `.exe` appears under `src-tauri/target/release/`.
- **Notifications blocked**: Grant OS-level notification permission the first time Tauri prompts, otherwise the proxied `Notification` API can’t surface alerts while the window is hidden.
