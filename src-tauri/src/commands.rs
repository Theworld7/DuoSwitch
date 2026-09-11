//! 前端可调用的命令层。
//!
//! 约定：所有返回状态的命令统一回 `StateDto`，界面只认这一种结构。

use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, MutexGuard};

use chrono::{DateTime, Local};
use serde::Serialize;
use tauri::Manager;
use tauri::State;

use crate::config::{self, Config, Mode};
use crate::scheduler::{self, Conflict, Shared, Transition};
use crate::{suncalc, win32};

pub struct AppState {
    pub shared: Arc<Mutex<Shared>>,
    /// 调度线程的唤醒通道
    pub kick: Sender<()>,
}

/// 锁被 panic 污染时也继续用，总比整个应用卡死好。
pub fn lock(shared: &Arc<Mutex<Shared>>) -> MutexGuard<'_, Shared> {
    match shared.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct StateDto {
    pub config: Config,
    pub revision: u64,
    /// 系统当前实际主题
    pub theme: Mode,
    /// 调度最后一次主动断言的模式
    pub applied_mode: Option<Mode>,
    /// 下一个跳变点
    pub next: Option<Transition>,
    pub conflict: Option<Conflict>,
    pub last_error: Option<String>,
    pub notice: Option<String>,
    pub paused: bool,
    pub last_tick: Option<DateTime<Local>>,
    pub elevated: bool,
}

