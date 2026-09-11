//! 配置模型与 JSON 持久化。
//!
//! 存放在 `%APPDATA%\com.FinnXiong.duoswitch\config.json`，
//! 与 Tauri 的 app_config_dir 约定一致，但独立计算，
//! 便于 CLI 模式在未初始化 Tauri 的情况下也能读写。

use std::fs;
use std::path::{Path, PathBuf};

use chrono::Weekday;
use serde::{Deserialize, Serialize};

pub const APP_DIR: &str = "com.FinnXiong.duoswitch";
const FILE_NAME: &str = "config.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Light,
    Dark,
}

impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Light => "light",
            Mode::Dark => "dark",
        }
    }

    pub fn parse(text: &str) -> Option<Mode> {
        match text.to_ascii_lowercase().as_str() {
            "light" => Some(Mode::Light),
            "dark" => Some(Mode::Dark),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DayFilter {
    Daily,
    Weekdays,
    Weekend,
}

impl DayFilter {
    pub fn matches(self, weekday: Weekday) -> bool {
        match self {
            DayFilter::Daily => true,
            DayFilter::Weekdays => matches!(
                weekday,
                Weekday::Mon | Weekday::Tue | Weekday::Wed | Weekday::Thu | Weekday::Fri
            ),
            DayFilter::Weekend => matches!(weekday, Weekday::Sat | Weekday::Sun),
        }
    }
}

/// 固定时间点规则：`minutes` 为当日 00:00 起的分钟数。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeRule {
    pub mode: Mode,
    pub minutes: u32,
    pub days: DayFilter,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Schedule {
    Fixed { rules: Vec<TimeRule> },
    Sun { latitude: f64, longitude: f64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// 调度总开关
    pub enabled: bool,
    /// 亮色模式下使用的壁纸（绝对路径）
    pub light_wallpaper: Option<PathBuf>,
    /// 暗色模式下使用的壁纸（绝对路径）
    pub dark_wallpaper: Option<PathBuf>,
    pub schedule: Schedule,
    /// 是否同时接管锁屏壁纸（需要管理员权限）
    pub lock_screen: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: true,
            light_wallpaper: None,
            dark_wallpaper: None,
            schedule: Schedule::Fixed {
                rules: vec![
                    TimeRule {
                        mode: Mode::Light,
                        minutes: 7 * 60,
                        days: DayFilter::Daily,
                    },
                    TimeRule {
                        mode: Mode::Dark,
                        minutes: 19 * 60,
                        days: DayFilter::Daily,
                    },
                ],
            },
            lock_screen: false,
        }
    }
}

impl Config {
    pub fn wallpaper_for(&self, mode: Mode) -> Option<&Path> {
        match mode {
            Mode::Light => self.light_wallpaper.as_deref(),
            Mode::Dark => self.dark_wallpaper.as_deref(),
        }
    }

    /// 校正越界输入，并剔除指向不存在文件的壁纸路径。
    /// 返回被修正的说明，便于界面提示。
    pub fn sanitize(&mut self) -> Vec<String> {
        let mut notes = Vec::new();

        match &mut self.schedule {
            Schedule::Fixed { rules } => {
                for rule in rules.iter_mut() {
                    let clamped = rule.minutes.min(24 * 60 - 1);
                    if clamped != rule.minutes {
                        notes.push(format!("时间点 {} 超出范围，已收敛到 23:59", rule.minutes));
                        rule.minutes = clamped;
                    }
                }
            }
            Schedule::Sun {
                latitude,
                longitude,
            } => {
                if !(-90.0..=90.0).contains(latitude) {
                    notes.push("纬度超出 -90~90，已重置为 39.9042".into());
                    *latitude = 39.9042;
                }
                if !(-180.0..=180.0).contains(longitude) {
                    notes.push("经度超出 -180~180，已重置为 116.4074".into());
                    *longitude = 116.4074;
                }
            }
        }

        for (label, slot) in [
            ("亮色", &mut self.light_wallpaper),
            ("暗色", &mut self.dark_wallpaper),
        ] {
            if let Some(path) = slot.as_ref() {
                if !path.is_file() {
                    notes.push(format!("{}壁纸文件不存在，已清除：{}", label, path.display()));
                    *slot = None;
                }
            }
        }

        notes
    }
}

pub fn config_dir() -> PathBuf {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(APP_DIR)
}

pub fn config_path(dir: &Path) -> PathBuf {
    dir.join(FILE_NAME)
}

/// 读取配置。文件缺失或损坏时返回默认配置，并把损坏文件改名备份。
pub fn load(dir: &Path) -> Config {
    let path = config_path(dir);
    let raw = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(_) => return Config::default(),
    };

    match serde_json::from_str::<Config>(&raw) {
        Ok(cfg) => cfg,
        Err(err) => {
            let backup = path.with_extension("json.broken");
            let _ = fs::rename(&path, &backup);
            eprintln!(
                "config.json 解析失败（{err}），已备份到 {}，改用默认配置",
                backup.display()
            );
            Config::default()
        }
    }
}

