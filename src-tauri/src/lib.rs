use serde::Deserialize;
use std::{
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc,
    },
};
use tauri::{
    menu::{CheckMenuItem, MenuBuilder, MenuEvent, MenuItem, SubmenuBuilder},
    tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent},
    utils::config::WebviewUrl,
    webview::{NewWindowResponse, PageLoadEvent},
    AppHandle, Event, Listener, Manager, Url, WebviewWindowBuilder, Wry,
};
#[cfg(target_os = "macos")]
use tauri::TitleBarStyle;
use tauri_plugin_autostart::{MacosLauncher, ManagerExt as AutostartManagerExt};
use tauri_plugin_global_shortcut::GlobalShortcutExt;
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_opener::OpenerExt;
use tauri_plugin_window_state::StateFlags;

const WHATSAPP_URL: &str = "https://web.whatsapp.com/";
const UNREAD_EVENT: &str = "whatsapp-lite://unread-count";
const NOTIFICATION_EVENT: &str = "whatsapp-lite://notification";
const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/123.0.0.0 Safari/537.36";
const ALLOWED_WEBVIEW_HOSTS: &[&str] = &["web.whatsapp.com", "flows.whatsapp.net"];

const MENU_TOGGLE: &str = "tray-toggle";
const MENU_RELOAD: &str = "tray-reload";
const MENU_HARD_RELOAD: &str = "tray-hard-reload";
const MENU_QUIT: &str = "tray-quit";
const MENU_AUTOSTART: &str = "tray-autostart";

static BRIDGE_SCRIPT: &str = include_str!("scripts/whatsapp_bridge.js");

#[derive(Debug, Deserialize)]
struct UnreadPayload {
    count: u32,
}

#[derive(Debug, Deserialize, Default)]
struct NotificationOptions {
    #[serde(default)]
    body: Option<String>,
}

#[derive(Debug, Deserialize)]
struct NotificationPayload {
    title: String,
    #[serde(default)]
    options: NotificationOptions,
}

#[derive(Clone)]
struct TrayHandle(Arc<TrayState>);

struct TrayState {
    icon: TrayIcon<Wry>,
    toggle_item: MenuItem<Wry>,
    autostart_item: CheckMenuItem<Wry>,
    unread: AtomicU32,
    hidden: AtomicBool,
}

impl TrayHandle {
    fn new(
        icon: TrayIcon<Wry>,
        toggle_item: MenuItem<Wry>,
        autostart_item: CheckMenuItem<Wry>,
        hidden: bool,
    ) -> Self {
        let handle = Self(Arc::new(TrayState {
            icon,
            toggle_item,
            autostart_item,
            unread: AtomicU32::new(0),
            hidden: AtomicBool::new(hidden),
        }));
        handle.set_hidden(hidden);
        handle
    }

    fn update_unread(&self, app: &AppHandle<Wry>, count: u32) {
        self.0.unread.store(count, Ordering::Relaxed);
        let image = build_tray_icon(count);
        let _ = self.0.icon.set_icon(Some(image));

        let tooltip = if count == 0 {
            "WhatsApp Lite".to_string()
        } else if count > 999 {
            "999+ unread messages".to_string()
        } else if count == 1 {
            "1 unread message".to_string()
        } else {
            format!("{count} unread messages")
        };
        let _ = self.0.icon.set_tooltip(Some(tooltip.as_str()));

        let badge_label = badge_text(count);
        let _ = self.0.icon.set_title(badge_label.as_deref());

        if let Some(window) = app.get_webview_window("main") {
            let badge_count = if count == 0 {
                None
            } else {
                Some(count.min(99) as i64)
            };
            let _ = window.set_badge_count(badge_count);
            #[cfg(target_os = "macos")]
            {
                let _ = window.set_badge_label(badge_label);
            }
        }
    }

    fn set_hidden(&self, hidden: bool) {
        self.0.hidden.store(hidden, Ordering::Relaxed);
        let label = if hidden {
            "Show WhatsApp Lite"
        } else {
            "Hide WhatsApp Lite"
        };
        let _ = self.0.toggle_item.set_text(label);
    }

    fn hidden(&self) -> bool {
        self.0.hidden.load(Ordering::Relaxed)
    }

    fn set_autostart_checked(&self, enabled: bool) {
        let _ = self.0.autostart_item.set_checked(enabled);
    }

    fn autostart_item(&self) -> CheckMenuItem<Wry> {
        self.0.autostart_item.clone()
    }

    fn icon(&self) -> TrayIcon<Wry> {
        self.0.icon.clone()
    }
}

pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_opener::Builder::new()
                .open_js_links_on_click(false)
                .build(),
        )
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_notification::init())
        .plugin(
            tauri_plugin_window_state::Builder::new()
                .with_state_flags(
                    StateFlags::SIZE
                        | StateFlags::POSITION
                        | StateFlags::MAXIMIZED
                        | StateFlags::VISIBLE
                        | StateFlags::FULLSCREEN,
                )
                .build(),
        )
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .on_page_load(|webview, payload| {
            if payload.event() == PageLoadEvent::Finished && webview.label() == "main" {
                let _ = webview.eval(BRIDGE_SCRIPT);
            }
        })
        .setup(|app| {
            setup_application(app)?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn setup_application(app: &mut tauri::App<Wry>) -> tauri::Result<()> {
    let autostart_enabled = app.autolaunch().is_enabled().unwrap_or(false);

    let window = build_main_window(app)?;
    let initially_hidden = !window.is_visible().unwrap_or(true);
    let tray = build_tray(app, autostart_enabled, initially_hidden)?;

    register_tray_menu_events(tray.clone());
    register_tray_click_handler(app, tray.clone());

    tray.set_autostart_checked(autostart_enabled);

    listen_unread_counts(app, tray.clone());
    listen_notifications(app);
    register_shortcuts(app, tray);

    // Request permission so notifications can fire even if the UI never asks.
    let _ = app.notification().request_permission();

    Ok(())
}

fn build_main_window(app: &mut tauri::App<Wry>) -> tauri::Result<tauri::WebviewWindow<Wry>> {
    let handle = app.handle().clone();
    let navigation_handle = handle.clone();
    let mut builder = WebviewWindowBuilder::new(
        app,
        "main",
        WebviewUrl::External(WHATSAPP_URL.parse().expect("valid WhatsApp URL")),
    )
    .title("WhatsApp Lite")
    .inner_size(1200.0, 800.0)
    .min_inner_size(360.0, 540.0)
    .resizable(true)
    .decorations(true)
    .visible(true)
    .user_agent(USER_AGENT)
    .devtools(cfg!(debug_assertions))
    .on_navigation(move |url| {
        if allow_whatsapp_url(url) {
            true
        } else {
            let _ = navigation_handle
                .opener()
                .open_url(url.to_string(), None::<&str>);
            false
        }
    })
    .on_new_window(move |url: Url, _features| {
        let _ = handle.opener().open_url(url.to_string(), None::<&str>);
        NewWindowResponse::Deny
    });

    #[cfg(target_os = "macos")]
    {
        builder = builder.title_bar_style(TitleBarStyle::Visible);
    }

    builder.build()
}

fn build_tray(
    app: &mut tauri::App<Wry>,
    autostart_enabled: bool,
    hidden: bool,
) -> tauri::Result<TrayHandle> {
    let toggle_item =
        MenuItem::with_id(app, MENU_TOGGLE, "Hide WhatsApp Lite", true, None::<&str>)?;
    let autostart_item = CheckMenuItem::with_id(
        app,
        MENU_AUTOSTART,
        "Start at Login",
        true,
        autostart_enabled,
        None::<&str>,
    )?;

    let preferences_menu = SubmenuBuilder::new(app, "Preferences")
        .item(&autostart_item)
        .build()?;

    let menu = MenuBuilder::new(app)
        .item(&toggle_item)
        .separator()
        .text(MENU_RELOAD, "Reload")
        .text(MENU_HARD_RELOAD, "Clear Cache & Reload")
        .separator()
        .item(&preferences_menu)
        .separator()
        .text(MENU_QUIT, "Quit WhatsApp Lite")
        .build()?;

    let tray_icon = TrayIconBuilder::new()
        .menu(&menu)
        .icon(build_tray_icon(0))
        .tooltip("WhatsApp Lite")
        .show_menu_on_left_click(false)
        .build(app)?;

    Ok(TrayHandle::new(
        tray_icon,
        toggle_item,
        autostart_item,
        hidden,
    ))
}

fn register_tray_menu_events(tray: TrayHandle) {
    let icon = tray.icon();
    icon.on_menu_event(move |app_handle: &AppHandle<Wry>, event: MenuEvent| {
        match event.id().as_ref() {
            MENU_TOGGLE => toggle_main_window(app_handle, &tray, None),
            MENU_RELOAD => {
                toggle_main_window(app_handle, &tray, Some(true));
                let _ = reload_webview(app_handle);
            }
            MENU_HARD_RELOAD => {
                toggle_main_window(app_handle, &tray, Some(true));
                let _ = hard_reload_webview(app_handle);
            }
            MENU_QUIT => app_handle.exit(0),
            MENU_AUTOSTART => {
                let checked = tray.autostart_item().is_checked().unwrap_or(false);
                tray.set_autostart_checked(checked);
                if checked {
                    if let Err(err) = app_handle.autolaunch().enable() {
                        eprintln!("Failed to enable autostart: {err}");
                    }
                } else if let Err(err) = app_handle.autolaunch().disable() {
                    eprintln!("Failed to disable autostart: {err}");
                }
            }
            _ => {}
        }
    });
}

fn register_tray_click_handler(app: &tauri::App<Wry>, tray: TrayHandle) {
    let icon = tray.icon();
    let app_handle = app.handle().clone();
    icon.on_tray_icon_event(move |_icon, event| {
        if let TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        } = event
        {
            toggle_main_window(&app_handle, &tray, None);
        }
    });
}

fn listen_unread_counts(app: &tauri::App<Wry>, tray: TrayHandle) {
    let tray_state = tray.clone();
    let app_handle = app.handle().clone();
    app.listen_any(UNREAD_EVENT, move |event: Event| {
        if let Ok(data) = serde_json::from_str::<UnreadPayload>(event.payload()) {
            tray_state.update_unread(&app_handle, data.count);
        }
    });
}

fn listen_notifications(app: &tauri::App<Wry>) {
    let app_handle = app.handle().clone();
    app.listen_any(NOTIFICATION_EVENT, move |event: Event| {
        if let Ok(data) = serde_json::from_str::<NotificationPayload>(event.payload()) {
            let mut builder = app_handle.notification().builder();
            builder = builder.title(data.title);
            if let Some(body) = data.options.body {
                builder = builder.body(body);
            }
            let _ = builder.show();
        }
    });
}

fn register_shortcuts(app: &tauri::App<Wry>, tray: TrayHandle) {
    let reload_tray = tray.clone();
    if let Err(err) = app.global_shortcut().on_shortcut(
        "CmdOrCtrl+R",
        move |app_handle, _, _| {
            toggle_main_window(app_handle, &reload_tray, Some(true));
            let _ = reload_webview(app_handle);
        },
    )
    {
        eprintln!("Failed to register CmdOrCtrl+R shortcut: {err}");
    }

    let hard_tray = tray.clone();
    if let Err(err) = app.global_shortcut().on_shortcut(
        "CmdOrCtrl+Shift+R",
        move |app_handle, _, _| {
            toggle_main_window(app_handle, &hard_tray, Some(true));
            let _ = hard_reload_webview(app_handle);
        },
    )
    {
        eprintln!("Failed to register CmdOrCtrl+Shift+R shortcut: {err}");
    }

    if let Err(err) = app.global_shortcut().on_shortcut(
        "CmdOrCtrl+W",
        move |app_handle, _, _| {
            toggle_main_window(app_handle, &tray, Some(false));
        },
    )
    {
        eprintln!("Failed to register CmdOrCtrl+W shortcut: {err}");
    }
}

fn toggle_main_window(app: &AppHandle<Wry>, tray: &TrayHandle, desired_visible: Option<bool>) {
    if let Some(window) = app.get_webview_window("main") {
        let should_show = desired_visible.unwrap_or(tray.hidden());
        if should_show {
            let _ = window.show();
            let _ = window.set_focus();
            tray.set_hidden(false);
        } else {
            let _ = window.hide();
            tray.set_hidden(true);
        }
    }
}

fn reload_webview(app: &AppHandle<Wry>) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window("main") {
        window.eval("window.location.reload();")?;
    }
    Ok(())
}

