//! DuoSwitch 主入口。
//!
//! 带 CLI 参数时只做命令行动作并退出，不初始化界面 ——
//! 这样 `--apply-lockscreen` 可以被提权后的自身进程复用，
//! 也方便在没有界面的情况下实测切换是否真的生效。

mod commands;
mod config;
mod scheduler;
mod suncalc;
mod thumbnail;
mod win32;

use std::sync::{Arc, Mutex};

use chrono::Local;
use serde_json::json;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, WindowEvent};

use commands::AppState;
use config::Mode;
use scheduler::{RuntimeState, Shared};

pub fn run() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(code) = run_cli(&args) {
        std::process::exit(code);
    }

    let config_dir = config::config_dir();
    let mut cfg = config::load(&config_dir);
    let notes = cfg.sanitize();

    let shared = Arc::new(Mutex::new(Shared {
        cfg,
        revision: 1,
        rt: RuntimeState {
            notice: if notes.is_empty() {
                None
            } else {
                Some(notes.join("；"))
            },
            ..RuntimeState::default()
        },
    }));

    let worker = Arc::clone(&shared);

    tauri::Builder::default()
        // 单实例插件必须最先注册：否则二次启动会多出一个托盘图标和一套调度
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        // app_name 决定自启项在「任务管理器 → 启动」里显示的名字，默认会取 crate 名
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .app_name("DuoSwitch")
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::save_config,
            commands::apply_now,
            commands::set_paused,
            commands::sun_preview,
            commands::open_color_settings,
            commands::wallpaper_preview,
            commands::dismiss_conflict,
            commands::setup_lock_helper,
            commands::lock_helper_ready,
            commands::clear_lock_screen,
            commands::apply_lock_screen,
            commands::current_theme,
            commands::builtin_wallpapers,
        ])
        .setup(move |app| {
            let handle = app.handle().clone();

            // 首次启动时把内置壁纸写入配置，并跳过调度器的首次主动断言，
            // 避免装好一启动就偷偷换掉用户桌面 —— 用户点「立即应用」才生效。
            // AppHandle + shared 在这里都 ready，唯一一次检测。
            if commands::seed_builtin_wallpapers_if_first_run(
                &handle,
                &worker,
                &config_dir,
            ) {
                {
                    let mut guard = commands::lock(&worker);
                    // 把当前时段槽记为「已处理」，并让调度器认为壁纸已跟随当前系统主题，
                    // 这样第一次 tick 会判定为 Idle，不会 set_theme / set_wallpaper。
                    // 之后照常运行：下个切换点到来、或用户手动应用时才动桌面。
                    let slot = scheduler::current_transition(&guard.cfg, Local::now());
                    guard.rt.followed_wallpaper = Some(win32::current_theme());
                    if let Some(slot) = slot {
                        guard.rt.applied = Some(slot);
                        guard.rt.applied_revision = Some(guard.revision);
                    }
                }
                handle.emit("first-run-seeded", ()).ok();
            }

            let kick = {
                let notify_handle = handle.clone();
                scheduler::spawn(Arc::clone(&worker), move |snapshot| {
                    let dto = commands::state_dto(&snapshot);
                    sync_window_theme(&notify_handle, dto.theme);
                    let _ = notify_handle.emit("state", dto);
                })
            };

            app.manage(AppState {
                shared: Arc::clone(&worker),
                kick,
            });

            build_tray(&handle)?;
            sync_window_theme(&handle, win32::current_theme());
            Ok(())
        })
        .on_window_event(|window, event| {
            // 关窗口只收进托盘，调度必须继续跑
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("DuoSwitch 启动失败");
}

/// 让本窗口标题栏跟随系统明暗主题。
fn sync_window_theme(app: &AppHandle, mode: Mode) {
    let dark = matches!(mode, Mode::Dark);
    if let Some(window) = app.get_webview_window("main") {
        if let Ok(hwnd) = window.hwnd() {
            win32::set_dark_titlebar(hwnd.0, dark);
        }
    }
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "显示主窗口", true, None::<&str>)?;
    let light = MenuItem::with_id(app, "light", "立即转浅色", true, None::<&str>)?;
    let dark = MenuItem::with_id(app, "dark", "立即转深色", true, None::<&str>)?;
    let pause = MenuItem::with_id(app, "pause", "暂停调度", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &show,
            &PredefinedMenuItem::separator(app)?,
            &light,
            &dark,
            &PredefinedMenuItem::separator(app)?,
            &pause,
            &quit,
        ],
    )?;

    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("DuoSwitch")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(handle_menu);

    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }

    builder.build(app)?;
    Ok(())
}

