//! Windows 平台操作：明暗主题读写与广播、桌面壁纸、锁屏壁纸、提权助手。
//!
//! 关键点：改完 `SystemUsesLightTheme` 注册表值后必须广播
//! `WM_SETTINGCHANGE` 且 lParam 指向 `"ImmersiveColorSet"`，
//! 否则任务栏、开始菜单等都不会立刻变。

use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use winreg::enums::{
    HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_QUERY_VALUE, KEY_SET_VALUE,
};
use winreg::RegKey;

use crate::config::Mode;

const PERSONALIZE_KEY: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Themes\Personalize";
const DESKTOP_KEY: &str = r"Control Panel\Desktop";
const LOCKSCREEN_CSP_KEY: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\PersonalizationCSP";

/// 填充模式（等比放大后裁切）
const WALLPAPER_STYLE_FILL: &str = "10";

fn to_wide(value: &std::ffi::OsStr) -> Vec<u16> {
    value.encode_wide().chain(std::iter::once(0)).collect()
}

fn wide(value: &str) -> Vec<u16> {
    std::ffi::OsStr::new(value)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// 读取系统当前明暗主题。读不到时按亮色处理（Windows 默认值）。
pub fn current_theme() -> Mode {
    let key = match RegKey::predef(HKEY_CURRENT_USER).open_subkey(PERSONALIZE_KEY) {
        Ok(key) => key,
        Err(_) => return Mode::Light,
    };
    let light: u32 = key.get_value("SystemUsesLightTheme").unwrap_or(1);
    if light == 0 {
        Mode::Dark
    } else {
        Mode::Light
    }
}

/// 广播主题变更，让 shell 与已运行的程序立刻重绘。
pub fn broadcast_theme_change() {
    use windows::Win32::UI::WindowsAndMessaging::{
        SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
    };

    let param = wide("ImmersiveColorSet");
    let mut result: usize = 0;

    unsafe {
        let _ = SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            windows::Win32::Foundation::WPARAM(0),
            windows::Win32::Foundation::LPARAM(param.as_ptr() as isize),
            SMTO_ABORTIFHUNG,
            5000,
            Some(&mut result),
        );
    }
}

/// 设置系统明暗主题（同时改应用级与系统级两个值），并广播生效。
pub fn set_theme(mode: Mode) -> Result<(), String> {
    let key = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(PERSONALIZE_KEY, KEY_SET_VALUE | KEY_QUERY_VALUE)
        .map_err(|e| format!("打开主题注册表失败：{e}"))?;

    let value: u32 = match mode {
        Mode::Light => 1,
        Mode::Dark => 0,
    };

    key.set_value("SystemUsesLightTheme", &value)
        .map_err(|e| format!("写入 SystemUsesLightTheme 失败：{e}"))?;
    key.set_value("AppsUseLightTheme", &value)
        .map_err(|e| format!("写入 AppsUseLightTheme 失败：{e}"))?;

    broadcast_theme_change();
    Ok(())
}

/// 设置桌面壁纸（填充模式）。
pub fn set_wallpaper(path: &Path) -> Result<(), String> {
    use windows::Win32::UI::WindowsAndMessaging::{
        SystemParametersInfoW, SPIF_SENDCHANGE, SPIF_UPDATEINIFILE, SPI_SETDESKWALLPAPER,
    };

    if !path.is_file() {
        return Err(format!("壁纸文件不存在：{}", path.display()));
    }

    // 先落样式，否则 SPI 会沿用上一次的缩放方式
    let desktop = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(DESKTOP_KEY, KEY_SET_VALUE)
        .map_err(|e| format!("打开桌面注册表失败：{e}"))?;
    desktop
        .set_value("WallpaperStyle", &WALLPAPER_STYLE_FILL)
        .map_err(|e| format!("写入 WallpaperStyle 失败：{e}"))?;
    desktop
        .set_value("TileWallpaper", &"0")
        .map_err(|e| format!("写入 TileWallpaper 失败：{e}"))?;

    let mut wide_path = to_wide(path.as_os_str());
    unsafe {
        SystemParametersInfoW(
            SPI_SETDESKWALLPAPER,
            0,
            Some(wide_path.as_mut_ptr() as *mut c_void),
            SPIF_UPDATEINIFILE | SPIF_SENDCHANGE,
        )
        .map_err(|e| format!("设置壁纸失败：{e}"))?;
    }
    Ok(())
}

