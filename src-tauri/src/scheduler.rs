//! 调度引擎：按时段槽决定何时切换，并识别被外部改动的主题。
//!
//! 设计要点 —— 不做「定时器到点执行」，而是每次 tick 用当前时刻反推
//! 「本时段应处的模式」。好处是电脑休眠/唤醒、系统时间被改、时钟漂移
//! 都不需要额外处理，醒来后第一次 tick 自然会把状态纠正过来。
//!
//! 控制权策略：每个时段槽只主动断言一次。槽内如果检测到主题被外部改掉
//! （典型是 Win11 原生的「自动在浅色/深色间切换」在打架），**不夺回**，
//! 只把壁纸跟随过去并在界面上报警 —— 否则会与系统调度互相翻转成死循环。

use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use chrono::{DateTime, Datelike, Duration as ChronoDuration, Local, NaiveDate};
use serde::Serialize;

use crate::config::{Config, Mode, Schedule};
use crate::{suncalc, win32};

/// tick 间隔。每次只读两个注册表值，开销可忽略。
const TICK: Duration = Duration::from_secs(15);

/// 一次调度跳变：(生效时刻, 应处的模式)。
/// `at` 同时充当「时段槽」的唯一标识。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Transition {
    pub at: DateTime<Local>,
    pub mode: Mode,
}

/// 某个日期上的全部跳变点，按时间升序。
/// 极昼/极夜或时区落入夏令时缝隙时，对应条目会被跳过。
pub fn transitions_on(cfg: &Config, date: NaiveDate) -> Vec<Transition> {
    let mut out: Vec<Transition> = Vec::new();

    match &cfg.schedule {
        Schedule::Fixed { rules } => {
            for rule in rules {
                if !rule.days.matches(date.weekday()) {
                    continue;
                }
                let Some(naive) = date.and_hms_opt(rule.minutes / 60, rule.minutes % 60, 0) else {
                    continue;
                };
                if let Some(at) = resolve_local(naive) {
                    out.push(Transition { at, mode: rule.mode });
                }
            }
        }
        Schedule::Sun {
            latitude,
            longitude,
        } => {
            let times = suncalc::sun_times(date, *latitude, *longitude);
            if let Some(rise) = times.sunrise {
                out.push(Transition {
                    at: rise,
                    mode: Mode::Light,
                });
            }
            if let Some(set) = times.sunset {
                out.push(Transition {
                    at: set,
                    mode: Mode::Dark,
                });
            }
        }
    }

    out.sort_by_key(|t| t.at);
    out
}

/// 本地时间解析。夏令时「不存在的时刻」取最近的有效时刻，避免整条规则失效。
fn resolve_local(naive: chrono::NaiveDateTime) -> Option<DateTime<Local>> {
    let result = naive.and_local_timezone(Local);
    result
        .single()
        .or_else(|| result.earliest())
        .or_else(|| result.latest())
}

/// 当前应处的模式及其生效时刻。向前最多回溯 8 天，
/// 以保证「周末不切换」这类规则能一直沿用上一个工作日的结果。
pub fn current_transition(cfg: &Config, now: DateTime<Local>) -> Option<Transition> {
    for back in 0..=8i64 {
        let date = now.date_naive() - ChronoDuration::days(back);
        let list = transitions_on(cfg, date);
        if let Some(found) = list.into_iter().rev().find(|t| t.at <= now) {
            return Some(found);
        }
    }
    None
}

/// 下一个将要发生的跳变点。向后最多找 8 天，供界面显示「下次切换」。
pub fn next_transition(cfg: &Config, now: DateTime<Local>) -> Option<Transition> {
    for ahead in 0..=8i64 {
        let date = now.date_naive() + ChronoDuration::days(ahead);
        let list = transitions_on(cfg, date);
        if let Some(found) = list.into_iter().find(|t| t.at > now) {
            return Some(found);
        }
    }
    None
}

/// 外部改动记录：系统在我们之外改了主题。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Conflict {
    /// 调度认为该处的模式
    pub expected: Mode,
    /// 实际被改成的模式
    pub actual: Mode,
    pub at: DateTime<Local>,
}

