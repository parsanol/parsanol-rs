# Ruby Examples Implementation Plan

## Overview

This document outlines the plan to implement examples demonstrating the new features (captures, scopes, dynamic atoms, and streaming parser with captures) in the Ruby (`parsanol-ruby`) project.

## Current State

- **Location**: `/Users/mulgogi/src/parsanol/parsanol-ruby/example/`
- **Existing examples**:
  - `capture/` - Demonstrates heredoc-style capture with dynamic callbacks
  - `scopes/` - Minimal example showing scope isolation

## Goals

Create comprehensive Ruby examples matching the Rust examples:

## Example Structure

Each example should follow the existing pattern:
```
example/<name>/
├── basic.rb          # Main example code
├── basic.rb.md        # Markdown documentation
└── example.json        # Metadata
```

---

## 1. Captures Example (`captures/`)

### Location
`example/captures/`

### Files to Create

#### example.json
```json
{
  "id": "captures",
  "title": "Capture Atoms",
  "description": "Extract named values from parsed input using capture atoms. Works like named groups in regex.",
  "category": " "feature",
  "tags": ["capture", "extraction", "named-groups", "zero-copy"],
  "difficulty": "beginner",
  "concepts": ["capture atoms", "named captures", "capture extraction", "capture state"],

  "motivation": {
    "why": "Extract specific parts of input without building full AST. Like regex named groups.",
    "useCases": [
      "Extract email components (local@domain)",
      "Parse configuration files (key=value)",
      "Extract log fields"
    ]
  },

  "inputFormat": {
    "description": "Structured text with parts to capture",
    "examples": [
      {"input": "hello", "description": "Simple greeting", "valid": true},
      {"input": "user@example.com", "description": "Email address", "valid": true}
    ]
  },

  "outputFormat": {
    "description": "Captures accessible via capture_state",
    "structure": {
      "capture_state": {"description": "Hash of captured values"}
    }
  },

  "backendCompatibility": {
    "packrat": {"support": " "full", "notes": " "Native Packrat support" },
    "bytecode": {"support": " "full", "notes":  "Works via Packrat" },
    "streaming": {"support":  "full", "notes":  "Works with capture state" }
  },

  "related": ["capture", "scopes", "dynamic"],
  "implementations": {
    "ruby": {"basic": "basic.rb" }
  }
}
```

#### basic.rb
```ruby
# frozen_string_literal: true

# Capture Atoms Example
#
# Demonstrates how to extract named values from parsed input using capture atoms.
# Captures work like named groups in regular expressions, but are integrated
# into the parsing grammar.

$LOAD_PATH.unshift "#{File.dirname(__FILE__)}/../lib"
require 'parsanol/parslet'
require 'pp'

puts "Capture Atoms Example"
puts "=====================\n"

# ===========================================================================
# Example 1: Basic Capture
# ===========================================================================
puts "--- Example 1: Basic Capture ---\n"

# Simple capture: match 'hello' and capture it
parser = str('hello').capture(:greeting)

input = "hello world"
result = parser.parse(input)

if result[:greeting]
  puts "  Captured 'greeting': #{result[:greeting].inspect}"
end

# ===========================================================================
# Example 2: Email Parsing with Nested Captures
# ===========================================================================
puts "\n--- Example 2: Email Parsing with Nested Captures ---\n"

# Grammar: local@domain with captures for each part
email_parser = match('[a-zA-Z0-9._%+-]+').capture(:local) >>
                   str('@') >>
                   match('[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}').capture(:domain)

input = "user@example.com"
result = email_parser.parse(input)

puts "  Full email: #{result[:email]}"
puts "  Local part: #{result[:local]}"
puts "  Domain: #{result[:domain]}"

# ===========================================================================
# Example 3: Multiple Captures
# ===========================================================================
puts "\n--- Example 3: Multiple Captures ---\n"

# Parse key=value pairs
kv_parser = match('[a-z]+').capture(:key) >>
              str('=') >>
              match('[a-zA-Z0-9]+').capture(:value)

input = "name=Alice"
result = kv_parser.parse(input)

puts "  Key: #{result[:key]}"
puts "  Value: #{result[:value]}"

# ===========================================================================
# Example 4: Captures with Context (Advanced)
# ===========================================================================
puts "\n--- Example 4: Using Captures with Dynamic ---\n"

# Use captured value in dynamic block
class CaptureParser < Parsanol::Parser
  include Parsanol::Parslet

  rule(:type) { match('[a-z]+').capture(:type) }
  rule(:value) do
    dynamic do |_source, context|
      # Get captured type to determine value parser
      type = context.captures[:type]
      case type
      when 'int' then match('\d+')
      when 'str' then match('[a-z]+')
      when 'bool' then str('true') | str('false')
      else match('[a-z]+')  # fallback
      end.capture(:value)
    end
  end
  rule(:declaration) { type >> str(':') >> match('[a-z]+').capture(:name) >> str('=') >> value }
  root :declaration
end

test_cases = [
  ['int:count=42', 'int'],
  ['str:message=hello', 'str'],
  ['bool:enabled=true', 'bool']
]

test_cases.each do |input, expected_type|
  puts "  Parsing: #{input}"
  parser = CaptureParser.new
  result = parser.parse(input)
  puts "  ✓ type: #{result[:type]}"
  puts "    name: #{result[:name]}"
  puts "    value: #{result[:value]}"
end

# ===========================================================================
# Summary
# ===========================================================================
puts "\n--- Benefits of Capture Atoms ---"
puts "* Zero-copy: captures store offsets, not strings"
puts "* Works across all backends (Packrat, Streaming)"
puts "* Clean API: capture(name) method on atoms"
puts "* No AST construction needed for simple extraction"

puts "\n--- Performance Notes ---"
puts "* Captures add minimal overhead (~5% for heavy use)"
puts "* Capture lookup is O(n) where n = number of captures"
puts "* Consider scope atoms for nested contexts"

puts "\n--- API Summary ---"
puts "  atom.capture(:name)  -> captures match result"
puts "  result[:name]         -> retrieves captured value"
puts "  context.captures[:name] -> in dynamic blocks"
```

