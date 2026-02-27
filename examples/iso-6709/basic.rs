//! ISO 6709 Geographic Coordinate Parser Example
//!
//! This example demonstrates parsing ISO 6709 geographic point locations
//! (latitude/longitude with optional altitude).
//!
//! ISO 6709 is the standard for geographic point locations. Format:
//! - Latitude/Longitude: +40.6894-074.0447 (Statue of Liberty)
//! - With altitude: +40.6894-074.0447+93.0CRSWGS_84/
//! - Sexagesimal (DMS): +40 41 21.84-074 02 40.92
//!
//! Run with: cargo run --example iso-6709/basic --no-default-features

use parsanol::portable::{
    parser_dsl::{choice, dynamic, re, seq, str, GrammarBuilder, ParsletExt},
    AstArena, Grammar, PortableParser,
};
use serde::{Deserialize, Serialize};

/// Build ISO 6709 grammar
fn build_iso6709_grammar() -> Grammar {
    GrammarBuilder::new()
        // Sign: + for N/E, - for S/W
        .rule("lat_sign", str("+").or(str("-")))
        .rule("lon_sign", str("+").or(str("-")))
        // Decimal degrees
        .rule(
            "decimal_deg",
            seq(vec![
                dynamic(re("[0-9]{1,2}")), // degrees
                dynamic(
                    seq(vec![
                        dynamic(str(".")),
                        dynamic(re("[0-9]+")), // fraction
                    ])
                    .optional(),
                ),
            ]),
        )
        // Sexagesimal (degrees, minutes, seconds)
        .rule(
            "sexagesimal",
            seq(vec![
                dynamic(re("[0-9]{1,2}")), // degrees
                dynamic(
                    seq(vec![
                        dynamic(str(" ")),
                        dynamic(re("[0-9]{1,2}")), // minutes
                        dynamic(
                            seq(vec![
                                dynamic(str(" ")),
                                dynamic(re("[0-9]{1,2}")), // seconds
                                dynamic(
                                    seq(vec![
                                        dynamic(str(".")),
                                        dynamic(re("[0-9]+")), // sec fraction
                                    ])
                                    .optional(),
                                ),
                            ])
                            .optional(),
                        ),
                    ])
                    .optional(),
                ),
            ]),
        )
        // Latitude: -90 to +90
        .rule(
            "latitude",
            seq(vec![
                dynamic(str("+").or(str("-"))),
                dynamic(choice(vec![
                    // Decimal degrees: +DD.DDDD or +DD.DD
                    dynamic(re("[0-9]{1,2}(\\.[0-9]+)?")),
                    // Sexagesimal: +DD MM SS.ss
                    dynamic(re("[0-9]{1,2}( [0-9]{1,2}){0,2}(\\.[0-9]+)?")),
                ])),
            ]),
        )
        // Longitude: -180 to +180
        .rule(
            "longitude",
            seq(vec![
                dynamic(str("+").or(str("-"))),
                dynamic(choice(vec![
                    // Decimal degrees: +DDD.DDDD
                    dynamic(re("[0-9]{1,3}(\\.[0-9]+)?")),
                    // Sexagesimal: +DDD MM SS.ss
                    dynamic(re("[0-9]{1,3}( [0-9]{1,2}){0,2}(\\.[0-9]+)?")),
                ])),
            ]),
        )
        // Altitude (optional)
        .rule(
            "altitude",
            seq(vec![
                dynamic(str("+").or(str("-"))),
                dynamic(re("[0-9]+")),
                dynamic(seq(vec![dynamic(str(".")), dynamic(re("[0-9]+"))]).optional()),
            ]),
        )
        // Coordinate Reference System (optional)
        .rule(
            "crs",
            seq(vec![dynamic(str("CRS")), dynamic(re("[A-Z0-9_]+"))]),
        )
        // Complete coordinate
        .rule(
            "coordinate",
            seq(vec![
                // Latitude
                dynamic(str("+").or(str("-"))),
                dynamic(re("[0-9]{1,2}(\\.[0-9]+)?")),
                // Longitude
                dynamic(str("+").or(str("-"))),
                dynamic(re("[0-9]{1,3}(\\.[0-9]+)?")),
                // Optional altitude
                dynamic(
                    seq(vec![
                        dynamic(str("+").or(str("-"))),
                        dynamic(re("[0-9]+(\\.[0-9]+)?")),
                    ])
                    .optional(),
                ),
                // Optional CRS
                dynamic(
                    seq(vec![
                        dynamic(str("CRS")),
                        dynamic(re("[A-Z0-9_]+")),
                        dynamic(str("/")),
                    ])
                    .optional(),
                ),
            ]),
        )
        .build()
}