#[derive(Debug, Clone, Default)]
pub struct RuntimeState {
    /// 已主动断言过的时段槽
    pub applied: Option<Transition>,
    /// 断言时的配置版本。配置一改就重新断言，避免改了壁纸不生效。
    pub applied_revision: Option<u64>,
    /// 已经为哪个模式跟随过壁纸，防止每 tick 重复调用系统 API
    pub followed_wallpaper: Option<Mode>,
    pub conflict: Option<Conflict>,
    pub last_error: Option<String>,
    /// 配置保存时被自动修正的内容说明
    pub notice: Option<String>,
    pub paused: bool,
    pub last_tick: Option<DateTime<Local>>,
}

#[derive(Debug, Clone)]
pub struct Shared {
    pub cfg: Config,
    /// 配置版本号，每次保存自增
    pub revision: u64,
    pub rt: RuntimeState,
}

impl Default for Shared {
    fn default() -> Self {
        Self {
            cfg: Config::default(),
            revision: 0,
            rt: RuntimeState::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Idle,
    /// 时段槽切换（或配置变更）：同时断言主题与壁纸
    Apply(Transition),
    /// 槽内主题被外部改掉：只跟随壁纸，不夺回控制权
    FollowWallpaper(Mode),
}

/// 纯决策函数，便于单元测试。
pub fn decide(
    rt: &RuntimeState,
    slot: Option<Transition>,
    revision: u64,
    theme: Mode,
) -> Decision {
    let Some(slot) = slot else {
        return Decision::Idle;
    };

    let same_slot =
        rt.applied == Some(slot) && rt.applied_revision == Some(revision);

    if !same_slot {
        return Decision::Apply(slot);
    }

    if theme == slot.mode {
        Decision::Idle
    } else if rt.followed_wallpaper == Some(theme) {
        Decision::Idle
    } else {
        Decision::FollowWallpaper(theme)
    }
}

/// 按给定模式断言主题与壁纸。锁屏失败不阻断主题/壁纸。
pub fn apply_mode(cfg: &Config, mode: Mode) -> Result<Option<String>, String> {
    win32::set_theme(mode)?;

    if let Some(wallpaper) = cfg.wallpaper_for(mode) {
        win32::set_wallpaper(wallpaper)?;
    }

    if !cfg.lock_screen {
        return Ok(None);
    }

    let Some(wallpaper) = cfg.wallpaper_for(mode) else {
        return Ok(Some("该模式未配置壁纸，锁屏未更新".into()));
    };

    // 本进程提权了就直接写；否则交给事先注册好的最高权限助手任务静默完成
    let outcome = if win32::is_elevated() {
        win32::set_lock_screen(wallpaper)
    } else {
        win32::run_lock_helper()
    };

    match outcome {
        Ok(()) => Ok(None),
        Err(err) => Ok(Some(format!("锁屏壁纸未更新：{err}"))),
    }
}

/// 启动调度线程，返回唤醒通道：往里 `send(())` 可让它立刻跑一次 tick，
/// 不必等到下一个 15 秒周期（配置变更、手动切换、暂停开关后都用得上）。
///
/// 通道全部断开时线程自行退出，因此调用方只需持有通道的副本。
/// `notify` 在每次 tick 后收到一份状态快照，用于向界面推送；
/// 它**不应**再去锁同一个 Mutex（快照已独立）。
pub fn spawn<F>(shared: Arc<Mutex<Shared>>, notify: F) -> Sender<()>
where
    F: Fn(Shared) + Send + 'static,
{
    let (tx, rx): (Sender<()>, Receiver<()>) = std::sync::mpsc::channel();

    let worker = Arc::clone(&shared);
    thread::spawn(move || loop {
        let snapshot = {
            let mut guard = match worker.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            tick(&mut guard);
            guard.clone()
        };
        notify(snapshot);

        match rx.recv_timeout(TICK) {
            Ok(()) => continue,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => break,
        }
    });

    tx
}

fn tick(shared: &mut Shared) {
    let now = Local::now();
    shared.rt.last_tick = Some(now);

    if !shared.cfg.enabled || shared.rt.paused {
        // 复位，恢复调度时重新断言一次
        shared.rt.applied = None;
        shared.rt.applied_revision = None;
        shared.rt.followed_wallpaper = None;
        return;
    }

    let slot = current_transition(&shared.cfg, now);
    let theme = win32::current_theme();

    match decide(&shared.rt, slot, shared.revision, theme) {
        Decision::Idle => {}
        Decision::Apply(target) => match apply_mode(&shared.cfg, target.mode) {
            Ok(note) => {
                shared.rt.applied = Some(target);
                shared.rt.applied_revision = Some(shared.revision);
                shared.rt.followed_wallpaper = None;
                shared.rt.conflict = None;
                shared.rt.last_error = note;
            }
            Err(err) => {
                shared.rt.last_error = Some(err);
            }
        },
        Decision::FollowWallpaper(actual) => {
            if let Some(wallpaper) = shared.cfg.wallpaper_for(actual) {
                if let Err(err) = win32::set_wallpaper(wallpaper) {
                    shared.rt.last_error = Some(err);
                }
            }
            shared.rt.followed_wallpaper = Some(actual);
            shared.rt.conflict = Some(Conflict {
                expected: slot.map_or(actual, |s| s.mode),
                actual,
                at: now,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{DayFilter, TimeRule};
    use chrono::Datelike;

    fn at(date: NaiveDate, hour: u32, minute: u32) -> DateTime<Local> {
        resolve_local(date.and_hms_opt(hour, minute, 0).expect("合法时间")).expect("本地时间可解析")
    }

    fn fixed_cfg(rules: Vec<TimeRule>) -> Config {
        Config {
            schedule: Schedule::Fixed { rules },
            ..Config::default()
        }
    }

    fn rule(mode: Mode, hour: u32, minute: u32, days: DayFilter) -> TimeRule {
        TimeRule {
            mode,
            minutes: hour * 60 + minute,
            days,
        }
    }

    #[test]
    fn weekday_filter_drops_weekend_transitions() {
        let cfg = fixed_cfg(vec![rule(Mode::Light, 7, 0, DayFilter::Weekdays)]);
        let saturday = NaiveDate::from_ymd_opt(2026, 9, 12).expect("周六");
        let monday = NaiveDate::from_ymd_opt(2026, 9, 14).expect("周一");

        assert!(transitions_on(&cfg, saturday).is_empty(), "周六不应有跳变");
        assert_eq!(transitions_on(&cfg, monday).len(), 1, "周一应有一条");
    }

    #[test]
    fn current_transition_holds_last_mode_across_midnight() {
        // 只有一条 07:00 转亮色的规则：跨过午夜后到次日 07:00 前应仍是亮色
        let cfg = fixed_cfg(vec![rule(Mode::Light, 7, 0, DayFilter::Daily)]);

        let noon = at(NaiveDate::from_ymd_opt(2026, 9, 11).expect("日期"), 12, 0);
        let mid = at(NaiveDate::from_ymd_opt(2026, 9, 12).expect("日期"), 3, 0);

        let a = current_transition(&cfg, noon).expect("应有生效槽");
        let b = current_transition(&cfg, mid).expect("应有生效槽(回溯)");

        assert_eq!(a.mode, Mode::Light);
        assert_eq!(b.mode, Mode::Light);
        assert_eq!(b.at.date_naive().day(), 11, "应回溯到 9/11 的 07:00");
    }

    #[test]
    fn morning_is_light_and_evening_is_dark() {
        let cfg = Config::default();
        let date = NaiveDate::from_ymd_opt(2026, 9, 11).expect("日期");

        let morning = current_transition(&cfg, at(date, 9, 0)).expect("槽");
        assert_eq!(morning.mode, Mode::Light);
        assert_eq!(morning.at, at(date, 7, 0));

        let evening = current_transition(&cfg, at(date, 21, 0)).expect("槽");
        assert_eq!(evening.mode, Mode::Dark);
        assert_eq!(evening.at, at(date, 19, 0));

        // 边界：正好 19:00 应算入暗色时段
        let boundary = current_transition(&cfg, at(date, 19, 0)).expect("槽");
        assert_eq!(boundary.mode, Mode::Dark);
    }

    #[test]
    fn sun_schedule_produces_rise_then_set() {
        let cfg = Config {
            schedule: Schedule::Sun {
                latitude: 39.9042,
                longitude: 116.4074,
            },
            ..Config::default()
        };
        let date = NaiveDate::from_ymd_opt(2026, 9, 11).expect("日期");
        let list = transitions_on(&cfg, date);

        assert_eq!(list.len(), 2, "应产生日出与日落两个跳变");
        assert_eq!(list[0].mode, Mode::Light);
        assert_eq!(list[1].mode, Mode::Dark);
        assert!(list[0].at < list[1].at, "日出应早于日落");
    }

    #[test]
    fn apply_when_slot_changes_idle_when_consistent() {
        let date = NaiveDate::from_ymd_opt(2026, 9, 11).expect("日期");
        let slot = Transition {
            at: at(date, 7, 0),
            mode: Mode::Light,
        };
        let mut rt = RuntimeState::default();

        // 首次：不管主题是什么都要断言一次
        assert_eq!(
            decide(&rt, Some(slot), 0, Mode::Dark),
            Decision::Apply(slot)
        );

        rt.applied = Some(slot);
        rt.applied_revision = Some(0);

        // 一致 -> 空闲
        assert_eq!(decide(&rt, Some(slot), 0, Mode::Light), Decision::Idle);

        // 外部改成暗色 -> 只跟随壁纸
        assert_eq!(
            decide(&rt, Some(slot), 0, Mode::Dark),
            Decision::FollowWallpaper(Mode::Dark)
        );

        // 已经跟随过 -> 不再重复调用系统 API
        rt.followed_wallpaper = Some(Mode::Dark);
        assert_eq!(decide(&rt, Some(slot), 0, Mode::Dark), Decision::Idle);
    }

    #[test]
    fn config_change_forces_reapply() {
        let date = NaiveDate::from_ymd_opt(2026, 9, 11).expect("日期");
        let slot = Transition {
            at: at(date, 7, 0),
            mode: Mode::Light,
        };
        let mut rt = RuntimeState::default();
        rt.applied = Some(slot);
        rt.applied_revision = Some(3);

        // 配置版本变了（换了壁纸），即使时段槽相同也要重新断言
        assert_eq!(
            decide(&rt, Some(slot), 4, Mode::Light),
            Decision::Apply(slot)
        );
    }

    #[test]
    fn no_transition_yields_idle() {
        let cfg = fixed_cfg(vec![rule(Mode::Light, 7, 0, DayFilter::Weekend)]);
        let monday = NaiveDate::from_ymd_opt(2026, 9, 14).expect("周一");
        let now = at(monday, 12, 0);

        // 工作日没有任何规则，且回溯 8 天内有周末的规则，因此应能取到槽
        let slot = current_transition(&cfg, now);
        assert!(slot.is_some(), "回溯范围内应找到周末的跳变");

        // 完全空规则时才会是 None
        let empty = fixed_cfg(Vec::new());
        assert!(current_transition(&empty, now).is_none());
        assert_eq!(
            decide(&RuntimeState::default(), None, 0, Mode::Light),
            Decision::Idle
        );
    }

    #[test]
    fn next_transition_walks_into_following_days() {
        let date = NaiveDate::from_ymd_opt(2026, 9, 11).expect("日期");
        let cfg = Config::default();

        // 09:00 时下一个跳变应是当天 19:00 转暗色
        let next = next_transition(&cfg, at(date, 9, 0)).expect("应有下一次跳变");
        assert_eq!(next.mode, Mode::Dark);
        assert_eq!(next.at, at(date, 19, 0));

        // 21:00 时已过当天全部跳变，应跨到次日 07:00
        let tomorrow = next_transition(&cfg, at(date, 21, 0)).expect("应有下一次跳变");
        assert_eq!(tomorrow.mode, Mode::Light);
        assert_eq!(tomorrow.at.date_naive().day(), 12);
    }
}