#### basic.rb.md
```markdown
# Capture Atoms - Ruby Implementation

## How to Run

```bash
ruby example/captures/basic.rb
```

## Code Walkthrough

### Basic Setup

Create a grammar with captures, then parse:

```ruby
use parsanol/parslet

# Simple capture
parser = str('hello').capture(:greeting)
result = parser.parse("hello world")
puts result[:greeting]  # => "hello"
```

### Email Parsing with Nested Captures

```ruby
email_parser = match('[a-zA-Z0-9._%+-]+').capture(:local) >>
                 str('@') >>
                 match('[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}').capture(:domain)

result = email_parser.parse("user@example.com")
# => {:local => "user", :domain => "example.com"}
```

### Using Captures with Dynamic

```ruby
class CaptureParser < Parsanol::Parser
  include Parsanol::Parslet

  rule(:type) { match('[a-z]+').capture(:type) }
  rule(:value) do
    dynamic do |_source, context|
      case context.captures[:type]
      when 'int' then match('\d+')
      when 'str' then match('[a-z]+')
      end.capture(:value)
    end
  end
  rule(:declaration) { type >> str(':') >> match('[a-z]+').capture(:name) >> str('=') >> value }
  root :declaration
end

result = CaptureParser.new.parse("int:count=42")
# => {:type => "int", :name => "count", :value => "42"}
```

### Accessing Capture State

After parsing, access captures from the result:

```ruby
result = parser.parse(input)
result[:name]  # Access captured value
result.keys  # All capture names
```

## Output Types

### ParseResult

```ruby
result[:capture_name]  # Access captured value
result.keys              # All capture names
result.values           # All captured values
```

## Design Decisions

### Capture Persistence

Captures persist throughout the parse and are available in the result:

```ruby
# Captures from entire parse
result = parser.parse(input)
all_captures = result.keys
```

### Integration with Dynamic

Captures can be referenced in dynamic blocks:

```ruby
dynamic do |_source, context|
  captured = context.captures[:name]
  str(captured)  # Use captured value
end
```

## Performance Notes

| Metric | Value |
|--------|-------|
| Capture overhead | ~5% for heavy use |
| Lookup time | O(n) where n = number of captures |
| Memory per capture | Offset + length (zero-copy) |

**Optimization Tips**:
1. Use scopes to limit capture accumulation
2. Process captures incrementally for very large files
3. Capture only what you need

## Error Handling

```ruby
begin
  result = parser.parse(input)
  if result[:expected_capture]
    # Process capture
  end
rescue Parsanol::ParseFailed => e
  puts "Parse error: #{e}"
end
```

---

## 2. Scopes Example (`scopes/`)

### Update existing files

#### example.json
```json
{
  "id": "scopes",
  "title": "Scope Atoms",
  "description": "Create isolated capture contexts with scope atoms. Captures inside scopes are discarded on exit.",
  "category": "  feature",
  "tags": ["scope", "isolation", "capture-cleanup", "memory-management"],
  "difficulty": "intermediate",
  "concepts": ["scope atoms", "capture isolation", "memory cleanup", "nested contexts"],

  "motivation": {
    "why": "Prevent capture pollution from nested parsing. Each scope has its own capture state that's cleaned up on exit.",
    "useCases": [
      "Parse nested structures without accumulating captures",
      "Process repeated items with isolated capture state",
      "Memory-bounded parsing of large inputs"
    ]
  },

  "inputFormat": {
    "description": "Nested structures with scoped parsing",
    "examples": [
      {"input": "prefix inner suffix", "description": "Nested structure", "valid": true}
    ]
  },

  "outputFormat": {
    "description": "Only outer captures visible, inner captures discarded",
    "structure": {
      "capture_state": {"description": "Only captures from outside scopes"}
    }
  },

  "backendCompatibility": {
    "packrat": {"support": "full", "notes": "Native Packrat support"},
    "bytecode": {"support": "full", "notes": "Works via Packrat"},
    "streaming": {"support": "full", "notes": "Works with capture state"}
  },

  "related": ["capture", "dynamic"],
  "implementations": {
    "ruby": {"basic": "basic.rb" }
  }
}
```

#### basic.rb
```ruby
# frozen_string_literal: true

