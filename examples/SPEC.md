# Example Specification

This document defines the standard structure for all Parsanol examples. Every example MUST follow this specification to ensure consistency, maintainability, and proper website generation.

## Core Principles

1. **Single Source of Truth**: All example content lives in the example directory, NOT in the website repository.
2. **Co-location**: Code, documentation, diagrams, and metadata are all in the same directory.
3. **Separation of Concerns**: Format-level info (shared) goes in JSON; implementation-specific info goes in Markdown.
4. **Website as Consumer**: The website generator READS from example directories, it does not store content.

## Directory Structure

```
parsanol-rs/examples/{example-id}/
├── basic.rs              # Primary Rust implementation (required)
├── basic.rs.md           # Rust-specific documentation (required)
├── diagram.svg           # Visual diagram (optional but recommended)
└── example.json          # Format-level metadata (required)

parsanol-ruby/example/{example-id}/
├── basic.rb              # Primary Ruby implementation (if exists)
├── basic.rb.md           # Ruby-specific documentation (if code exists)
└── (no example.json - reads from parsanol-rs)
```

## File Responsibilities

### example.json - Format-Level Metadata

Contains information about the **FORMAT** being parsed, not the code. This is shared across all language implementations.

**Required fields:**

| Field | Type | Description |
|-------|------|-------------|
| `id` | string | Unique identifier (kebab-case, matches directory name) |
| `title` | string | Human-readable title |
| `description` | string | Brief description (1-2 sentences) |
| `category` | string | One of the defined categories |
| `difficulty` | string | "beginner", "intermediate", or "advanced" |

**Optional but recommended fields:**

| Field | Type | Description |
|-------|------|-------------|
| `tags` | string[] | Searchable tags |
| `concepts` | string[] | Key parsing concepts demonstrated |
| `motivation` | object | Why parse this format? |
| `motivation.why` | string | Explanation of use cases |
| `motivation.useCases` | string[] | List of specific use cases |
| `inputFormat` | object | Description of valid inputs |
| `inputFormat.description` | string | Format description |
| `inputFormat.syntax` | string | Syntax notation (like regex pattern) |
| `inputFormat.examples` | array | Input examples |
| `inputFormat.examples[].input` | string | The input string |
| `inputFormat.examples[].description` | string | What this input represents |
| `inputFormat.examples[].valid` | boolean | Is this valid input? |
| `outputFormat` | object | Description of output |
| `outputFormat.description` | string | What the parser produces |
| `outputFormat.structure` | object | Field definitions (conceptual) |
| `related` | string[] | Related example IDs |
| `implementations` | object | Map of language implementations |

**Example:**

```json
{
  "id": "iso-6709",
  "title": "ISO 6709 Geographic Coordinate Parser",
  "description": "Parse ISO 6709 geographic point locations (latitude, longitude, and optional altitude).",
  "category": "data-formats",
  "tags": ["geo", "coordinates", "latitude", "longitude", "gps"],
  "difficulty": "intermediate",
  "concepts": [
    "sign convention",
    "latitude/longitude ranges",
    "optional components"
  ],

  "motivation": {
    "why": "ISO 6709 is the international standard for representing geographic point locations.",
    "useCases": [
      "GPS navigation applications",
      "Photo geotagging",
      "Location APIs",
      "GIS databases"
    ]
  },

  "inputFormat": {
    "description": "Signed decimal or sexagesimal coordinates with optional altitude and CRS.",
    "syntax": "±DD.DDDD±DDD.DDDD[±AAA.A][CRScode/]",
    "examples": [
      {
        "input": "+40.6894-074.0447",
        "description": "Statue of Liberty (decimal degrees)",
        "valid": true
      },
      {
        "input": "+95-074",
        "description": "Invalid - latitude exceeds 90°",
        "valid": false
      }
    ]
  },

  "outputFormat": {
    "description": "Structured coordinate object with latitude, longitude, and optional fields.",
    "structure": {
      "latitude": { "description": "Decimal degrees, positive=North, negative=South" },
      "longitude": { "description": "Decimal degrees, positive=East, negative=West" },
      "altitude": { "description": "Optional altitude in meters" },
      "crs": { "description": "Optional coordinate reference system" }
    }
  },

  "related": ["url", "csv", "json"],
  "implementations": {
    "rust": { "basic": "basic.rs" }
  }
}
```

### basic.rs.md - Rust Implementation Documentation

Contains information about **THIS RUST CODE**, not the format. This is specific to the Rust implementation.