/// Parsed ISO 6709 coordinate
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeoCoordinate {
    pub latitude: f64,
    pub longitude: f64,
    pub altitude: Option<f64>,
    pub crs: Option<String>,
}

/// Parse ISO 6709 coordinate string
pub fn parse_iso6709(input: &str) -> Result<String, String> {
    let grammar = build_iso6709_grammar();
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    let _ast = parser
        .parse()
        .map_err(|e| format!("Parse error: {:?}", e))?;

    // Parse manually for structured output
    let input = input.trim();

    // Remove trailing slash if present
    let input = input.strip_suffix('/').unwrap_or(input);

    let mut chars = input.chars().peekable();
    let mut coord = GeoCoordinate {
        latitude: 0.0,
        longitude: 0.0,
        altitude: None,
        crs: None,
    };

    // Parse latitude
    let lat_sign = match chars.next() {
        Some('+') => 1.0,
        Some('-') => -1.0,
        _ => return Err("Expected +/- at start".into()),
    };

    let mut lat_str = String::new();
    while let Some(c) = chars.peek() {
        if *c == '+' || *c == '-' {
            break;
        }
        lat_str.push(chars.next().unwrap());
    }
    coord.latitude = lat_str
        .trim()
        .parse::<f64>()
        .map(|v| v * lat_sign)
        .map_err(|_| format!("Invalid latitude: {}", lat_str))?;

    // Parse longitude
    let lon_sign = match chars.next() {
        Some('+') => 1.0,
        Some('-') => -1.0,
        _ => return Err("Expected +/- before longitude".into()),
    };

    let mut lon_str = String::new();
    while let Some(c) = chars.peek() {
        if *c == '+' || *c == '-' || *c == 'C' {
            break;
        }
        lon_str.push(chars.next().unwrap());
    }
    coord.longitude = lon_str
        .trim()
        .parse::<f64>()
        .map(|v| v * lon_sign)
        .map_err(|_| format!("Invalid longitude: {}", lon_str))?;

    // Parse optional altitude
    if let Some(&c) = chars.peek() {
        if c == '+' || c == '-' {
            let alt_sign = match chars.next() {
                Some('+') => 1.0,
                Some('-') => -1.0,
                _ => unreachable!(),
            };

            let mut alt_str = String::new();
            while let Some(c) = chars.peek() {
                if *c == 'C' {
                    break;
                }
                alt_str.push(chars.next().unwrap());
            }
            coord.altitude = Some(
                alt_str
                    .trim()
                    .parse::<f64>()
                    .map(|v| v * alt_sign)
                    .map_err(|_| format!("Invalid altitude: {}", alt_str))?,
            );
        }
    }

    // Parse optional CRS
    let remaining: String = chars.collect();
    if let Some(stripped) = remaining.strip_prefix("CRS") {
        coord.crs = Some(stripped.to_string());
    }

    serde_json::to_string_pretty(&coord).map_err(|e| e.to_string())
}

/// Convert sexagesimal (DMS) to decimal degrees
pub fn dms_to_decimal(degrees: f64, minutes: f64, seconds: f64, sign: f64) -> f64 {
    sign * (degrees + minutes / 60.0 + seconds / 3600.0)
}

fn main() {
    println!("ISO 6709 Geographic Coordinate Parser");
    println!("======================================\n");

    let examples = [
        // Basic coordinates
        ("+40.6894-074.0447", "Statue of Liberty (decimal)"),
        ("+48.8584+002.2945", "Eiffel Tower"),
        ("-90+000", "South Pole"),
        ("+00+000", "Null Island (0,0)"),
        // With altitude
        ("+40.6894-074.0447+93.0", "Statue of Liberty with altitude"),
        // With CRS
        ("+48.8584+002.2945+330CRSWGS_84/", "Eiffel Tower with CRS"),
        // High altitude
        ("+27.9881+086.9250+8848.86CRSWGS_84/", "Mount Everest"),
        // Sexagesimal (DMS) format
        ("+40 41 21.84-074 02 40.92", "Statue of Liberty (DMS)"),
    ];

    for (input, description) in examples {
        println!("Input: {} ({})", input, description);
        match parse_iso6709(input) {
            Ok(json) => println!("Output:\n{}\n", json),
            Err(e) => println!("Error: {}\n", e),
        }
    }

    println!("ISO 6709 Format Reference:");
    println!("--------------------------");
    println!("  ±DD.DDDD±DDD.DDDD         Basic (lat, lon)");
    println!("  ±DD.DDDD±DDD.DDDD±AAA.A   With altitude");
    println!("  ±DD MM SS.ss±DDD MM SS.ss Sexagesimal (DMS)");
    println!("  ...CRSEPSG_CODE/          With coordinate reference system");
}