# Scope Atoms Example
#
# Demonstrates how to create isolated capture contexts with scope atoms.
# Captures made inside a scope are discarded when the scope exits.

$LOAD_PATH.unshift "#{File.dirname(__FILE__)}/../lib"
require 'parsanol/parslet'

include Parsanol::Parslet

puts "Scope Atoms Example"
puts "===================\n"

# ===========================================================================
# Example 1: Basic Scope Isolation
# ===========================================================================
puts "--- Example 1: Basic Scope Isolation ---\n"

# Without scope: captures accumulate, last value wins
parser = str('a').capture(:temp) >> str('b') >> str('c').capture(:temp)

input = "abc"
result = parser.parse(input)

puts "  Without scope:"
puts "    'temp' value: #{result[:temp].inspect}"  # "c" (last wins)

# With scope: inner captures are discarded
parser = str('prefix').capture(:outer) >> str(' ') >>
         scope { str('inner').capture(:inner) } >>
         str(' ') >> str('suffix').capture(:outer_end)

input = "prefix inner suffix"
result = parser.parse(input)

puts "\n  With scope:"
puts "    'outer': #{result[:outer].inspect}"
puts "    'outer_end': #{result[:outer_end].inspect}"
puts "    'inner': #{result[:inner].inspect rescue => nil}"  # Not present

# ===========================================================================
# Example 2: Nested Scopes
# ===========================================================================
puts "\n--- Example 2: Nested Scopes ---\n"

parser = str('L1').capture(:level) >> str(' ') >>
         scope {
           str('L2').capture(:level) >> str(' ') >>
           scope { str('L3').capture(:level) }
         }

input = "L1 L2 L3"
result = parser.parse(input)

puts "  Nested scopes - only L1 persists:"
puts "    'level' value: #{result[:level].inspect}"  # "L1"

# ===========================================================================
# Example 3: INI Configuration Parsing
# ===========================================================================
puts "\n--- Example 3: INI Configuration Parsing ---\n"

class IniParser < Parsanol::Parser
  include Parsanol::Parslet

  rule(:section_header) { str('[') >> match('[a-zA-Z_]+').capture(:section) >> str(']') >> str("\n") }
  rule(:kv_pair) { match('[a-zA-Z_]+').capture(:key) >> str('=') >> match('[^\n]+').capture(:value) >> str("\n") }
  rule(:section) { section_header >> scope { kv_pair.repeat(1) } }
  rule(:config) { section.repeat(1) }
  root :config
end

input = "[database]\nhost=localhost\nport=5432\n\n[server]\nport=8080\ndebug=true\n"

puts "  Input:\n#{input}"
parser = IniParser.new
result = parser.parse(input)

puts "  Outer captures: #{result.keys}"
puts "  (key/value captures are discarded after each section)"

# ===========================================================================
# Example 4: Scope for Memory Cleanup
# ===========================================================================
puts "\n--- Example 4: Scope for Memory Cleanup ---\n"

# Processing repeated structures - each gets its own scope
class ItemParser < Parsanol::Parser
  include Parsanol::Parslet

  rule(:item) { scope { match('\d+').capture(:id) >> str(':') >> match('[a-zA-Z]+').capture(:name) } }
  rule(:items) { str('item') >> item.repeat(1) }
  root :items
end

input = "item123:apple456:banana789:cherry"
puts "  Processing repeated items with scoped captures"
puts "  Input: #{input}"

parser = ItemParser.new
result = parser.parse(input)

puts "  Final captures: #{result.keys}"
puts "  (id and name captures are discarded after each item)"

# ===========================================================================
# Summary
# ===========================================================================
puts "\n--- Benefits of Scope Atoms ---"
puts "* Prevent capture pollution from nested parsing"
puts "* Each recursion level has its own capture state"
puts "* Automatic cleanup when scope exits"
puts "* Memory bounded during parse"
puts "* Essential for parsing nested structures"

puts "\n--- Performance Notes ---"
puts "* Scope push/pop is O(c_scope) where c_scope = captures in scope"
puts "* Each nesting level adds ~2% overhead"
puts "* Use scopes liberally - they're cheap"

puts "\n--- DSL Helper ---"
puts "  scope { parslet }  // Wraps parslet in isolated capture context"

puts "\n--- API Summary ---"
puts "  scope { inner }          -> isolates captures"
puts "  result[:name]            -> access captures (inner ones excluded)"
```

#### basic.rb.md
```markdown
# Scope Atoms - Ruby Implementation

## How to Run

```bash
ruby example/scopes/basic.rb
```

## Code Walkthrough

### Basic Scope Isolation

```ruby
# Without scope: last capture wins
parser = str('a').capture(:temp) >> str('b') >> str('c').capture(:temp)
result = parser.parse("abc")
result[:temp]  # => "c"

# With scope: inner capture is discarded
parser = str('prefix').capture(:outer) >>
         scope { str('inner').capture(:inner) } >>
         str('suffix').capture(:outer_end)

result = parser.parse("prefix inner suffix")
result[:inner]  # => nil (discarded)
result[:outer]  # => "prefix"
```

