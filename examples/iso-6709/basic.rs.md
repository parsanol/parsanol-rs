# ISO 6709 Geographic Coordinate Parser - Rust Implementation

## How to Run

```bash
cargo run --example iso-6709/basic --no-default-features
```

## Code Walkthrough

### Sign Parsing (Hemisphere Indicator)

The first character of each coordinate component indicates the hemisphere:

```rust
.rule("lat_sign", str("+").or(str("-")))
.rule("lon_sign", str("+").or(str("-")))
```

The sign convention follows ISO 6709: `+` = North/East, `-` = South/West. This is fundamental to the format - coordinates are always explicitly signed, never implied by cardinal direction letters (N/S/E/W).

### Decimal Degrees Format

The most common format uses decimal degrees with optional fractional part:

```rust
.rule("decimal_deg", seq(vec![
    dynamic(re("[0-9]{1,2}")), // degrees
    dynamic(seq(vec![
        dynamic(str(".")),
        dynamic(re("[0-9]+")), // fraction
    ]).optional()),
]))
```

Latitude uses 1-2 digits (range 0-90), while longitude uses 1-3 digits (range 0-180). The fractional part is optional but provides precision.

### Coordinate Structure

The complete coordinate combines all components in sequence:

```rust
.rule("coordinate", seq(vec![
    // Latitude: ±DD.DDDD
    dynamic(str("+").or(str("-"))),
    dynamic(re("[0-9]{1,2}(\\.[0-9]+)?")),
    // Longitude: ±DDD.DDDD
    dynamic(str("+").or(str("-"))),
    dynamic(re("[0-9]{1,3}(\\.[0-9]+)?")),
    // Optional altitude
    dynamic(altitude_parser.optional()),
    // Optional CRS
    dynamic(crs_parser.optional()),
]))
```

The order is fixed: latitude, longitude, altitude (optional), CRS (optional).

### Output Transformation

The parsed string is transformed into a typed Rust struct:

```rust
pub struct GeoCoordinate {
    pub latitude: f64,
    pub longitude: f64,
    pub altitude: Option<f64>,
    pub crs: Option<String>,
}
```

Using `f64` provides ~15 decimal digits of precision, sufficient for sub-millimeter accuracy. `Option<T>` handles optional fields.

## Output Types

The parser produces a `GeoCoordinate` struct that serializes to JSON:

```json
{
  "latitude": 48.8584,
  "longitude": 2.2945,
  "altitude": 330.0,
  "crs": "WGS_84"
}
```

## Design Decisions

### Why Manual Post-Parse Transformation?

The grammar validates structure but semantic validation (e.g., latitude ≤ 90°) happens in `parse_iso6709()` after parsing. This separation allows clear error messages and flexible output types.

### Sexagesimal Conversion

The `dms_to_decimal()` helper converts DMS format to decimal degrees:

```rust
pub fn dms_to_decimal(degrees: f64, minutes: f64, seconds: f64, sign: f64) -> f64 {
    sign * (degrees + minutes / 60.0 + seconds / 3600.0)
}
```

Example: 40°41'21.84" → 40.6894°

## ISO 6709 Format Reference

```
Basic:          ±DD.DDDD±DDD.DDDD
With altitude:  ±DD.DDDD±DDD.DDDD±AAA.A
With CRS:       ±DD.DDDD±DDD.DDDDCRS_name/
Sexagesimal:    ±DD MM SS.ss±DDD MM SS.ss
```

## Famous Locations

| Location | ISO 6709 |
|----------|----------|
| Statue of Liberty | `+40.6894-074.0447` |
| Eiffel Tower | `+48.8584+002.2945+330` |
| Mount Everest | `+27.9881+086.9250+8848.86` |
| South Pole | `-90+000` |

## Related Examples

- `url` - URL parsing (maps URLs with coordinates)
- `csv` - CSV with location data
- `json` - GeoJSON parsing