pub fn state_dto(shared: &Shared) -> StateDto {
    StateDto {
        config: shared.cfg.clone(),
        revision: shared.revision,
        theme: win32::current_theme(),
        applied_mode: shared.rt.applied.map(|t| t.mode),
        next: scheduler::next_transition(&shared.cfg, Local::now()),
        conflict: shared.rt.conflict,
        last_error: shared.rt.last_error.clone(),
        notice: shared.rt.notice.clone(),
        paused: shared.rt.paused,
        last_tick: shared.rt.last_tick,
        elevated: win32::is_elevated(),
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SunPreview {
    pub date: String,
    /// HH:MM 本地时间
    pub sunrise: Option<String>,
    pub sunset: Option<String>,
    pub polar: bool,
}

#[tauri::command]
pub fn get_state(state: State<'_, AppState>) -> StateDto {
    let guard = lock(&state.shared);
    state_dto(&guard)
}

#[tauri::command]
pub fn save_config(state: State<'_, AppState>, config: Config) -> Result<StateDto, String> {
    let mut next = config;
    let notes = next.sanitize();

    config::save(&config::config_dir(), &next)?;

    let dto = {
        let mut guard = lock(&state.shared);
        guard.cfg = next;
        guard.revision += 1;
        // 清掉断言记录，让调度线程用新配置重新断言一次
        guard.rt.applied = None;
        guard.rt.applied_revision = None;
        guard.rt.followed_wallpaper = None;
        guard.rt.conflict = None;
        guard.rt.notice = if notes.is_empty() {
            None
        } else {
            Some(notes.join("；"))
        };
        state_dto(&guard)
    };

    let _ = state.kick.send(());
    Ok(dto)
}

/// 手动立即切换。`mode` 省略时按系统当前主题来一次（用于「立即应用」）。
///
/// 手动结果会被记为「已跟随」，因此不会被误判成外部冲突；
/// 下一个调度跳变点到来时仍会按计划重新断言。
#[tauri::command]
pub fn apply_now(state: State<'_, AppState>, mode: Option<Mode>) -> Result<StateDto, String> {
    let dto = {
        let mut guard = lock(&state.shared);
        let target = mode.unwrap_or_else(win32::current_theme);
        let cfg = guard.cfg.clone();

        match scheduler::apply_mode(&cfg, target) {
            Ok(note) => {
                guard.rt.followed_wallpaper = Some(target);
                guard.rt.conflict = None;
                guard.rt.last_error = note;
            }
            Err(err) => {
                guard.rt.last_error = Some(err.clone());
                return Err(err);
            }
        }
        state_dto(&guard)
    };

    // 让调度线程同步内部状态，避免下一 tick 重复动作
    let _ = state.kick.send(());
    Ok(dto)
}

#[tauri::command]
pub fn set_paused(state: State<'_, AppState>, paused: bool) -> Result<StateDto, String> {
    let dto = {
        let mut guard = lock(&state.shared);
        guard.rt.paused = paused;
        state_dto(&guard)
    };
    let _ = state.kick.send(());
    Ok(dto)
}

#[tauri::command]
pub fn sun_preview(latitude: f64, longitude: f64) -> SunPreview {
    let today = Local::now().date_naive();
    let times = suncalc::sun_times(today, latitude, longitude);

    let clock = |dt: Option<DateTime<Local>>| dt.map(|t| t.format("%H:%M").to_string());

    SunPreview {
        date: today.format("%Y-%m-%d").to_string(),
        sunrise: clock(times.sunrise),
        sunset: clock(times.sunset),
        polar: times.polar,
    }
}

#[tauri::command]
pub fn open_color_settings() -> Result<(), String> {
    win32::open_uri("ms-settings:colors")
}

/// 缩略图 data URL。文件不存在或解码失败时返回 null，界面显示占位。
#[tauri::command]
pub fn wallpaper_preview(path: String) -> Option<String> {
    crate::thumbnail::data_url(std::path::Path::new(&path))
}

/// 关闭「主题被外部改动」的提示。
#[tauri::command]
pub fn dismiss_conflict(state: State<'_, AppState>) -> StateDto {
    let mut guard = lock(&state.shared);
    guard.rt.conflict = None;
    state_dto(&guard)
}

/// 注册锁屏助手任务。未提权时拉起自身的管理员实例去注册（会弹一次 UAC）。
#[tauri::command]
pub fn setup_lock_helper() -> Result<(), String> {
    if win32::is_elevated() {
        win32::install_lock_helper()
    } else {
        win32::relaunch_elevated(&["--install-lock-helper".to_string()])
    }
}

/// 助手任务是否已注册。按需查询，不放进 state_dto —— 那会每 15 秒起一次 schtasks。
#[tauri::command]
pub fn lock_helper_ready() -> bool {
    win32::lock_helper_registered()
}

/// 撤销对锁屏壁纸的接管：先关配置开关，再删任务、清注册表。
///
/// 顺序不能反 —— 开关还开着的话，下一次断言又会把锁屏写回去。
#[tauri::command]
pub fn clear_lock_screen(state: State<'_, AppState>) -> Result<StateDto, String> {
    let (dto, snapshot) = {
        let mut guard = lock(&state.shared);
        guard.cfg.lock_screen = false;
        guard.revision += 1;
        guard.rt.last_error = None;
        (state_dto(&guard), guard.cfg.clone())
    };

    config::save(&config::config_dir(), &snapshot)?;

    if win32::is_elevated() {
        win32::uninstall_lock_helper();
        win32::clear_lock_screen()?;
    } else {
        win32::relaunch_elevated(&["--uninstall-lock-helper".to_string()])?;
    }

    let _ = state.kick.send(());
    Ok(dto)
}

/// 把当前主题与壁纸一次性对齐到系统现有主题（用于启动时同步自己窗口外观）。
#[tauri::command]
pub fn current_theme() -> Mode {
    win32::current_theme()
}

/// 返回内置壁纸（浅色 / 深色）的绝对路径，找不到时对应字段为 null。
///
/// 资源路径：打包后位于安装目录的 `resources/` 子目录（`app.path().resource_dir()`）；
/// dev 模式下 `tauri dev` 会设置资源目录指向 `src-tauri/target/<profile>/`，
/// 如果那里没有，额外回落到 `CARGO_MANIFEST_DIR/resources/`。
#[tauri::command]
pub fn builtin_wallpapers(app: tauri::AppHandle) -> BuiltinWallpapers {
    let candidates = build_resource_candidates(&app);
    let pick = |name: &str| -> Option<std::path::PathBuf> {
        for dir in &candidates {
            let path = dir.join(name);
            if path.is_file() {
                return Some(path);
            }
        }
        None
    };
    BuiltinWallpapers {
        light: pick("builtin-light.png"),
        dark: pick("builtin-dark.png"),
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BuiltinWallpapers {
    pub light: Option<std::path::PathBuf>,
    pub dark: Option<std::path::PathBuf>,
}

fn build_resource_candidates(app: &tauri::AppHandle) -> Vec<std::path::PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(dir) = app.path().resource_dir() {
        dirs.push(dir);
    }
    // dev 模式回落：直接指向源码里的 resources/，免得 tauri dev 找不到。
    // 打包后这目录不存在候选会被 is_file() 过滤掉。
    dirs.push(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources"));
    dirs
}

/// 首次启动填充内置壁纸。
///
/// 仅当 `config.json` 还不存在时把内置壁纸路径写入配置，**不调用 apply**——
/// 避免装好就偷偷换桌面，让用户在界面上点「立即应用」才会生效。
///
/// 返回是否做了填充（true=填充了），供 setup 里决定要不要给前端发一次事件。
pub fn seed_builtin_wallpapers_if_first_run(
    app: &tauri::AppHandle,
    shared: &Arc<Mutex<Shared>>,
    config_dir: &std::path::Path,
) -> bool {
    let path = config::config_path(config_dir);
    if path.is_file() {
        return false;
    }

    let candidates = build_resource_candidates(app);
    let pick = |name: &str| -> Option<std::path::PathBuf> {
        candidates
            .iter()
            .map(|dir| dir.join(name))
            .find(|p| p.is_file())
    };

    let light = pick("builtin-light.png");
    let dark = pick("builtin-dark.png");
    if light.is_none() && dark.is_none() {
        return false;
    }

    let snapshot = {
        let mut guard = lock(shared);
        if let Some(ref p) = light {
            guard.cfg.light_wallpaper = Some(p.clone());
        }
        if let Some(ref p) = dark {
            guard.cfg.dark_wallpaper = Some(p.clone());
        }
        guard.revision += 1;
        guard.cfg.clone()
    };

    if let Err(err) = config::save(config_dir, &snapshot) {
        eprintln!("写入首次启动内置壁纸配置失败：{err}");
        return false;
    }
    true
}