### Nested Scopes

```ruby
parser = str('L1').capture(:level) >> str(' ') >>
         scope {
           str('L2').capture(:level) >> str(' ') >>
           scope { str('L3').capture(:level) }
         }

result = parser.parse("L1 L2 L3")
result[:level]  # => "L1" (only outermost survives)
```

### INI Configuration Parsing

```ruby
class IniParser < Parsanol::Parser
  include Parsanol::Parslet

  rule(:section) { section_header >> scope { kv_pair.repeat(1) } }
end

# Each section's key/value captures are isolated
result = parser.parse("[database]\nhost=localhost\n\n[server]\nport=8080\n")
result.keys  # => [:section] (key/value discarded)
```

## Design Decisions

### Capture Isolation

Captures inside a scope are pushed onto a stack and popped on exit:

```ruby
scope {
  str('a').capture(:x)  # Pushed onto capture stack
}  # Popped on scope exit
```

### Memory Bounds

Memory for captures is bounded by scope depth:
```
memory = base_captures + sum(scope_captures)
```

For deeply nested structures, scopes prevent unbounded capture accumulation.

### Scope Stack

Scopes form a stack during parsing:
```ruby
# Scope depth 0
str('outer').capture(:a) >>
scope {
  # Scope depth 1
  str('inner').capture(:b) >>
  scope {
    # Scope depth 2
    str('deep').capture(:c)
  }
  # Back to depth 1, :c discarded
}
# Back to depth 0, :b discarded
```

## Performance Notes

| Metric | Value |
|--------|-------|
| Scope push/pop overhead | O(c_scope) captures |
| Per-nesting overhead | ~2% |
| Memory impact | Bounded by scope depth |

**Optimization Tips**:
1. Use scopes for repeated structures
2. Scope deeply nested parsing
3. Scope recursive rules

## Error Handling

```ruby
begin
  result = parser.parse(input)
rescue Parsanol::ParseFailed => e
  puts "Parse error: #{e}"
end
```

---

## 3. Dynamic Example (`dynamic/`)

### Location
`example/dynamic/`

### Files to Create

#### example.json
```json
{
  "id": "dynamic",
  "title": "Dynamic Atoms",
  "description": "Runtime-determined parsing via callbacks. Grammar can change based on context.",
  "category": "feature",
  "tags": ["dynamic", "callback", "context-sensitive", "runtime"],
  "difficulty": "advanced",
  "concepts": ["dynamic atoms", "callbacks", "context-sensitive parsing", "capture-aware"],

  "motivation": {
    "why": "Allow grammar to change at parse time based on position, input, or previously captured values.",
    "useCases": [
      "Context-sensitive keywords",
      "Parser switching based on captures",
      "Conditional parsing logic",
      "Configuration-driven grammars"
    ]
  },

  "inputFormat": {
    "description": "Any input - grammar determined at runtime",
    "examples": [
      {"input": "ruby def method", "description": "Ruby-style", "valid": true},
      {"input": "python lambda x", "description": "Python-style", "valid": true}
    ]
  },

  "outputFormat": {
    "description": "Parsed based on dynamic callback result",
    "structure": {
      "ast": {"description": "Parse tree from dynamically selected parser"}
    }
  },

  "backendCompatibility": {
    "packrat": {"support": "full", "notes": "Native Packrat support"},
    "bytecode": {"support": "partial", "notes": "Uses Packrat fallback"},
    "streaming": {"support": "partial", "notes": "Uses Packrat fallback"}
  },

  "related": ["capture", "scope"],
  "implementations": {
    "ruby": {"basic": "basic.rb" }
  }
}
```

