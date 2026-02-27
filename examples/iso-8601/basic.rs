//! ISO 8601 Date/Time Parser Example
//!
//! This example demonstrates parsing ISO 8601 date, time, and datetime strings.
//! ISO 8601 is the international standard for date and time representations.
//!
//! Supported formats:
//! - Calendar dates: 2024-01-15, 20240115
//! - Week dates: 2024-W02-1 (year-week-weekday)
//! - Ordinal dates: 2024-015 (year-day of year)
//! - Times: 10:30:00, 10:30:00.123
//! - Timezones: Z, +02:00, -05:00
//! - Date-times: 2024-01-15T10:30:00Z
//! - Durations: P1Y2M3DT4H5M6S
//!
//! Run with: cargo run --example iso-8601/basic --no-default-features

use parsanol::portable::{
    parser_dsl::{choice, dynamic, re, seq, str, GrammarBuilder, ParsletExt},
    AstArena, Grammar, PortableParser,
};
use serde::{Deserialize, Serialize};

/// Build ISO 8601 grammar
fn build_iso8601_grammar() -> Grammar {
    GrammarBuilder::new()
        // Date components
        .rule("year", re("[0-9]{4}"))
        .rule("month", re("[0-9]{2}"))
        .rule("day", re("[0-9]{2}"))
        .rule("date_separator", str("-").optional())
        // Calendar date: YYYY-MM-DD or YYYYMMDD
        .rule(
            "calendar_date",
            seq(vec![
                dynamic(re("[0-9]{4}")),
                dynamic(str("-").optional()),
                dynamic(re("[0-9]{2}")),
                dynamic(str("-").optional()),
                dynamic(re("[0-9]{2}")),
            ]),
        )
        // Week date: YYYY-Www-D
        .rule(
            "week_date",
            seq(vec![
                dynamic(re("[0-9]{4}")),
                dynamic(str("-W")),
                dynamic(re("[0-9]{2}")), // week
                dynamic(str("-")),
                dynamic(re("[1-7]")), // weekday (1=Monday, 7=Sunday)
            ]),
        )
        // Ordinal date: YYYY-DDD
        .rule(
            "ordinal_date",
            seq(vec![
                dynamic(re("[0-9]{4}")),
                dynamic(str("-")),
                dynamic(re("[0-9]{3}")), // day of year
            ]),
        )
        // Time components
        .rule("hour", re("[0-9]{2}"))
        .rule("minute", re("[0-9]{2}"))
        .rule("second", re("[0-9]{2}"))
        .rule(
            "fraction",
            seq(vec![dynamic(str(".")), dynamic(re("[0-9]+"))]),
        )
        // Time: HH:MM:SS or HHMMSS
        .rule(
            "time_basic",
            seq(vec![
                dynamic(re("[0-9]{2}")),
                dynamic(str(":").optional()),
                dynamic(re("[0-9]{2}")),
                dynamic(str(":").optional()),
                dynamic(re("[0-9]{2}")),
                dynamic(seq(vec![dynamic(str(".")), dynamic(re("[0-9]+"))]).optional()),
            ]),
        )
        // Timezone
        .rule("utc_designator", str("Z"))
        .rule(
            "tz_offset",
            seq(vec![
                dynamic(str("+").or(str("-"))),
                dynamic(re("[0-9]{2}")),
                dynamic(str(":").optional()),
                dynamic(re("[0-9]{2}").optional()),
            ]),
        )
        .rule(
            "timezone",
            choice(vec![
                dynamic(str("Z")),
                dynamic(seq(vec![
                    dynamic(str("+").or(str("-"))),
                    dynamic(re("[0-9]{2}")),
                    dynamic(str(":").optional()),
                    dynamic(re("[0-9]{2}").optional()),
                ])),
            ]),
        )
        // Combined date-time
        .rule(
            "datetime",
            seq(vec![
                dynamic(choice(vec![
                    dynamic(re("[0-9]{4}-[0-9]{2}-[0-9]{2}")), // calendar
                    dynamic(re("[0-9]{4}-W[0-9]{2}-[1-7]")),   // week
                    dynamic(re("[0-9]{4}-[0-9]{3}")),          // ordinal
                ])),
                dynamic(str("T").or(str(" "))),
                dynamic(re("[0-9]{2}(:[0-9]{2}){1,2}")), // time
                dynamic(re("[.,][0-9]+").optional()),    // fraction
                dynamic(
                    choice(vec![
                        dynamic(str("Z")),
                        dynamic(re("[+-][0-9]{2}(:?[0-9]{2})?")),
                    ])
                    .optional(),
                ),
            ]),
        )
        // Duration: PnYnMnDTnHnMnS
        .rule(
            "duration",
            seq(vec![
                dynamic(str("P")),
                dynamic(
                    seq(vec![
                        dynamic(re("[0-9]+Y").optional()),
                        dynamic(re("[0-9]+M").optional()),
                        dynamic(re("[0-9]+D").optional()),
                        dynamic(
                            seq(vec![
                                dynamic(str("T")),
                                dynamic(re("[0-9]+H").optional()),
                                dynamic(re("[0-9]+M").optional()),
                                dynamic(re("[0-9]+S").optional()),
                            ])
                            .optional(),
                        ),
                    ])
                    .optional(),
                ),
            ]),
        )
        // Root: try datetime, then date, then time, then duration
        .rule(
            "iso_value",
            choice(vec![
                dynamic(re("[0-9]{4}(-[0-9]{2}){2}[T ][0-9]{2}:[0-9]{2}")), // datetime
                dynamic(re("[0-9]{4}-W[0-9]{2}-[1-7]")),                    // week date
                dynamic(re("[0-9]{4}-[0-9]{3}")),                           // ordinal date
                dynamic(re("[0-9]{4}(-[0-9]{2}){0,2}")),                    // calendar date
                dynamic(re("[0-9]{2}:[0-9]{2}(:[0-9]{2})?")),               // time
                dynamic(re("P[0-9YMDTHMS]+")),                              // duration
            ]),
        )
        .build()
}

