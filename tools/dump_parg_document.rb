# frozen_string_literal: true

# rs#298 slice 2: the document-parity wire. Dumps the Ruby parser's
# DOCUMENT structure (grammar name/version, entries, embedded tests,
# and each rule's AST as nested nodes) for a .parg source:
#
#   ruby tools/dump_parg_document.rb corpus/conformance/x.parg \
#     parsanol/tests/fixtures/doc_parity/x.document.json
#
# The Rust parser's parity test diffs against it, like the token
# fixtures. Regenerate in the same PR as any Ruby parser change.
$LOAD_PATH.unshift File.expand_path("../parsanol", __dir__) if ENV["PARSANOL_LIB"]
require "parsanol"
require "json"

def node_to_h(node)
  case node.kind
  when :lit then { "kind" => "lit", "text" => node.a, "fold" => node.b == true }
  when :class then { "kind" => "class", "ranges" => node.a }
  when :category then { "kind" => "category", "code" => node.a }
  when :seq then { "kind" => "seq", "items" => node.a.map { |c| node_to_h(c) } }
  when :alt then { "kind" => "alt", "branches" => node.a.map { |c| node_to_h(c) } }
  when :rep then { "kind" => "rep", "min" => node.b, "max" => node.c, "child" => node_to_h(node.a) }
  when :opt then { "kind" => "opt", "child" => node_to_h(node.a) }
  when :pred
    hash = { "kind" => "pred", "positive" => node.a == true, "child" => node_to_h(node.b) }
    hash["behind"] = true if node.c == :behind
    hash
  when :cap then { "kind" => "cap", "name" => node.a, "child" => node_to_h(node.b) }
  when :ref then { "kind" => "ref", "name" => node.a }
  when :table then { "kind" => "table", "table" => node.a, "column" => node.b }
  else raise "unknown node kind #{node.kind}"
  end
end

source_path = ARGV.fetch(0)
out_path = ARGV.fetch(1)
document = Parsanol::PARG::Parser.new(File.read(source_path)).parse
File.write(out_path, JSON.generate(
  "grammar_name" => document.grammar_name,
  "version" => document.version,
  "entries" => document.entries,
  "tests" => document.tests.map do |test|
    { "kind" => test.kind, "entry" => test.entry, "input" => test.input, "expect" => test.expect }
  end,
  "rules" => document.rules.transform_values { |node| node_to_h(node) },
))
puts "#{source_path}: #{document.rules.size} rules, #{document.entries.size} entries"