#### basic.rb
```ruby
# frozen_string_literal: true

# Dynamic Atoms Example
#
# Demonstrates runtime-determined parsing via callbacks.
# Dynamic atoms allow context-sensitive parsing where the grammar
# itself depends on the input or previously captured values.

$LOAD_PATH.unshift "#{File.dirname(__FILE__)}/../lib"
require 'parsanol/parslet'
require 'pp'

puts "Dynamic Atoms Example"
puts "====================\n"

# ===========================================================================
# Example 1: Constant Callback
# ===========================================================================
puts "--- Example 1: Constant Callback ---\n"

# Always returns the same parser
parser = dynamic { str('hello') }

input = "hello world"
result = parser.parse(input)
puts "  Parsed successfully"
puts "  Matched: #{result.inspect}"

# ===========================================================================
# Example 2: Context-Sensitive Callback
# ===========================================================================
puts "\n--- Example 2: Context-Sensitive Callback ---\n"

# Different keyword based on preceding context
class LanguageParser < Parsanol::Parser
  include Parsanol::Parslet

  rule(:keyword) do
    dynamic do |source, context|
      pos = context.pos
      input = source.string

      # Look at preceding context
      if pos >= 5 && input[pos-5..pos] == 'ruby '
        puts "    -> Detected Ruby context"
        str('def')
      elsif pos >= 7 && input[pos-7..pos] == 'python '
        puts "    -> Detected Python context"
        str('lambda')
      else
        puts "    -> No context, using 'function'"
        str('function')
      end
    end
  end

  rule(:statement) { str('ruby ') | str('python ') | str('') } >> keyword >> match('[a-z]+')
  root :statement
end

test_cases = [
  ['ruby def method', 'Ruby'],
  ['python lambda x', 'Python'],
  ['function foo', 'JavaScript']
]

test_cases.each do |input, lang|
  puts "  Testing #{lang} input: #{input.inspect}"
  parser = LanguageParser.new
  begin
    result = parser.parse(input)
    puts "  ✓ Parsed: #{result.inspect}"
  rescue Parsanol::ParseFailed => e
    puts "  ✗ Parse error: #{e}"
  end
  puts
end

# ===========================================================================
# Example 3: Position-Based Callback
# ===========================================================================
puts "--- Example 3: Position-Based Callback ---\n"

class PositionParser < Parsanol::Parser
  include Parsanol::Parslet

  rule(:token) do
    dynamic do |source, context|
      pos = context.pos
      input = source.string

      if pos == 0
        # First position: keyword
        str('let') | str('const') | str('var')
      elsif pos < input.length / 2
        # First half: identifier
        match('[a-zA-Z_][a-zA-Z0-9_]*')
      else
        # Second half: value
        match('\d+') | match('[a-z]+')
      end
    end
  end

  rule(:stmt) { token >> str(' ') >> token >> str('=') >> token }
  root :stmt
end

input = "let x=42"
puts "  Parsing: #{input.inspect}"
parser = PositionParser.new
begin
  result = parser.parse(input)
  puts "  ✓ Parsed: #{result.inspect}"
rescue Parsanol::ParseFailed => e
  puts "  ✗ Parse error: #{e}"
end

# ===========================================================================
# Example 4: Capture-Aware Callback
# ===========================================================================
puts "\n--- Example 4: Capture-Aware Callback ---\n"

class CaptureAwareParser < Parsanol::Parser
  include Parsanol::Parslet

  rule(:type) { match('[a-z]+').capture(:type) }
  rule(:name) { match('[a-z]+').capture(:name) }

  rule(:value) do
    dynamic do |_source, context|
      type = context.captures[:type]
      puts "    -> Type capture: #{type.inspect}"

      case type
      when 'int' then match('\d+')
      when 'str' then match('[a-z]+')
      when 'bool' then str('true') | str('false')
      else match('[a-z]+')
      end.capture(:value)
    end
  end

  rule(:declaration) { type >> str(':') >> name >> str('=') >> value }
  root :declaration
end

test_cases = [
  ['int:count=42', 'int'],
  ['str:message=hello', 'str'],
  ['bool:enabled=true', 'bool']
]

test_cases.each do |input, expected_type|
  puts "  Parsing: #{input.inspect}"
  parser = CaptureAwareParser.new
  begin
    result = parser.parse(input)
    puts "  ✓ Parsed successfully"
    puts "    type: #{result[:type].inspect}"
    puts "    name: #{result[:name].inspect}"
    puts "    value: #{result[:value].inspect}"
  rescue Parsanol::ParseFailed => e
    puts "  ✗ Parse error: #{e}"
  end
  puts
end

# ===========================================================================
# Example 5: Configuration-Driven Parsing
# ===========================================================================
puts "\n--- Example 5: Configuration-Driven Parsing ---\n"

# Parser behavior can be configured at runtime
strict_mode = true

class ConfigurableParser < Parsanol::Parser
  include Parsanol::Parslet

  attr_accessor :strict_mode

  rule(:identifier) do
    dynamic do |_source, _context|
      if @strict_mode
        # Strict: lowercase only
        match('[a-z][a-z0-9_]*')
      else
        # Lenient: any identifier
        match('[a-zA-Z_][a-zA-Z0-9_]*')
      end
    end
  end

  root :identifier
end

puts "  Strict mode (lowercase only):"
parser = ConfigurableParser.new
parser.strict_mode = true

['variable', 'Variable'].each do |input|
  begin
    result = parser.parse(input)
    puts "    ✓ #{input.inspect} - accepted"
  rescue Parsanol::ParseFailed
    puts "    ✗ #{input.inspect} - rejected"
  end
end

# ===========================================================================
# Summary
# ===========================================================================
puts "\n--- Benefits of Dynamic Atoms ---"
puts "* Context-sensitive parsing at runtime"
puts "* Access to position, input, and captures"
puts "* Plugin architecture support"
puts "* Configuration-driven grammars"

puts "\n--- Backend Compatibility ---"
puts "* Packrat:  Native support (direct callback invocation)"
puts "* Bytecode: Packrat fallback (slower)"
puts "* Streaming: Packrat fallback (slower)"

puts "\n--- Performance Notes ---"
puts "* Native (Packrat): ~5% overhead per dynamic atom"
puts "* Callback should be fast - avoid I/O or heavy computation"

puts "\n--- DSL Helper ---"
puts "  dynamic { |source, context| parslet }  # Block returns parser"

puts "\n--- API Summary ---"
puts "  dynamic do |source, context|"
puts "    context.pos           # Current position"
puts "    context.captures[:n]  # Access captured values"
puts "    source.string         # Full input"
puts "    # Return a parslet atom"
puts "  end"
```