/// 原子写入：先写临时文件再 rename，避免掉电/崩溃留下半截 JSON。
pub fn save(dir: &Path, cfg: &Config) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| format!("创建配置目录失败：{e}"))?;

    let path = config_path(dir);
    let tmp = path.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(cfg).map_err(|e| format!("序列化配置失败：{e}"))?;

    fs::write(&tmp, text).map_err(|e| format!("写入临时配置失败：{e}"))?;
    // std::fs::rename 在 Windows 上使用 MOVEFILE_REPLACE_EXISTING，可覆盖目标
    fs::rename(&tmp, &path).map_err(|e| format!("替换配置文件失败：{e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_daily_seven_and_nineteen() {
        let cfg = Config::default();
        match &cfg.schedule {
            Schedule::Fixed { rules } => {
                assert_eq!(rules.len(), 2);
                assert_eq!(rules[0].mode, Mode::Light);
                assert_eq!(rules[0].minutes, 420);
                assert_eq!(rules[1].mode, Mode::Dark);
                assert_eq!(rules[1].minutes, 1140);
            }
            other => panic!("默认调度应为固定时间点，实际 {other:?}"),
        }
    }

    #[test]
    fn day_filter_respects_weekday() {
        assert!(DayFilter::Daily.matches(Weekday::Sun));
        assert!(DayFilter::Weekdays.matches(Weekday::Wed));
        assert!(!DayFilter::Weekdays.matches(Weekday::Sat));
        assert!(DayFilter::Weekend.matches(Weekday::Sat));
        assert!(!DayFilter::Weekend.matches(Weekday::Mon));
    }

    #[test]
    fn sanitize_clamps_time_and_clears_missing_wallpaper() {
        let mut cfg = Config::default();
        cfg.schedule = Schedule::Fixed {
            rules: vec![TimeRule {
                mode: Mode::Dark,
                minutes: 5000,
                days: DayFilter::Daily,
            }],
        };
        cfg.light_wallpaper = Some(PathBuf::from(r"C:\definitely\not\here.jpg"));

        let notes = cfg.sanitize();

        match &cfg.schedule {
            Schedule::Fixed { rules } => assert_eq!(rules[0].minutes, 1439),
            other => panic!("调度类型被意外改变：{other:?}"),
        }
        assert!(cfg.light_wallpaper.is_none());
        assert_eq!(notes.len(), 2, "应报告两条修正：{notes:?}");
    }

    #[test]
    fn round_trip_through_json() {
        let cfg = Config {
            schedule: Schedule::Sun {
                latitude: 39.9042,
                longitude: 116.4074,
            },
            lock_screen: true,
            ..Config::default()
        };
        let text = serde_json::to_string(&cfg).expect("序列化");
        let back: Config = serde_json::from_str(&text).expect("反序列化");
        assert_eq!(cfg, back);
    }

    #[test]
    fn missing_fields_fall_back_to_default() {
        let cfg: Config = serde_json::from_str(r#"{"lock_screen":true}"#).expect("部分字段应可解析");
        assert!(cfg.lock_screen);
        assert!(cfg.enabled, "缺失字段应取默认值");
        assert_eq!(cfg.schedule, Config::default().schedule);
    }
}