fn hard_reload_webview(app: &AppHandle<Wry>) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window("main") {
        window.clear_all_browsing_data()?;
        window.eval("window.location.reload();")?;
    }
    Ok(())
}

fn allow_whatsapp_url(url: &Url) -> bool {
    match url.scheme() {
        "tauri" | "about" => true,
        "https" => url
            .host_str()
            .map(|host| {
                ALLOWED_WEBVIEW_HOSTS
                    .iter()
                    .any(|allowed| host.eq_ignore_ascii_case(allowed))
            })
            .unwrap_or(false),
        _ => false,
    }
}

fn badge_text(count: u32) -> Option<String> {
    if count == 0 {
        None
    } else if count > 99 {
        Some("99+".to_string())
    } else {
        Some(count.to_string())
    }
}

fn build_tray_icon(count: u32) -> tauri::image::Image<'static> {
    const SIZE: u32 = 96;
    let mut data = vec![0u8; (SIZE * SIZE * 4) as usize];
    fill_rect(&mut data, SIZE, [0x0b, 0x14, 0x1a, 0xff]);
    draw_circle(
        &mut data,
        SIZE,
        SIZE as f32 / 2.0,
        SIZE as f32 / 2.0,
        SIZE as f32 * 0.42,
        [0x19, 0x28, 0x30, 0xff],
    );
    draw_circle(
        &mut data,
        SIZE,
        SIZE as f32 / 2.0,
        SIZE as f32 / 2.0,
        SIZE as f32 * 0.35,
        [0x25, 0xd3, 0x66, 0xff],
    );
    draw_phone(&mut data, SIZE);
    if count > 0 {
        draw_badge(&mut data, SIZE, count);
    }
    tauri::image::Image::new_owned(data, SIZE, SIZE)
}