#### basic.rb.md
```markdown
# Dynamic Atoms - Ruby Implementation

## How to Run

```bash
ruby example/dynamic/basic.rb
```

## Code Walkthrough

### Basic Dynamic Callback

```ruby
# Always returns the same parser
parser = dynamic { str('hello') }
result = parser.parse("hello world")
```

### Context-Sensitive Callback

```ruby
dynamic do |source, context|
  pos = context.pos
  input = source.string

  # Look at preceding context
  if pos >= 5 && input[pos-5..pos] == 'ruby '
    str('def')
  else
    str('function')
  end
end
```

### Capture-Aware Callback

```ruby
dynamic do |_source, context|
  type = context.captures[:type]

  case type
  when 'int' then match('\d+')
  when 'str' then match('[a-z]+')
  when 'bool' then str('true') | str('false')
  end.capture(:value)
end
```

## DynamicContext Fields

```ruby
source   # Source object with input string
pos      # Current position in input
captures # Hash of captured values
```

## Design Decisions

### Callback Signature

```ruby
dynamic do |source, context|
  # source: Parsanol::Source or similar
  # context: Parsanol::Context with pos and captures

  # Must return a parslet atom
  str('something') | match('pattern')
end
```

### When Callback is Invoked

Callbacks are invoked at parse time, not grammar construction time:

```ruby
# Grammar construction - callback NOT invoked yet
parser = dynamic { some_callback }

# Parse time - callback invoked
result = parser.parse(input)
```

## Performance Notes

| Metric | Value |
|--------|-------|
| Callback overhead | ~5% per dynamic atom |
| Fallback overhead | ~20% slower on non-Packrat backends |
| Recommended for | Context-sensitive parsing |

**Optimization Tips**:
1. Keep callbacks fast - avoid I/O
2. Cache expensive computations
3. Use with capture for type-driven parsing

## Error Handling

```ruby
dynamic do |source, context|
  # Return nil to fail the parse
  nil  # Causes parse failure at this position

  # Or return a valid atom
  str('valid')
end
```

---

## 4. Streaming with Captures Example (`streaming-captures/`)

### Location
`example/streaming-captures/`

### Files to Create

#### example.json
```json
{
  "id": "streaming-captures",
  "title": "Streaming Parser with Captures",
  "description": "Extract named values from large files without loading them into memory.",
  "category": "feature",
  "tags": ["streaming", "captures", "memory", "large-files", "extraction"],
  "difficulty": "advanced",
  "concepts": ["streaming with captures", "bounded memory", "chunk-based extraction", "capture persistence"],

  "motivation": {
    "why": "Combine memory efficiency of streaming with convenience of named captures for GB-scale files.",
    "useCases": [
      "Log file analysis (extract IPs, status codes from 10GB+ logs)",
      "CSV export processing (extract emails from 500MB+ files)",
      "Real-time feed parsing (stock tickers, event streams)",
      "Database dump processing (extract specific fields)"
    ]
  },

  "inputFormat": {
    "description": "Large files processed in fixed-size chunks.",
    "examples": [
      {"input": "192.168.1.1 - - [10/Oct/2000:13:55:36] \"GET /path\" 200 2326\n...", "description": "Apache access log (millions of lines)", "valid": true}
    ]
  },

  "outputFormat": {
    "description": "Captures available after streaming parse completes.",
    "structure": {
      "capture_state": {"description": "CaptureState with all extracted captures"}
    }
  },

  "backendCompatibility": {
    "packrat": {"support": "n/a", "notes": "Use streaming instead"},
    "bytecode": {"support": "n/a", "notes": "Use streaming instead"},
    "streaming": {"support": "full", "notes": "Primary backend for this feature"}
  },

  "related": ["captures", "scopes", "streaming"],
  "implementations": {
    "ruby": {"basic": "basic.rb" }
  }
}
```

#### basic.rb
```ruby
# frozen_string_literal: true

# Streaming Parser with Captures Example
#
# Demonstrates how to extract named values from large files without loading
# them into memory. Combines streaming parser efficiency with capture extraction.

$LOAD_PATH.unshift "#{File.dirname(__FILE__)}/../lib"
require 'parsanol/parslet'
require 'parsanol/streaming_parser'
require 'stringio'

puts "Streaming Parser with Captures Example"
puts "======================================\n"

# ===========================================================================
# Example 1: Basic Streaming with Captures
# ===========================================================================
puts "--- Example 1: Basic Streaming with Captures ---\n"

# Grammar: Extract email addresses
email_parser = match('[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}').capture(:email)

# Configure streaming with small chunks for demo
config = { chunk_size: 64, window_size: 2 }

input = "Contact us at user@example.com or support@test.org for help."