/// Parsed ISO 8601 date components
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IsoDate {
    pub year: i32,
    pub month: Option<u32>,
    pub day: Option<u32>,
    pub week: Option<u32>,
    pub weekday: Option<u32>,
    pub ordinal_day: Option<u32>,
}

/// Parsed ISO 8601 time components
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IsoTime {
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
    pub fraction: Option<f64>,
    pub timezone: Option<String>,
}

/// Parsed ISO 8601 datetime
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IsoDateTime {
    pub date: IsoDate,
    pub time: Option<IsoTime>,
}

/// Parsed ISO 8601 duration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IsoDuration {
    pub years: Option<u32>,
    pub months: Option<u32>,
    pub days: Option<u32>,
    pub hours: Option<u32>,
    pub minutes: Option<u32>,
    pub seconds: Option<u32>,
}

/// Parse ISO 8601 datetime string
pub fn parse_iso8601(input: &str) -> Result<String, String> {
    let grammar = build_iso8601_grammar();
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    let _ast = parser
        .parse()
        .map_err(|e| format!("Parse error: {:?}", e))?;

    // Parse manually for structured output
    let input = input.trim();

    // Try duration first
    if input.starts_with('P') {
        let duration = parse_duration(input)?;
        return serde_json::to_string_pretty(&duration).map_err(|e| e.to_string());
    }

    // Try datetime
    if let Some(sep_pos) = input.find(['T', ' ']) {
        let date_str = &input[..sep_pos];
        let time_str = &input[sep_pos + 1..];

        let date = parse_date(date_str)?;
        let time = parse_time(time_str)?;

        let datetime = IsoDateTime {
            date,
            time: Some(time),
        };
        return serde_json::to_string_pretty(&datetime).map_err(|e| e.to_string());
    }

    // Try date only
    if input.contains('-') || input.len() == 8 {
        let date = parse_date(input)?;
        return serde_json::to_string_pretty(&date).map_err(|e| e.to_string());
    }

    // Try time only
    if input.contains(':') {
        let time = parse_time(input)?;
        return serde_json::to_string_pretty(&time).map_err(|e| e.to_string());
    }

    Err(format!("Unrecognized ISO 8601 format: {}", input))
}

