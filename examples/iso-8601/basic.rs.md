# ISO 8601 Date/Time Parser - Rust Implementation

## How to Run

```bash
cargo run --example iso-8601/basic --no-default-features
```

## Code Walkthrough

### Calendar Date Parsing

Calendar dates are `YYYY-MM-DD` or `YYYYMMDD`:

```rust
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
```

The separator is optional for compact format.

### Week Date Parsing

Week dates use `YYYY-Www-D` format:

```rust
.rule(
    "week_date",
    seq(vec![
        dynamic(re("[0-9]{4}")),
        dynamic(str("-W")),
        dynamic(re("[0-9]{2}")), // week (01-53)
        dynamic(str("-")),
        dynamic(re("[1-7]")),    // weekday (1=Monday, 7=Sunday)
    ]),
)
```

Week 1 contains the year's first Thursday.

### Ordinal Date Parsing

Ordinal dates use day of year:

```rust
.rule(
    "ordinal_date",
    seq(vec![
        dynamic(re("[0-9]{4}")),
        dynamic(str("-")),
        dynamic(re("[0-9]{3}")), // day of year (001-366)
    ]),
)
```

Day 015 means January 15th.

### Time Parsing

Times include optional fractions and timezone:

```rust
.rule(
    "time_basic",
    seq(vec![
        dynamic(re("[0-9]{2}")),
        dynamic(str(":").optional()),
        dynamic(re("[0-9]{2}")),
        dynamic(str(":").optional()),
        dynamic(re("[0-9]{2}")),
        dynamic(/* fraction */).optional(),
    ]),
)
.rule(
    "timezone",
    choice(vec![
        dynamic(str("Z")),
        dynamic(/* offset: +HH:MM or -HH:MM */),
    ]),
)
```

`Z` indicates UTC; offsets are `+HH:MM` or `-HH:MM`.

### Duration Parsing

Durations use `PnYnMnDTnHnMnS` format:

```rust
.rule(
    "duration",
    seq(vec![
        dynamic(str("P")),
        dynamic(seq(vec![
            dynamic(re("[0-9]+Y").optional()),
            dynamic(re("[0-9]+M").optional()),
            dynamic(re("[0-9]+D").optional()),
            dynamic(seq(vec![
                dynamic(str("T")),
                dynamic(re("[0-9]+H").optional()),
                dynamic(re("[0-9]+M").optional()),
                dynamic(re("[0-9]+S").optional()),
            ]).optional()),
        ]).optional()),
    ]),
)
```

`P` starts date portion, `T` starts time portion.

### Date Parsing Implementation

Dates are parsed by detecting format:

```rust
fn parse_date(input: &str) -> Result<IsoDate, String> {
    // Week date: YYYY-Www-D
    if input.contains("-W") {
        let parts: Vec<&str> = input.split("-W").collect();
        let year: i32 = parts[0].parse()?;
        let week_weekday: Vec<&str> = parts[1].split('-').collect();
        let week: u32 = week_weekday[0].parse()?;
        let weekday: u32 = week_weekday.get(1).and_then(|s| s.parse().ok()).unwrap_or(1);
        return Ok(IsoDate { year, week: Some(week), weekday: Some(weekday), ... });
    }

    // Ordinal date: YYYY-DDD
    if parts.len() == 2 && parts[1].len() == 3 {
        return Ok(IsoDate { year, ordinal_day: Some(day_of_year), ... });
    }

    // Calendar date: YYYY-MM-DD
    Ok(IsoDate { year, month: Some(month), day: Some(day), ... })
}
```

## Output Types

```rust
pub struct IsoDate {
    pub year: i32,
    pub month: Option<u32>,
    pub day: Option<u32>,
    pub week: Option<u32>,
    pub weekday: Option<u32>,
    pub ordinal_day: Option<u32>,
}

pub struct IsoTime {
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
    pub fraction: Option<f64>,
    pub timezone: Option<String>,
}

pub struct IsoDateTime {
    pub date: IsoDate,
    pub time: Option<IsoTime>,
}

pub struct IsoDuration {
    pub years: Option<u32>,
    pub months: Option<u32>,
    pub days: Option<u32>,
    pub hours: Option<u32>,
    pub minutes: Option<u32>,
    pub seconds: Option<u32>,
}
```

Optional fields handle the different date/time representations.

## Design Decisions

### Why Optional Fields?

ISO 8601 has multiple representations. Optional fields allow one struct to handle calendar, week, and ordinal dates.

### Why String Timezone?

Full timezone parsing requires a timezone database. String representation allows consumers to interpret as needed.
