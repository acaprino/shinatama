//! RFC 3339 UTC timestamps without a date-time dependency.
use std::time::{SystemTime, UNIX_EPOCH};

/// `YYYY-MM-DDTHH:MM:SSZ` for a Unix time in seconds (proleptic Gregorian, UTC).
pub fn rfc3339_utc(seconds: u64) -> String {
    let days = (seconds / 86_400) as i64;
    let rem = seconds % 86_400;
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

pub fn now_rfc3339() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    rfc3339_utc(seconds)
}

#[cfg(test)]
mod tests {
    #[test]
    fn known_instants() {
        assert_eq!(super::rfc3339_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(super::rfc3339_utc(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(super::rfc3339_utc(1_793_750_400), "2026-11-04T00:00:00Z");
    }
}