/// 让本应用窗口的标题栏跟随明暗主题（否则深色系统下窗口顶边是白的）。
///
/// 收原始指针而不是 crate 自己的 `HWND`，避免与 Tauri 依赖的
/// windows crate 版本不一致时类型对不上。
pub fn set_dark_titlebar(hwnd: *mut c_void, dark: bool) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE};

    let value: i32 = i32::from(dark);
    unsafe {
        let _ = DwmSetWindowAttribute(
            HWND(hwnd),
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &value as *const i32 as *const c_void,
            std::mem::size_of::<i32>() as u32,
        );
    }
}

/// 是否以管理员身份运行。
pub fn is_elevated() -> bool {
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::Security::{
        GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
    };
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
            return false;
        }

        let mut elevation = TOKEN_ELEVATION::default();
        let mut returned: u32 = 0;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elevation as *mut _ as *mut c_void),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut returned,
        )
        .is_ok();
        let _ = CloseHandle(token);

        ok && elevation.TokenIsElevated != 0
    }
}

/// 以管理员身份重新启动自身，附加给定参数。会弹出 UAC 提示。
pub fn relaunch_elevated(args: &[String]) -> Result<(), String> {
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let exe = std::env::current_exe().map_err(|e| format!("取自身路径失败：{e}"))?;
    let exe_w = to_wide(exe.as_os_str());

    let joined = args.join(" ");
    let params_w = wide(&joined);
    let verb_w = wide("runas");

    unsafe {
        let result = ShellExecuteW(
            None,
            PCWSTR(verb_w.as_ptr()),
            PCWSTR(exe_w.as_ptr()),
            PCWSTR(params_w.as_ptr()),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        );
        // ShellExecuteW 返回值 <= 32 表示失败，32 之上是实例句柄
        if result.0 as usize <= 32 {
            return Err(format!(
                "请求管理员权限失败（ShellExecuteW 返回 {}），可能是被 UAC 取消",
                result.0 as usize
            ));
        }
    }
    Ok(())
}

/// 用系统默认程序打开一个 URI，例如 `ms-settings:colors`。
pub fn open_uri(uri: &str) -> Result<(), String> {
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let verb_w = wide("open");
    let uri_w = wide(uri);

    unsafe {
        let result = ShellExecuteW(
            None,
            PCWSTR(verb_w.as_ptr()),
            PCWSTR(uri_w.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        );
        if result.0 as usize <= 32 {
            return Err(format!("打开 {uri} 失败（返回 {}）", result.0 as usize));
        }
    }
    Ok(())
}

/// 写锁屏壁纸。需要管理员权限（HKLM 写入）。
pub fn set_lock_screen(path: &Path) -> Result<(), String> {
    if !path.is_file() {
        return Err(format!("锁屏壁纸文件不存在：{}", path.display()));
    }

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let (key, _) = hklm
        .create_subkey(LOCKSCREEN_CSP_KEY)
        .map_err(|e| format!("写入锁屏注册表失败（需要管理员权限）：{e}"))?;

    let target = path.display().to_string();
    key.set_value("LockScreenImageStatus", &1u32)
        .map_err(|e| format!("写入 LockScreenImageStatus 失败：{e}"))?;
    key.set_value("LockScreenImagePath", &target)
        .map_err(|e| format!("写入 LockScreenImagePath 失败：{e}"))?;
    key.set_value("LockScreenImageUrl", &target)
        .map_err(|e| format!("写入 LockScreenImageUrl 失败：{e}"))?;

    broadcast_theme_change();
    Ok(())
}

/// 撤销对锁屏壁纸的接管，交还给系统设置。需要管理员权限。
pub fn clear_lock_screen() -> Result<(), String> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    match hklm.delete_subkey_all(LOCKSCREEN_CSP_KEY) {
        Ok(()) => {
            broadcast_theme_change();
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("清除锁屏接管失败（需要管理员权限）：{e}")),
    }
}