fn handle_menu(app: &AppHandle, event: tauri::menu::MenuEvent) {
    match event.id().as_ref() {
        "show" => {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }
        "light" => manual_apply(app, Some(Mode::Light)),
        "dark" => manual_apply(app, Some(Mode::Dark)),
        "pause" => toggle_pause(app),
        "quit" => app.exit(0),
        _ => {}
    }
}

/// 托盘/界面触发的即时切换。结果记为「已跟随」，不会被误判成外部冲突。
fn manual_apply(app: &AppHandle, mode: Option<Mode>) {
    let state = app.state::<AppState>();

    let dto = {
        let mut guard = commands::lock(&state.shared);
        let target = mode.unwrap_or_else(win32::current_theme);
        let cfg = guard.cfg.clone();

        match scheduler::apply_mode(&cfg, target) {
            Ok(note) => {
                guard.rt.followed_wallpaper = Some(target);
                guard.rt.conflict = None;
                guard.rt.last_error = note;
            }
            Err(err) => guard.rt.last_error = Some(err),
        }
        commands::state_dto(&guard)
    };

    let _ = app.emit("state", dto);
    let _ = state.kick.send(());
}

fn toggle_pause(app: &AppHandle) {
    let state = app.state::<AppState>();

    let dto = {
        let mut guard = commands::lock(&state.shared);
        guard.rt.paused = !guard.rt.paused;
        commands::state_dto(&guard)
    };

    let _ = app.emit("state", dto);
    let _ = state.kick.send(());
}

// ---------------------------------------------------------------- CLI 模式

fn run_cli(args: &[String]) -> Option<i32> {
    let first = args.first()?.as_str();
    match first {
        "--dump-state" => Some(cli_dump_state()),
        "--set-mode" => Some(cli_set_mode(args.get(1).map(String::as_str))),
        "--apply-lockscreen" => Some(cli_apply_lockscreen(args.get(1).map(String::as_str))),
        "--apply-lockscreen-auto" => Some(cli_apply_lockscreen_auto()),
        "--install-lock-helper" => Some(cli_install_lock_helper()),
        "--clear-lockscreen" | "--uninstall-lock-helper" => Some(cli_uninstall_lock_helper()),
        "--help" | "-h" => {
            print_help();
            Some(0)
        }
        // 其它参数一律交给 Tauri（WebView2 会追加自己的参数）
        _ => None,
    }
}

fn cli_dump_state() -> i32 {
    let dir = config::config_dir();
    let mut cfg = config::load(&dir);
    let notes = cfg.sanitize();
    let now = Local::now();

    let fmt = |t: scheduler::Transition| {
        json!({ "at": t.at.to_rfc3339(), "mode": t.mode.as_str() })
    };

    let payload = json!({
        "elevated": win32::is_elevated(),
        "config_path": config::config_path(&dir).display().to_string(),
        "config_exists": config::config_path(&dir).is_file(),
        "theme_now": win32::current_theme().as_str(),
        "enabled": cfg.enabled,
        "lock_screen": cfg.lock_screen,
        "light_wallpaper": cfg.light_wallpaper.as_ref().map(|p| p.display().to_string()),
        "dark_wallpaper": cfg.dark_wallpaper.as_ref().map(|p| p.display().to_string()),
        "schedule": serde_json::to_value(&cfg.schedule).unwrap_or(serde_json::Value::Null),
        "current_transition": scheduler::current_transition(&cfg, now).map(fmt),
        "next_transition": scheduler::next_transition(&cfg, now).map(fmt),
        "notes": notes,
    });

    match serde_json::to_string_pretty(&payload) {
        Ok(text) => {
            println!("{text}");
            0
        }
        Err(err) => {
            eprintln!("序列化状态失败：{err}");
            1
        }
    }
}