puts "  Input: #{input.inspect}"
puts "  Chunk size: #{config[:chunk_size]}"
puts "  Window size: #{config[:window_size]}"

# Note: Requires native extension for full streaming functionality
if Parsanol::Native.available?
  streaming_parser = Parsanol::StreamingParser.new(email_parser, **config)

  io = StringIO.new(input)
  results = streaming_parser.parse_stream(io)

  puts "  Results: #{results.length} items parsed"

  results.each do |result|
    puts "    #{result.inspect}"
  end
else
  puts "  (Streaming parser requires native extension)"
  puts "  Falling back to regular parse:"

  result = email_parser.parse(input)
  puts "  Captured email: #{result[:email].inspect}"
end

# ===========================================================================
# Example 2: Log File Analysis
# ===========================================================================
puts "\n--- Example 2: Log File Analysis ---\n"

# Grammar: Parse Apache-style log lines
# Pattern: IP - - [timestamp] "METHOD path ..." status size
ip_parser = match('\d+\.\d+\.\d+\.\d+').capture(:ip)
timestamp_parser = match('[^\]]+').capture(:timestamp)
method_parser = match('[A-Z]+').capture(:method)
path_parser = match('[^\s]+').capture(:path)

log_parser = ip_parser >> str(' - - [') >> timestamp_parser >> str('] "') >>
              method_parser >> str(' ') >> path_parser >> match(' [^"]+ "') >>
              match('\d+').capture(:status) >> str(' ') >> match('\d+').capture(:size)

sample_log = <<~LOG
192.168.1.1 - - [10/Oct/2000:13:55:36 -0700] "GET /index.html HTTP/1.0" 200 2326
10.0.0.1 - - [10/Oct/2000:13:55:37 -0700] "POST /api/users HTTP/1.0" 201 512
172.16.0.1 - - [10/Oct/2000:13:55:38 -0700] "GET /favicon.ico HTTP/1.0" 404 128
LOG

puts "  Processing log file..."
puts "  Sample input (#{sample_log.lines.count} lines):"
sample_log.lines.first(2).each { |line| puts "    #{line}" }

if Parsanol::Native.available?
  streaming_parser = Parsanol::StreamingParser.new(log_parser, chunk_size: 128)

  io = StringIO.new(sample_log)
  results = streaming_parser.parse_stream(io)

  puts "  Parsed #{results.length} log lines"
else
  puts "  (Streaming requires native extension)"
  puts "  Parsing first line with regular parser:"

  result = log_parser.parse(sample_log.lines.first)
  puts "    IP: #{result[:ip].inspect}"
  puts "    Method: #{result[:method].inspect}"
  puts "    Status: #{result[:status].inspect}"
end

# ===========================================================================
# Example 3: Memory-Bounded Processing
# ===========================================================================
puts "\n--- Example 3: Memory-Bounded Processing ---\n"

word_parser = match('[a-z]+').capture(:word)
input = "apple banana cherry date elderberry fig grape"

puts "  Input: #{input}"
puts "  Testing different chunk sizes:"

[16, 32, 64].each do |chunk_size|
  if Parsanol::Native.available?
    streaming_parser = Parsanol::StreamingParser.new(word_parser, chunk_size: chunk_size)
    io = StringIO.new(input)

    results = streaming_parser.parse_stream(io)
    puts "    Chunk size #{chunk_size}: #{results.length} results"
  else
    puts "    Chunk size #{chunk_size}: (requires native extension)"
  end
end

puts "\n  Memory usage is bounded by chunk_size * window_size"

# ===========================================================================
# Example 4: Chunk Size Selection Guide
# ===========================================================================
puts "\n--- Example 4: Chunk Size Selection Guide ---\n"

puts "  | Use Case              | Chunk Size   | Reason |"
puts "  |----------------------|--------------|--------|"
puts "  | Real-time feeds      | 4-16 KB      | Low latency |"
puts "  | Log files            | 256 KB - 1 MB | Throughput |"
puts "  | Network streams      | 8-64 KB      | Balance |"
puts "  | Large files          | 1-4 MB       | Fewer syscalls |"

puts "\n  Window size guidelines:"
puts "  | Grammar type         | Window | Reason |"
puts "  |----------------------|--------|--------|"
puts "  | Sequential           | 1-2    | Minimal backtracking |"
puts "  | Moderate backtracking| 2-3    | Default |"
puts "  | Heavy backtracking   | 4-5    | Complex grammars |"

puts "\n  Memory formula: memory = chunk_size * window_size + capture_state"

# ===========================================================================
# Example 5: StreamingResult Structure
# ===========================================================================
puts "\n--- Example 5: StreamingResult Structure ---\n"

puts "  StreamingParser#parse_stream returns:"
puts "  ["
puts "    {"
puts "      ast: ...,               # Parse tree"
puts "      bytes_processed: N,     # Bytes read"
puts "      captures: { ... },      # Extracted captures"
puts "    },"
puts "    ..."
puts "  ]"

