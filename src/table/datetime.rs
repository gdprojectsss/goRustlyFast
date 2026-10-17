pub(crate) fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs() as i64)
}

pub(crate) struct Civil {
    pub year: i64,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_index + 2) / 5 + 1) as u32;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    } as u32;
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year.rem_euclid(400);
    let month = i64::from(month);
    let day_of_year =
        (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn local_offset(timestamp: i64) -> i64 {
    let time = timestamp as _;
    let mut broken_down: libc::tm = unsafe { std::mem::zeroed() };
    let result = unsafe { libc::localtime_r(&time, &mut broken_down) };
    if result.is_null() {
        0
    } else {
        broken_down.tm_gmtoff
    }
}

pub(crate) fn to_civil(timestamp: i64, offset: i64) -> Civil {
    let local = timestamp + offset;
    let (year, month, day) = civil_from_days(local.div_euclid(86_400));
    let seconds = local.rem_euclid(86_400);
    Civil {
        year,
        month,
        day,
        hour: (seconds / 3600) as u32,
        minute: (seconds % 3600 / 60) as u32,
        second: (seconds % 60) as u32,
    }
}

pub(crate) fn format_local(timestamp: i64) -> String {
    let civil = to_civil(timestamp, local_offset(timestamp));
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        civil.year, civil.month, civil.day, civil.hour, civil.minute, civil.second
    )
}

pub(crate) fn format_relative(timestamp: i64, now: i64) -> String {
    let age = now - timestamp;
    if age < 0 {
        return format_local(timestamp);
    }
    let (amount, unit) = match age {
        0..60 => return "just now".into(),
        60..3600 => (age / 60, "minute"),
        3600..86_400 => (age / 3600, "hour"),
        86_400..2_592_000 => (age / 86_400, "day"),
        2_592_000..31_536_000 => (age / 2_592_000, "month"),
        _ => (age / 31_536_000, "year"),
    };
    let plural = if amount == 1 { "" } else { "s" };
    format!("{amount} {unit}{plural} ago")
}

pub(crate) fn parse_date(text: &str) -> Option<i64> {
    let (date, time) = match text.split_once([' ', 'T']) {
        Some((date, time)) => (date, Some(time)),
        None => (text, None),
    };
    let mut parts = date.split('-');
    let year = parts.next()?.parse().ok()?;
    let month = parts
        .next()?
        .parse()
        .ok()
        .filter(|month| (1..=12).contains(month))?;
    let day = parts
        .next()?
        .parse()
        .ok()
        .filter(|day| (1..=31).contains(day))?;
    if parts.next().is_some() {
        return None;
    }
    let mut seconds = 0;
    if let Some(time) = time {
        let time = time.trim_end_matches('Z');
        let mut fields = time.split(':');
        let hour: i64 = fields.next()?.parse().ok()?;
        let minute: i64 = fields.next().unwrap_or("0").parse().ok()?;
        let second: f64 = fields.next().unwrap_or("0").parse().ok()?;
        seconds = hour * 3600 + minute * 60 + second as i64;
    }
    let naive = days_from_civil(year, month, day) * 86_400 + seconds;
    Some(naive - local_offset(naive))
}

pub(crate) fn parse_age(text: &str) -> Option<i64> {
    let text = text.trim().to_ascii_lowercase();
    let split = text.find(|character: char| !character.is_ascii_digit() && character != '.')?;
    let (number, unit) = text.split_at(split);
    let number: f64 = number.parse().ok()?;
    let seconds = match unit.trim() {
        "s" | "sec" | "secs" | "second" | "seconds" => 1.0,
        "m" | "min" | "mins" | "minute" | "minutes" => 60.0,
        "h" | "hr" | "hrs" | "hour" | "hours" => 3600.0,
        "d" | "day" | "days" => 86_400.0,
        "w" | "wk" | "week" | "weeks" => 604_800.0,
        "mo" | "month" | "months" => 2_592_000.0,
        "y" | "yr" | "year" | "years" => 31_536_000.0,
        _ => return None,
    };
    Some((number * seconds) as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_round_trip() {
        for days in [-719_468, -1, 0, 1, 10_957, 20_000, 30_000] {
            let (year, month, day) = civil_from_days(days);
            assert_eq!(days_from_civil(year, month, day), days);
        }
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(20_743), (2026, 10, 17));
    }

    #[test]
    fn utc_formatting() {
        let civil = to_civil(1_792_195_200 + 3_723, 0);
        assert_eq!(
            (
                civil.year,
                civil.month,
                civil.day,
                civil.hour,
                civil.minute,
                civil.second
            ),
            (2026, 10, 17, 1, 2, 3)
        );
    }

    #[test]
    fn relative_ages() {
        assert_eq!(format_relative(1000, 1030), "just now");
        assert_eq!(format_relative(0, 7200), "2 hours ago");
        assert_eq!(format_relative(0, 86_400), "1 day ago");
    }

    #[test]
    fn ages_and_dates_parse() {
        assert_eq!(parse_age("2d"), Some(172_800));
        assert_eq!(parse_age("90 min"), Some(5400));
        assert_eq!(parse_age("soon"), None);
        let parsed = parse_date("2026-10-17").unwrap();
        assert_eq!(format_local(parsed), "2026-10-17 00:00:00");
        assert!(parse_date("2026-13-01").is_none());
    }
}