/// Parse date portion
fn parse_date(input: &str) -> Result<IsoDate, String> {
    // Week date: YYYY-Www-D
    if input.contains("-W") {
        let parts: Vec<&str> = input.split("-W").collect();
        if parts.len() != 2 {
            return Err("Invalid week date format".into());
        }
        let year: i32 = parts[0].parse().map_err(|_| "Invalid year")?;
        let week_weekday: Vec<&str> = parts[1].split('-').collect();
        let week: u32 = week_weekday[0].parse().map_err(|_| "Invalid week")?;
        let weekday: u32 = week_weekday
            .get(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(1);

        return Ok(IsoDate {
            year,
            month: None,
            day: None,
            week: Some(week),
            weekday: Some(weekday),
            ordinal_day: None,
        });
    }

    // Ordinal date: YYYY-DDD
    let parts: Vec<&str> = input.split('-').collect();
    if parts.len() == 2 && parts[1].len() == 3 {
        let year: i32 = parts[0].parse().map_err(|_| "Invalid year")?;
        let ordinal_day: u32 = parts[1].parse().map_err(|_| "Invalid ordinal day")?;

        return Ok(IsoDate {
            year,
            month: None,
            day: None,
            week: None,
            weekday: None,
            ordinal_day: Some(ordinal_day),
        });
    }

    // Calendar date: YYYY-MM-DD or YYYYMMDD
    let (year, month, day) = if input.contains('-') {
        let parts: Vec<&str> = input.split('-').collect();
        let year: i32 = parts[0].parse().map_err(|_| "Invalid year")?;
        let month: u32 = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(1);
        let day: u32 = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(1);
        (year, month, day)
    } else if input.len() == 8 {
        let year: i32 = input[0..4].parse().map_err(|_| "Invalid year")?;
        let month: u32 = input[4..6].parse().map_err(|_| "Invalid month")?;
        let day: u32 = input[6..8].parse().map_err(|_| "Invalid day")?;
        (year, month, day)
    } else {
        return Err(format!("Invalid date format: {}", input));
    };

    Ok(IsoDate {
        year,
        month: Some(month),
        day: Some(day),
        week: None,
        weekday: None,
        ordinal_day: None,
    })
}

/// Parse time portion
fn parse_time(input: &str) -> Result<IsoTime, String> {
    // Extract timezone
    let (time_part, timezone) = if let Some(pos) = input.rfind(['Z', '+', '-']) {
        if input.chars().nth(pos) == Some('Z') {
            (&input[..pos], Some("Z".to_string()))
        } else if pos > 8 {
            // Ensure it's not the date separator
            let tz = &input[pos..];
            // Validate it looks like a timezone
            if tz.starts_with(['+', '-']) {
                (&input[..pos], Some(tz.to_string()))
            } else {
                (input, None)
            }
        } else {
            (input, None)
        }
    } else {
        (input, None)
    };

    // Parse time components
    let parts: Vec<&str> = time_part.split(':').collect();
    let hour: u32 = parts[0].parse().map_err(|_| "Invalid hour")?;
    let minute: u32 = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);

    // Handle seconds with optional fraction
    let (second, fraction) = if let Some(sec_str) = parts.get(2) {
        if sec_str.contains('.') {
            let sec_parts: Vec<&str> = sec_str.split('.').collect();
            let sec: u32 = sec_parts[0].parse().map_err(|_| "Invalid second")?;
            let frac: f64 = format!("0.{}", sec_parts[1]).parse().unwrap_or(0.0);
            (sec, Some(frac))
        } else {
            (sec_str.parse().map_err(|_| "Invalid second")?, None)
        }
    } else {
        (0, None)
    };

    Ok(IsoTime {
        hour,
        minute,
        second,
        fraction,
        timezone,
    })
}

/// Parse duration
fn parse_duration(input: &str) -> Result<IsoDuration, String> {
    if !input.starts_with('P') {
        return Err("Duration must start with P".into());
    }

    let rest = &input[1..];
    let (date_part, time_part) = if let Some(t_pos) = rest.find('T') {
        (&rest[..t_pos], Some(&rest[t_pos + 1..]))
    } else {
        (rest, None)
    };

    let mut duration = IsoDuration {
        years: None,
        months: None,
        days: None,
        hours: None,
        minutes: None,
        seconds: None,
    };

    // Parse date components
    let mut num_buf = String::new();
    for c in date_part.chars() {
        if c.is_ascii_digit() {
            num_buf.push(c);
        } else {
            let value: u32 = num_buf.parse().unwrap_or(0);
            num_buf.clear();
            match c {
                'Y' => duration.years = Some(value),
                'M' => duration.months = Some(value),
                'D' => duration.days = Some(value),
                _ => {}
            }
        }
    }

    // Parse time components
    if let Some(time) = time_part {
        for c in time.chars() {
            if c.is_ascii_digit() {
                num_buf.push(c);
            } else {
                let value: u32 = num_buf.parse().unwrap_or(0);
                num_buf.clear();
                match c {
                    'H' => duration.hours = Some(value),
                    'M' => duration.minutes = Some(value),
                    'S' => duration.seconds = Some(value),
                    _ => {}
                }
            }
        }
    }

    Ok(duration)
}

fn main() {
    println!("ISO 8601 Date/Time Parser");
    println!("=========================\n");

    let examples = [
        // Calendar dates
        ("2024-01-15", "Calendar date"),
        ("20240115", "Compact date"),
        // Week dates
        ("2024-W02-1", "Week date (2nd week, Monday)"),
        // Ordinal dates
        ("2024-015", "Ordinal date (15th day of year)"),
        // Times
        ("10:30:00", "Time"),
        ("10:30:00.123", "Time with fraction"),
        // Date-times
        ("2024-01-15T10:30:00Z", "UTC datetime"),
        ("2024-01-15T10:30:00+09:00", "Datetime with timezone"),
        // Durations
        ("P1Y2M3D", "Duration (1 year, 2 months, 3 days)"),
        ("PT30M", "Duration (30 minutes)"),
        ("P1Y2M3DT4H5M6S", "Full duration"),
    ];

    for (input, description) in examples {
        println!("Input: {} ({})", input, description);
        match parse_iso8601(input) {
            Ok(json) => println!("Output:\n{}\n", json),
            Err(e) => println!("Error: {}\n", e),
        }
    }
}
