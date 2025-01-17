// https://github.com/eythaann/Seelen-UI/blob/master/src/background/tray.rs
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::{thread::sleep, time::Duration};
use tauri::utils::platform;
use tauri::image;
use tauri::{
    image::Image,
    menu::{MenuBuilder, MenuEvent, MenuItemBuilder},
    tray::{MouseButton, TrayIconBuilder, TrayIconEvent},
    App, AppHandle, Manager, Wry,
};
// platform


#[cfg(target_os = "windows")]
use tokio::time::interval;

#[cfg(target_os = "windows")]
use tauri::async_runtime;

// tray-id
const TRAY_ID: &str = "app-tray";

#[derive(Default)]
struct TrayState {
    #[cfg(target_os = "windows")]
    timer: Option<async_runtime::JoinHandle<()>>,

    is_running: bool,
}

// 定义结构体，包含鼠标位置以及 Tray 矩形
#[derive(Serialize, Deserialize, Debug, Clone)]
struct TrayIconEventInfo {
    mouse_position: (f64, f64),
    tray_rect: (i32, i32, i32, i32),
}

/// 尝试注册系统托盘图标，最多重试 10 次
pub fn try_register_tray_icon(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    const MAX_ATTEMPTS: u8 = 10;
    const RETRY_DELAY: Duration = Duration::from_millis(100);

    let mut attempts = 0;
    while let Err(e) = register_tray_icon(app) {
        if attempts >= MAX_ATTEMPTS {
            return Err(e);
        }
        attempts += 1;
        sleep(RETRY_DELAY);
    }
    Ok(())
}

fn register_tray_icon(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    // let menu = create_tray_menu(app)?;
    // let icon = app.default_window_icon().unwrap().clone();

    let handle = app.handle();

    let resource_dir = app.path().resource_dir().unwrap();

    #[cfg(target_os = "macos")]
    let icon_path = app.path().resolve(
        "resources/icon-mac.ico",
        tauri::path::BaseDirectory::Resource,
    )?;


    #[cfg(target_os = "windows")]
    let icon_path = app.path().resolve(
        "resources/icon.ico",
        tauri::path::BaseDirectory::Resource,
    )?;

    log::info!("iconPath:{:?}", icon_path);
    log::info!("resource_dir:{:?}", resource_dir);

    let image = Image::from_path(icon_path)?;

    app.manage(Mutex::new(TrayState::default()));
    let is_mac = platform::Target::current() == platform::Target::MacOS;


    TrayIconBuilder::with_id(TRAY_ID)
        .icon(image)
        .icon_as_template(is_mac)
        .menu_on_left_click(false)
        // .menu(&menu) // implementation on js side
        // .on_menu_event(create_menu_handler(handle.clone())) // implementation on js side
        .on_tray_icon_event(create_tray_handler(handle.clone()))
        .build(app)?;

    Ok(())
}

fn create_tray_menu(app: &mut App) -> Result<tauri::menu::Menu<Wry>, Box<dyn std::error::Error>> {
    let quit = MenuItemBuilder::with_id("quit", "退出").build(app)?;
    let show = MenuItemBuilder::with_id("show", "显示").build(app)?;
    let flash = MenuItemBuilder::with_id("flash", "闪烁").build(app)?;
    let flash_off = MenuItemBuilder::with_id("flash_off", "停止闪烁").build(app)?;
    MenuBuilder::new(app)
        .item(&show)
        .item(&flash)
        .item(&flash_off)
        .separator()
        .item(&quit)
        .build()
        .map_err(Into::into)
}

fn create_menu_handler(_handle: AppHandle) -> impl Fn(&AppHandle, MenuEvent) {
    move |app: &AppHandle, event: MenuEvent| match event.id().as_ref() {
        "quit" => std::process::exit(0),
        "show" => show_main_window(app.clone()),
        "flash" => {
            let _ = flash_tray_icon(app.clone(), true);
        }
        "flash_off" => {
            let _ = flash_tray_icon(app.clone(), false);
        }
        _ => (),
    }
}

