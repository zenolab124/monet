//! Cron 表达式单一入口。
//!
//! 存储用 **vixie/POSIX 惯例**（0/7=Sun, 1=Mon…6=Sat）——crontab(5)、
//! crontab.guru、系统 crontab 全用这套，用户/Agent 抄网上例子即可用。
//! 底层 cron crate 0.15 用 **Quartz 惯例**（1=Sun…7=Sat），本文件负责
//! 在传给 cron crate 之前把 dow 字段做一次映射，其余字段原样透传。
//!
//! 所有 `Schedule::from_str` 调用前必须经 `to_quartz_full()` 拿到 6 段
//! Quartz 表达式；调用 `format!("0 {}", raw_expr)` 属直接漏出 vixie
//! 表达式的历史反模式，禁止再新增。

/// 5 段 vixie cron → 6 段 Quartz cron（补秒 `0`，dow 字段做惯例映射）。
/// 输入非 5 段时原样返回补秒版本，交由 cron crate 报错。
pub fn to_quartz_full(vixie: &str) -> String {
    let parts: Vec<&str> = vixie.split_whitespace().collect();
    if parts.len() != 5 {
        return format!("0 {}", vixie);
    }
    format!(
        "0 {} {} {} {} {}",
        parts[0],
        parts[1],
        parts[2],
        parts[3],
        normalize_dow(parts[4])
    )
}

/// 只对 dow 字段做 vixie→Quartz 数字映射。
/// 规则：数字 n（0..=7）→ ((n % 7) + 1)。step 位不映射（间隔非 dow 值）。
/// `*` / `?` / `L` / `W` / `#` 等符号不动（后三个 vixie 不合法，透传交给下游报错）。
fn normalize_dow(field: &str) -> String {
    field
        .split(',')
        .map(normalize_dow_clause)
        .collect::<Vec<_>>()
        .join(",")
}

fn normalize_dow_clause(clause: &str) -> String {
    let (base, step) = match clause.split_once('/') {
        Some((b, s)) => (b, Some(s)),
        None => (clause, None),
    };
    let normalized_base = if let Some((a, b)) = base.split_once('-') {
        match (map_dow(a), map_dow(b)) {
            (Some(a), Some(b)) => format!("{}-{}", a, b),
            _ => base.to_string(),
        }
    } else {
        map_dow(base).map_or_else(|| base.to_string(), |n| n.to_string())
    };
    match step {
        Some(s) => format!("{}/{}", normalized_base, s),
        None => normalized_base,
    }
}

/// dow 值映射：0/7=Sun→1，1=Mon→2，……6=Sat→7。非数字（如 `*`）返回 None。
fn map_dow(tok: &str) -> Option<u32> {
    let n: u32 = tok.parse().ok()?;
    if n > 7 {
        return None;
    }
    Some((n % 7) + 1)
}