**Required sections:**

1. **How to Run** - Command to execute the example
2. **Code Walkthrough** - Explanation of how the code maps to the format
3. **Output Types** - The Rust types produced (structs, enums)

**Optional sections:**

4. **Design Decisions** - Why certain choices were made
5. **Performance Notes** - Any performance considerations

**Template:**

```markdown
# {Example Title} - Rust Implementation

## How to Run

```bash
cargo run --example {example-id}/basic --no-default-features
```

## Code Walkthrough

### {Component 1 Name}

[Explain what this part of the code does and how it maps to the format]

```rust
// Relevant code snippet
```

[Explanation continues...]

### {Component 2 Name}

[Repeat pattern for each major component]

## Output Types

The parser produces the following Rust types:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct {TypeName} {
    pub field1: f64,
    pub field2: Option<String>,
    // ...
}
```

## Design Decisions

[Optional: Explain any non-obvious choices made in the implementation]
```

### basic.rb.md - Ruby Implementation Documentation

Same structure as `basic.rs.md` but for Ruby code.

**Template:**

```markdown
# {Example Title} - Ruby Implementation

## How to Run

```bash
cd parsanol-ruby
ruby example/{example-id}/basic.rb
```

## Code Walkthrough

### {Component 1 Name}

[Explain what this part of the code does and how it maps to the format]

```ruby
# Relevant code snippet
```

## Output Types

The parser produces Ruby objects:

```ruby
# Example output structure
{ latitude: 40.6894, longitude: -74.0447, altitude: nil, crs: nil }
```

## Design Decisions

[Optional: Explain Ruby-specific choices]
```

### diagram.svg - Visual Diagram

An SVG diagram that visualizes:
- The parsing grammar structure
- Or the input/output transformation
- Or the key concepts

**Guidelines:**
- Keep it simple and readable
- Use consistent colors with the website theme
- Max width: 800px
- Save as optimized SVG

## Categories

Examples are organized into these categories:

| ID | Title | Description |
|----|-------|-------------|
| `expression-parsers` | Expression Parsers | Math/logic expressions with precedence |
| `data-formats` | Data Formats | JSON, CSV, XML, YAML, TOML, INI |
| `urls-network` | URLs & Network | URLs, emails, IP addresses |
| `code-template` | Code & Templates | ERB, S-expressions, mini-languages |
| `text-processing` | Text Processing | Sentences, strings, comments |
| `error-handling` | Error Handling | Rich error reporting techniques |
| `conceptual` | Conceptual Examples | Demonstrating specific parser concepts |

## Difficulty Levels

| Level | Description |
|-------|-------------|
| `beginner` | Simple format, clear structure, good for learning |
| `intermediate` | Multiple components, some complexity |
| `advanced` | Complex format, advanced techniques required |

## Content Guidelines

### What Goes in JSON vs Markdown

| Content Type | Location | Reason |
|-------------|----------|--------|
| Why parse this format | JSON | Shared across implementations |
| Valid input examples | JSON | Shared across implementations |
| Conceptual output structure | JSON | Shared across implementations |
| Code walkthrough | Markdown | Implementation-specific |
| Rust/Python/Ruby types | Markdown | Language-specific |
| Running instructions | Markdown | Environment-specific |
| Design decisions | Markdown | Implementation-specific |

### Writing Style

1. **Be Specific**: "Matches 1-2 digit degree values" not "Matches numbers"
2. **Show Examples**: Every concept should have a concrete example
3. **Explain Why**: Not just what the code does, but why it's done that way
4. **Link Concepts**: Connect code patterns to format semantics

## Website Generation

The website generator (`parsanol.github.io/scripts/generate-examples.cjs`) reads:

1. `example.json` for format-level sections (motivation, input format, output format)
2. `basic.rs` for the source code tab
3. `basic.rb` for the Ruby source code tab (if exists)
4. `diagram.svg` for the visual diagram (copies to website)

The generator does NOT:
- Store any example content itself
- Require manual file copying
- Maintain duplicate metadata

## Verification Checklist

Before submitting an example, verify:

- [ ] Directory name matches `id` in kebab-case
- [ ] `example.json` has all required fields
- [ ] `basic.rs.md` has How to Run, Code Walkthrough, Output Types
- [ ] `diagram.svg` exists (or deliberately omitted)
- [ ] Input examples include both valid and invalid cases
- [ ] Code walkthrough explains each major component
- [ ] Output types are documented with field descriptions