fn fill_rect(data: &mut [u8], size: u32, color: [u8; 4]) {
    for y in 0..size {
        for x in 0..size {
            set_pixel(data, size, x as i32, y as i32, color);
        }
    }
}

fn draw_circle(data: &mut [u8], size: u32, cx: f32, cy: f32, radius: f32, color: [u8; 4]) {
    let radius_sq = radius * radius;
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            if dx * dx + dy * dy <= radius_sq {
                set_pixel(data, size, x as i32, y as i32, color);
            }
        }
    }
}

fn draw_phone(data: &mut [u8], size: u32) {
    let width = (size as f32 * 0.22).round() as i32;
    let height = (size as f32 * 0.36).round() as i32;
    let x0 = (size as i32 - width) / 2;
    let y0 = (size as i32 - height) / 2;

    for y in 0..height {
        for x in 0..width {
            let px = x0 + x;
            let py = y0 + y;
            if x == 0 || x == width - 1 || y == 0 || y == height - 1 {
                set_pixel(data, size, px, py, [0x0b, 0x14, 0x1a, 0xff]);
            } else {
                set_pixel(data, size, px, py, [0xff, 0xff, 0xff, 0xff]);
            }
        }
    }

    let speaker_y = y0 + (height as f32 * 0.15) as i32;
    for x in x0 + (width / 4)..=x0 + (3 * width / 4) {
        set_pixel(data, size, x, speaker_y, [0x0b, 0x14, 0x1a, 0xff]);
    }

    let button_y = y0 + height - (height / 6);
    let button_radius = width.min(height) as f32 * 0.08;
    draw_circle(
        data,
        size,
        x0 as f32 + (width as f32 / 2.0),
        button_y as f32,
        button_radius,
        [0x0b, 0x14, 0x1a, 0xff],
    );
}

fn draw_badge(data: &mut [u8], size: u32, count: u32) {
    let label = badge_text(count).unwrap_or_else(|| "1".to_string());
    let cx = size as f32 * 0.72;
    let cy = size as f32 * 0.22;
    let radius = size as f32 * 0.23;

    draw_circle(data, size, cx, cy, radius, [0xdf, 0x3a, 0x32, 0xff]);

    let scale = 2;
    let glyph_width = 5 * scale;
    let glyph_height = 7 * scale;
    let spacing = scale;

    let text_width =
        label.chars().count() as i32 * glyph_width as i32 + ((label.len().saturating_sub(1)) as i32 * spacing);
    let start_x = (cx as i32) - text_width / 2;
    let start_y = (cy as i32) - (glyph_height as i32 / 2);
    for (index, ch) in label.chars().enumerate() {
        if let Some(glyph) = glyph_bits(ch) {
            draw_glyph(
                data,
                size,
                start_x + index as i32 * (glyph_width as i32 + spacing),
                start_y,
                glyph,
                scale as i32,
                [0xff, 0xff, 0xff, 0xff],
            );
        }
    }
}

fn glyph_bits(ch: char) -> Option<&'static [u8; 7]> {
    match ch {
        '0' => Some(&[0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110]),
        '1' => Some(&[0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110]),
        '2' => Some(&[0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111]),
        '3' => Some(&[0b11110, 0b00001, 0b00001, 0b00110, 0b00001, 0b00001, 0b11110]),
        '4' => Some(&[0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010]),
        '5' => Some(&[0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110]),
        '6' => Some(&[0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110]),
        '7' => Some(&[0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000]),
        '8' => Some(&[0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110]),
        '9' => Some(&[0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100]),
        '+' => Some(&[0b00100, 0b00100, 0b11111, 0b00100, 0b00100, 0b00000, 0b00000]),
        _ => None,
    }
}

fn draw_glyph(
    data: &mut [u8],
    size: u32,
    start_x: i32,
    start_y: i32,
    glyph: &[u8; 7],
    scale: i32,
    color: [u8; 4],
) {
    for (row, bits) in glyph.iter().enumerate() {
        for col in 0..5 {
            if bits & (1 << (4 - col)) != 0 {
                for sy in 0..scale {
                    for sx in 0..scale {
                        let px = start_x + col as i32 * scale + sx;
                        let py = start_y + row as i32 * scale + sy;
                        set_pixel(data, size, px, py, color);
                    }
                }
            }
        }
    }
}

fn set_pixel(data: &mut [u8], size: u32, x: i32, y: i32, color: [u8; 4]) {
    if x < 0 || y < 0 || x >= size as i32 || y >= size as i32 {
        return;
    }
    let idx = ((y as u32 * size + x as u32) * 4) as usize;
    data[idx..idx + 4].copy_from_slice(&color);
}