// ------------------------------------------------------------ 锁屏提权助手
//
// 写 HKLM 必须提权，但每次切换都弹 UAC 没法用。
// 做法：用户在开启锁屏功能时批准一次 UAC，注册一个「最高权限、仅按需触发」的
// 计划任务；之后主程序用 schtasks /Run 静默触发它，不再弹窗。
// 主界面本身始终以普通权限运行。

const LOCK_HELPER_TASK: &str = "DuoSwitchLockHelper";

/// 计划任务 XML。`UserId` 必须是「机器名\用户名」形式。
fn lock_helper_xml(exe: &Path) -> Result<String, String> {
    let computer = std::env::var("COMPUTERNAME").map_err(|_| "读不到 COMPUTERNAME".to_string())?;
    let user = std::env::var("USERNAME").map_err(|_| "读不到 USERNAME".to_string())?;
    let command = exe.display().to_string();

    Ok(format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.4" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo>
    <Description>DuoSwitch 锁屏壁纸写入助手。仅按需触发，不设触发器。</Description>
  </RegistrationInfo>
  <Triggers />
  <Principals>
    <Principal id="Author">
      <UserId>{computer}\{user}</UserId>
      <LogonType>InteractiveToken</LogonType>
      <RunLevel>HighestAvailable</RunLevel>
    </Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <AllowHardTerminate>true</AllowHardTerminate>
    <StartWhenAvailable>true</StartWhenAvailable>
    <Enabled>true</Enabled>
    <Hidden>false</Hidden>
    <ExecutionTimeLimit>PT5M</ExecutionTimeLimit>
    <Priority>7</Priority>
  </Settings>
  <Actions Context="Author">
    <Exec>
      <Command>{command}</Command>
      <Arguments>--apply-lockscreen-auto</Arguments>
    </Exec>
  </Actions>
</Task>"#
    ))
}

/// 静默执行外部命令（不闪控制台窗口）。
fn run_quiet(program: &str, args: &[&str]) -> Result<std::process::ExitStatus, String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    std::process::Command::new(program)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map_err(|e| format!("执行 {program} 失败：{e}"))
}

/// 注册提权助手任务（需要管理员权限）。
pub fn install_lock_helper() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| format!("取自身路径失败：{e}"))?;
    let xml = lock_helper_xml(&exe)?;

    // schtasks /XML 要求 UTF-16LE 带 BOM
    let mut bytes: Vec<u8> = vec![0xFF, 0xFE];
    for unit in xml.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }

    let path = std::env::temp_dir().join("duoswitch-lock-helper.xml");
    std::fs::write(&path, &bytes).map_err(|e| format!("写任务定义失败：{e}"))?;

    let xml_arg = path.display().to_string();
    let status = run_quiet(
        "schtasks.exe",
        &["/Create", "/TN", LOCK_HELPER_TASK, "/XML", &xml_arg, "/F"],
    );
    let _ = std::fs::remove_file(&path);

    let status = status?;
    if !status.success() {
        return Err(format!(
            "注册锁屏助手任务失败（schtasks 退出码 {:?}）",
            status.code()
        ));
    }
    Ok(())
}

/// 触发助手任务。任务以最高权限运行，因此这里不需要提权，也不会弹 UAC。
pub fn run_lock_helper() -> Result<(), String> {
    let status = run_quiet("schtasks.exe", &["/Run", "/TN", LOCK_HELPER_TASK])?;
    if !status.success() {
        return Err("触发锁屏助手失败，可能尚未注册".to_string());
    }
    Ok(())
}

/// 删除助手任务（需要管理员权限；失败不视为错误）。
pub fn uninstall_lock_helper() {
    let _ = run_quiet("schtasks.exe", &["/Delete", "/TN", LOCK_HELPER_TASK, "/F"]);
}

/// 助手任务是否已注册（不区分是否过期）。
pub fn lock_helper_registered() -> bool {
    run_quiet("schtasks.exe", &["/Query", "/TN", LOCK_HELPER_TASK])
        .map(|status| status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_theme_reads_a_valid_variant() {
        // 真实读取，只验证取值合法
        let mode = current_theme();
        assert!(matches!(mode, Mode::Light | Mode::Dark));
    }

    #[test]
    fn set_wallpaper_rejects_missing_file() {
        let err = set_wallpaper(Path::new(r"C:\definitely\not\here.jpg"));
        assert!(err.is_err(), "不存在的文件应被拒绝");
    }
}