fn create_tray_handler(handle: AppHandle) -> impl Fn(&tauri::tray::TrayIcon, TrayIconEvent) {
    move |_tray, event| {
        match event {
            TrayIconEvent::Click { button, .. } => {
                if button == MouseButton::Left {
                    show_main_window(handle.clone());
                }
            }
            // TrayIconEvent::Enter { id: _, position, rect } => {
            //     if get_tray_flash_state(handle.clone()) {
            //         let physical_position = rect.position.to_physical::<i32>(1.0);
            //         let physical_size = rect.size.to_physical::<i32>(1.0);
            //         let info = TrayIconEventInfo {
            //             mouse_position: (position.x, position.y),
            //             tray_rect: (
            //                 physical_position.x,
            //                 physical_position.y,
            //                 physical_size.width,
            //                 physical_size.height,
            //             ),
            //         };
            //         handle.emit("tray_mouseenter", info).unwrap();
            //     }
            // }
            // TrayIconEvent::Leave { id: _, position, rect } => {
            //     if get_tray_flash_state(handle.clone()) {
            //         let physical_position = rect.position.to_physical::<i32>(1.0);
            //         let physical_size = rect.size.to_physical::<i32>(1.0);
            //         let info = TrayIconEventInfo {
            //             mouse_position: (position.x, position.y),
            //             tray_rect: (
            //                 physical_position.x,
            //                 physical_position.y,
            //                 physical_size.width,
            //                 physical_size.height,
            //             ),
            //         };
            //         handle.emit("tray_mouseleave", info).unwrap();
            //     }
            // }
            _ => (),
        }
    }
}

fn show_main_window(app: AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    } else {
        log::warn!("Main window not found");
    }
}

#[tauri::command]
pub fn get_tray_flash_state(app: AppHandle) -> bool {
    let state = app.state::<Mutex<TrayState>>();
    let state = state.lock().unwrap();
    state.is_running
}

#[tauri::command]
#[cfg(target_os = "windows")]
pub fn flash_tray_icon(app: AppHandle, flash: bool) -> bool {
    let state = app.state::<Mutex<TrayState>>();

    let mut state = state.lock().unwrap();

    if flash == state.is_running {
        return true;
    }

    // 如果已有计时器在运行，先停止它
    if let Some(timer) = state.timer.take() {
        state.is_running = false;
        log::info!("停止定时器");
        timer.abort();
    }

    let tray_icon = app.tray_by_id(TRAY_ID).ok_or_else(|| false).unwrap();
    let app_handle = app.clone();

    if flash {
        log::info!("开始闪烁");
        state.is_running = true;
        let is_running = state.is_running;
        state.timer = Some(async_runtime::spawn(async move {
            let mut flag = true;
            let mut interval = interval(Duration::from_millis(500));
            while is_running {
                if flag {
                    if let Err(e) = tray_icon.set_icon(None) {
                        println!("设置托盘图标失败：{}", e);
                    }
                } else {
                    let icon = app_handle.default_window_icon().unwrap().clone();
                    if let Err(e) = tray_icon.set_icon(Some(icon)) {
                        println!("设置托盘图标失败：{}", e);
                    }
                }
                flag = !flag;
                interval.tick().await;
            }
        }));
    } else {
        state.is_running = false;
        let icon = app_handle.default_window_icon().unwrap().clone();
        if let Err(e) = tray_icon.set_icon(Some(icon)) {
            println!("设置托盘图标失败：{}", e);
        }
    }
    true
}

#[tauri::command]
#[cfg(not(target_os = "windows"))]
pub fn flash_tray_icon(app: AppHandle, flash: bool) -> bool {
    let state = app.state::<Mutex<TrayState>>();
    let mut state = state.lock().unwrap();
    if flash == state.is_running {
        return true;
    }

    let tray_icon = app.tray_by_id(TRAY_ID).ok_or_else(|| false).unwrap();

    if flash {
        state.is_running = true;
        log::info!("开始闪烁.. set_title(Some(\"1\")");
        let _ = tray_icon.set_title(Some(" 1"));
    } else {
        state.is_running = false;
        log::info!("停止闪烁.. set_title(Some(\"\")");
        let _ = tray_icon.set_title(Some(""));
    }

    true
}