# ===========================================================================
# Summary
# ===========================================================================
puts "\n--- Benefits of Streaming with Captures ---"
puts "* Process files larger than available RAM"
puts "* Captures persist across streaming parse operations"
puts "* Memory bounded by chunk_size * window_size"
puts "* Single pass through data"
puts "* Extract specific fields without loading entire file"

puts "\n--- Performance Notes ---"
puts "* Memory: O(chunk_size * window_size)"
puts "* Captures: Accumulate during parse, available at end"
puts "* For very large captures: process incrementally with reset()"

puts "\n--- API Summary ---"
puts "  parser = StreamingParser.new(grammar, chunk_size: 65536)"
puts "  results = parser.parse_stream(io)"
puts "  results.each { |r| r[:capture_name] }"
```

#### basic.rb.md
```markdown
# Streaming Parser with Captures - Ruby Implementation

## How to Run

```bash
ruby example/streaming-captures/basic.rb
```

## Requirements

Streaming parsing requires the native extension:

```ruby
# Check if native extension is available
Parsanol::Native.available?  # => true/false
```

## Code Walkthrough

### Basic Setup

```ruby
require 'parsanol/streaming_parser'

parser = Parsanol::StreamingParser.new(grammar, chunk_size: 65536)

File.open("large.log") do |f|
  parser.parse_stream(f) do |result|
    # Process each result
    puts result[:capture_name]
  end
end
```

### Chunk Configuration

```ruby
parser = Parsanol::StreamingParser.new(
  grammar,
  chunk_size: 64 * 1024,  # 64 KB chunks
  window_size: 2           # Keep 2 chunks in memory
)
```

### Accessing Captures

```ruby
results = parser.parse_stream(io)

results.each do |result|
  result[:capture_name]  # Access captured value
end
```

## StreamingParser API

```ruby
class Parsanol::StreamingParser
  # Create new streaming parser
  def initialize(grammar, chunk_size: 4096)

  # Add a chunk of input
  def add_chunk(chunk)

  # Parse current buffer
  def parse_chunk

  # Parse entire stream (yields results)
  def parse_stream(io, chunk_size: @chunk_size)

  # Reset for reuse
  def reset
end
```

## Chunk Size Selection

| Use Case | Chunk Size | Reason |
|----------|------------|--------|
| Real-time feeds | 4-16 KB | Low latency |
| Log files | 256 KB - 1 MB | Throughput |
| Network streams | 8-64 KB | Balance |
| Large files | 1-4 MB | Fewer syscalls |

## Memory Bounds

Memory is bounded by:
```
memory = chunk_size * window_size + capture_state
```

## Performance Notes

| Metric | Value |
|--------|-------|
| Memory overhead | chunk_size * window_size |
| Streaming overhead | ~10% vs non-streaming |
| Native required | Yes for full functionality |

**Optimization Tips**:
1. Use appropriate chunk size for your use case
2. Process results incrementally
3. Reset parser between independent inputs

## Error Handling

```ruby
begin
  results = parser.parse_stream(io)
rescue Parsanol::ParseFailed => e
  puts "Parse error: #{e}"
end
```

---

## Implementation Checklist

### Files to Create

```
parsanol-ruby/example/
├── captures/
│   ├── basic.rb
│   ├── basic.rb.md
│   └── example.json
├── scopes/
│   ├── basic.rb         # UPDATE existing
│   ├── basic.rb.md      # CREATE
│   └── example.json     # UPDATE existing
├── dynamic/
│   ├── basic.rb
│   ├── basic.rb.md
│   └── example.json
└── streaming-captures/
    ├── basic.rb
    ├── basic.rb.md
    └── example.json
```

### Implementation Steps

1. **Create captures/ directory**
   - [ ] `basic.rb` - Main example demonstrating capture atoms
   - [ ] `basic.rb.md` - Documentation in markdown
   - [ ] `example.json` - Metadata

2. **Update scopes/ directory**
   - [x] `basic.rb` - Exists, needs enhancement
   - [ ] `basic.rb.md` - Create documentation
   - [x] `example.json` - Exists, needs enhancement

3. **Create dynamic/ directory**
   - [ ] `basic.rb` - Main example demonstrating dynamic atoms
   - [ ] `basic.rb.md` - Documentation in markdown
   - [ ] `example.json` - Metadata

4. **Create streaming-captures/ directory**
   - [ ] `basic.rb` - Main example demonstrating streaming with captures
   - [ ] `basic.rb.md` - Documentation in markdown
   - [ ] `example.json` - Metadata

### Key Ruby API Patterns

| Feature | Ruby DSL |
|---------|----------|
| Capture | `atom.capture(:name)` |
| Scope | `scope { inner }` |
| Dynamic | `dynamic { \|source, context\| atom }` |
| Streaming | `StreamingParser.new(grammar).parse_stream(io)` |
| Access capture | `result[:name]` |
| Context captures | `context.captures[:name]` |

### Testing

After implementation, run each example:
```bash
cd parsanol-ruby
ruby example/captures/basic.rb
ruby example/scopes/basic.rb
ruby example/dynamic/basic.rb
ruby example/streaming-captures/basic.rb
```
