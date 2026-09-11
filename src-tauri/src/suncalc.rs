//! 日出日落计算（NOAA 近似式）。
//!
//! 只用本地系统时区，不联网。误差在中纬度约 ±1 分钟，够调度用。
//! 时区换算交给 chrono 的 `Local`，夏令时由系统时区库处理。

use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, TimeZone, Utc};

/// 含折射与日面半径的修正天顶角（度）
const ZENITH_DEG: f64 = 90.833;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SunTimes {
    pub sunrise: Option<DateTime<Local>>,
    pub sunset: Option<DateTime<Local>>,
    /// 极昼或极夜：true 时上面两项均为 None，调度需降级处理
    pub polar: bool,
}

/// 给定日期与经纬度（经度东正西负），返回当日的日出与日落本地时间。
pub fn sun_times(date: NaiveDate, latitude: f64, longitude: f64) -> SunTimes {
    let none = SunTimes {
        sunrise: None,
        sunset: None,
        polar: false,
    };

    let utc_midnight = match Utc
        .with_ymd_and_hms(date.year(), date.month(), date.day(), 0, 0, 0)
        .single()
    {
        Some(dt) => dt,
        None => return none,
    };

    let lat_rad = latitude.to_radians();
    if !(-90.0..=90.0).contains(&latitude) || lat_rad.cos().abs() < f64::EPSILON {
        return none;
    }

    // 分数年（gamma）用于太阳赤纬与均时差
    let gamma = 2.0 * std::f64::consts::PI / 365.0 * (date.ordinal() as f64 - 1.0);

    // 均时差，分钟
    let eq_of_time = 229.18
        * (0.000075 + 0.001868 * gamma.cos()
            - 0.032077 * gamma.sin()
            - 0.014615 * (2.0 * gamma).cos()
            - 0.040849 * (2.0 * gamma).sin());

    // 太阳赤纬，弧度
    let declination = 0.006918 - 0.399912 * gamma.cos()
        + 0.070257 * gamma.sin()
        - 0.006758 * (2.0 * gamma).cos()
        + 0.000907 * (2.0 * gamma).sin()
        - 0.002697 * (3.0 * gamma).cos()
        + 0.00148 * (3.0 * gamma).sin();

    let cos_omega = (ZENITH_DEG.to_radians().cos() - lat_rad.sin() * declination.sin())
        / (lat_rad.cos() * declination.cos());

    // |cos ω| > 1 表示当天太阳不升或不落
    if cos_omega.abs() > 1.0 {
        return SunTimes {
            sunrise: None,
            sunset: None,
            polar: true,
        };
    }

    let omega_deg = cos_omega.acos().to_degrees();
    let solar_noon_utc = 12.0 - longitude / 15.0 - eq_of_time / 60.0;

    // 以「当日 00:00 UTC」为基准加秒数，负值会自动跨到前一天，物理上正确
    let to_local = |hours_utc: f64| -> Option<DateTime<Local>> {
        let secs = (hours_utc * 3600.0).round() as i64;
        Some((utc_midnight + Duration::seconds(secs)).with_timezone(&Local))
    };

    SunTimes {
        sunrise: to_local(solar_noon_utc - omega_deg / 15.0),
        sunset: to_local(solar_noon_utc + omega_deg / 15.0),
        polar: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Timelike;

    fn minutes(dt: DateTime<Local>) -> u32 {
        dt.hour() * 60 + dt.minute()
    }

    #[test]
    fn beijing_early_september_is_plausible() {
        let d = NaiveDate::from_ymd_opt(2026, 9, 11).expect("2026-09-11 合法");
        let t = sun_times(d, 39.9042, 116.4074);
        assert!(!t.polar, "北京不在极区");

        let sunrise = minutes(t.sunrise.expect("应有日出"));
        let sunset = minutes(t.sunset.expect("应有日落"));

        // 北京 9 月 11 日：日出 ~05:54，日落 ~18:26（北京时间）
        assert!(
            (5 * 60 + 40..=6 * 60 + 10).contains(&sunrise),
            "日出偏离预期：{sunrise} 分钟"
        );
        assert!(
            (18 * 60 + 10..=18 * 60 + 45).contains(&sunset),
            "日落偏离预期：{sunset} 分钟"
        );
    }

    #[test]
    fn equator_day_length_is_about_twelve_hours() {
        let d = NaiveDate::from_ymd_opt(2026, 3, 21).expect("春分");
        let t = sun_times(d, 0.0, 0.0);
        let sunrise = t.sunrise.expect("应有日出");
        let sunset = t.sunset.expect("应有日落");
        let span = (sunset - sunrise).num_minutes();
        assert!((span - 720).abs() < 15, "赤道春秋分昼长应约 720 分钟，实际 {span}");
    }

    #[test]
    fn south_latitude_flips_season() {
        // 南半球 9 月应是冬季，昼短夜长
        let d = NaiveDate::from_ymd_opt(2026, 9, 11).expect("日期合法");
        let t = sun_times(d, -33.8688, 151.2093); // 悉尼
        let span = (t.sunset.expect("日落") - t.sunrise.expect("日出")).num_minutes();
        assert!(span < 720, "南半球 9 月昼长应短于 12 小时，实际 {span} 分钟");
    }

    #[test]
    fn polar_night_is_flagged() {
        let d = NaiveDate::from_ymd_opt(2026, 12, 21).expect("冬至");
        let t = sun_times(d, 78.2232, 15.6469); // 朗伊尔城
        assert!(t.polar, "北极圈内冬至应为极夜");
        assert!(t.sunrise.is_none() && t.sunset.is_none());
    }
}