/// vixie dow 字段 → systemd OnCalendar 命名 dow 段（Linux 调度分支专用）。
/// systemd OnCalendar 只吃命名周几（Mon..Sun），不吃数字——数字范围如 `2-4` 直接
/// 塞进去会被 systemd 拒识、routine 一律建不起来。此函数把 vixie 数字整段展开为
/// 命名。支持 `*`、单数字、`a-b` 范围、`a,b,c` 列表、`a-b/step`、命名（透传+首字母
/// 大写规整）。任一 token 无法解析返回 None。
#[allow(dead_code)] // 仅 Linux 分支消费；macOS/Windows 编译期没有调用点
pub fn vixie_dow_to_systemd(dow: &str) -> Option<String> {
    fn num_to_name(n: u32) -> Option<&'static str> {
        // vixie 允许 0 与 7 都表示 Sunday；超出 0..=7 视为非法
        if n > 7 {
            return None;
        }
        match n % 7 {
            0 => Some("Sun"),
            1 => Some("Mon"),
            2 => Some("Tue"),
            3 => Some("Wed"),
            4 => Some("Thu"),
            5 => Some("Fri"),
            6 => Some("Sat"),
            _ => unreachable!(),
        }
    }
    fn convert_token(tok: &str) -> Option<String> {
        let lower = tok.to_ascii_lowercase();
        if matches!(
            lower.as_str(),
            "mon" | "tue" | "wed" | "thu" | "fri" | "sat" | "sun"
        ) {
            let mut chars = lower.chars();
            let first = chars.next()?.to_ascii_uppercase();
            return Some(format!("{}{}", first, chars.as_str()));
        }
        tok.parse::<u32>().ok().and_then(num_to_name).map(String::from)
    }
    let parts: Result<Vec<String>, ()> = dow
        .split(',')
        .map(|clause| {
            let (base, step) = match clause.split_once('/') {
                Some((b, s)) => (b, Some(s)),
                None => (clause, None),
            };
            let base_out = if base == "*" {
                "*".to_string()
            } else if let Some((a, b)) = base.split_once('-') {
                let a = convert_token(a).ok_or(())?;
                let b = convert_token(b).ok_or(())?;
                format!("{}..{}", a, b)
            } else {
                convert_token(base).ok_or(())?
            };
            Ok(match step {
                Some(s) => format!("{}/{}", base_out, s),
                None => base_out,
            })
        })
        .collect();
    parts.ok().map(|v| v.join(","))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daily_no_dow_change() {
        assert_eq!(to_quartz_full("0 9 * * *"), "0 0 9 * * *");
    }

    #[test]
    fn weekday_range_shifts() {
        // vixie 1-5 = Mon-Fri → Quartz 2-6 = Mon-Fri
        assert_eq!(to_quartz_full("0 9 * * 1-5"), "0 0 9 * * 2-6");
    }

    #[test]
    fn weekday_list_shifts() {
        // vixie 1,3,5 = Mon/Wed/Fri → Quartz 2,4,6 = Mon/Wed/Fri
        assert_eq!(to_quartz_full("30 6 * * 1,3,5"), "0 30 6 * * 2,4,6");
    }

    #[test]
    fn sunday_zero_and_seven_both_map_to_one() {
        assert_eq!(to_quartz_full("0 9 * * 0"), "0 0 9 * * 1");
        assert_eq!(to_quartz_full("0 9 * * 7"), "0 0 9 * * 1");
    }

    #[test]
    fn saturday_six_maps_to_seven() {
        assert_eq!(to_quartz_full("0 9 * * 6"), "0 0 9 * * 7");
    }

    #[test]
    fn star_slash_step_untouched() {
        assert_eq!(to_quartz_full("0 9 * * */2"), "0 0 9 * * */2");
    }

    #[test]
    fn value_slash_step_maps_base_only() {
        // vixie 3/2 = Wed,Fri,Sun → Quartz 4/2 = Wed,Fri,Sun
        assert_eq!(to_quartz_full("0 9 * * 3/2"), "0 0 9 * * 4/2");
    }

    #[test]
    fn range_with_step() {
        assert_eq!(to_quartz_full("0 9 * * 1-5/2"), "0 0 9 * * 2-6/2");
    }

    #[test]
    fn mixed_list_with_range() {
        assert_eq!(to_quartz_full("0 9 * * 1-5,0"), "0 0 9 * * 2-6,1");
    }

    #[test]
    fn wrong_field_count_passes_through() {
        // 让 cron crate 报错，不在 helper 层拦
        assert_eq!(to_quartz_full("0 9 * *"), "0 0 9 * *");
    }

    #[test]
    fn dow_field_star_untouched() {
        assert_eq!(to_quartz_full("*/15 * * * *"), "0 */15 * * * *");
    }

    #[test]
    fn systemd_dow_single_number() {
        assert_eq!(vixie_dow_to_systemd("1").as_deref(), Some("Mon"));
        assert_eq!(vixie_dow_to_systemd("0").as_deref(), Some("Sun"));
        assert_eq!(vixie_dow_to_systemd("7").as_deref(), Some("Sun"));
        assert_eq!(vixie_dow_to_systemd("6").as_deref(), Some("Sat"));
    }

    #[test]
    fn systemd_dow_range_and_list() {
        // 之前只硬编码 1-5 → Mon..Fri；2-4、3-5 等被原样透传导致 systemd 拒识
        assert_eq!(vixie_dow_to_systemd("1-5").as_deref(), Some("Mon..Fri"));
        assert_eq!(vixie_dow_to_systemd("2-4").as_deref(), Some("Tue..Thu"));
        assert_eq!(vixie_dow_to_systemd("1,3,5").as_deref(), Some("Mon,Wed,Fri"));
        assert_eq!(vixie_dow_to_systemd("0,6").as_deref(), Some("Sun,Sat"));
    }

    #[test]
    fn systemd_dow_step_and_names() {
        assert_eq!(vixie_dow_to_systemd("1-5/2").as_deref(), Some("Mon..Fri/2"));
        assert_eq!(vixie_dow_to_systemd("mon-fri").as_deref(), Some("Mon..Fri"));
        assert_eq!(vixie_dow_to_systemd("MON,WED,FRI").as_deref(), Some("Mon,Wed,Fri"));
    }

    #[test]
    fn systemd_dow_invalid_returns_none() {
        assert_eq!(vixie_dow_to_systemd("8"), None);
        assert_eq!(vixie_dow_to_systemd("xyz"), None);
    }

    #[test]
    fn named_dow_passes_through_and_stays_correct() {
        // cron 0.15 命名别名 MON=2、TUE=3…SUN=1 恰好等价我们的 Quartz 数字，
        // 所以 helper 对命名 token 透传是对的。此测试是回归护栏：若上游 crate 把
        // MON 改回 vixie 1，本测试会失败，提示同步 helper 里的命名映射
        use chrono::{Datelike, TimeZone, Weekday};
        use cron::Schedule;
        use std::str::FromStr;
        let quartz = to_quartz_full("0 9 * * MON");
        let sched = Schedule::from_str(&quartz).unwrap();
        let after = chrono::Local.with_ymd_and_hms(2024, 1, 7, 12, 0, 0).unwrap(); // Sun
        let next = sched.after(&after).next().unwrap();
        assert_eq!(next.weekday(), Weekday::Mon);

        let quartz = to_quartz_full("0 9 * * MON-FRI");
        let sched = Schedule::from_str(&quartz).unwrap();
        let mut days = std::collections::HashSet::new();
        for dt in sched.after(&after).take(5) {
            days.insert(dt.weekday());
        }
        assert!(days.contains(&Weekday::Mon));
        assert!(days.contains(&Weekday::Fri));
        assert!(!days.contains(&Weekday::Sat));
        assert!(!days.contains(&Weekday::Sun));
    }

    #[test]
    fn parses_with_cron_crate() {
        // 端到端：转换结果能被 cron crate 正确解析并产生符合 vixie 语义的下一次触发
        use chrono::{Datelike, TimeZone, Weekday};
        use cron::Schedule;
        use std::str::FromStr;
        // 「每周一 9 点」vixie 写法
        let quartz = to_quartz_full("0 9 * * 1");
        let sched = Schedule::from_str(&quartz).unwrap();
        // 从任意周日之后采样，下一次应落在周一
        let after = chrono::Local.with_ymd_and_hms(2024, 1, 7, 12, 0, 0).unwrap(); // Sun
        let next = sched.after(&after).next().unwrap();
        assert_eq!(next.weekday(), Weekday::Mon);
    }
}