fn cli_set_mode(arg: Option<&str>) -> i32 {
    let Some(mode) = arg.and_then(Mode::parse) else {
        eprintln!("用法：--set-mode light|dark");
        return 2;
    };

    let cfg = config::load(&config::config_dir());
    let wallpaper = cfg
        .wallpaper_for(mode)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<未配置，仅切主题>".to_string());

    match scheduler::apply_mode(&cfg, mode) {
        Ok(note) => {
            println!("applied mode={} wallpaper={wallpaper}", mode.as_str());
            if let Some(note) = note {
                println!("note: {note}");
            }
            0
        }
        Err(err) => {
            eprintln!("切换失败：{err}");
            1
        }
    }
}

fn cli_apply_lockscreen(arg: Option<&str>) -> i32 {
    let Some(mode) = arg.and_then(Mode::parse) else {
        eprintln!("用法：--apply-lockscreen light|dark");
        return 2;
    };

    let cfg = config::load(&config::config_dir());
    let Some(path) = cfg.wallpaper_for(mode) else {
        eprintln!("{}模式未配置壁纸，无法设置锁屏", mode.as_str());
        return 1;
    };

    match win32::set_lock_screen(path) {
        Ok(()) => {
            println!("lockscreen applied mode={}", mode.as_str());
            0
        }
        Err(err) => {
            eprintln!("设置锁屏失败：{err}");
            1
        }
    }
}

/// 助手任务的入口：读当前系统主题，把对应模式的壁纸写进锁屏。
/// 不接受参数 —— 任务定义里无法传动态参数，读注册表反而更不容易出错。
fn cli_apply_lockscreen_auto() -> i32 {
    let cfg = config::load(&config::config_dir());
    let mode = win32::current_theme();

    let Some(path) = cfg.wallpaper_for(mode) else {
        eprintln!("{}模式未配置壁纸，无法设置锁屏", mode.as_str());
        return 1;
    };

    match win32::set_lock_screen(path) {
        Ok(()) => {
            println!("lockscreen applied mode={}", mode.as_str());
            0
        }
        Err(err) => {
            eprintln!("设置锁屏失败：{err}");
            1
        }
    }
}

fn cli_install_lock_helper() -> i32 {
    if let Err(err) = win32::install_lock_helper() {
        eprintln!("注册锁屏助手失败：{err}");
        return 1;
    }
    println!("lock helper task installed");

    // 顺手把当前主题对应的锁屏写一次，省得等到下一个切换点
    cli_apply_lockscreen_auto()
}

fn cli_uninstall_lock_helper() -> i32 {
    win32::uninstall_lock_helper();
    match win32::clear_lock_screen() {
        Ok(()) => {
            println!("lock helper removed and lockscreen takeover cleared");
            0
        }
        Err(err) => {
            eprintln!("移除助手成功，但清除锁屏接管失败：{err}");
            1
        }
    }
}

fn print_help() {
    println!(
        "\
DuoSwitch — 按时间自动切换 Windows 明暗主题与壁纸

  --dump-state                   打印当前配置与调度状态（JSON）
  --set-mode light|dark          立即应用指定模式（主题 + 壁纸）

锁屏相关（写 HKLM，需要管理员权限）：
  --apply-lockscreen light|dark  按指定模式写一次锁屏壁纸
  --apply-lockscreen-auto        按当前系统主题写一次锁屏（助手任务用）
  --install-lock-helper          注册最高权限助手任务，之后可静默触发
  --uninstall-lock-helper        删除助手任务并撤销锁屏接管

不带参数启动图形界面。

注意：release 构建隐藏控制台，上面这些命令的输出需要 debug 构建才能看到。"
    );
}
