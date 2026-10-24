# frozen_string_literal: true

# rs#298 slice 1: the token-parity wire. Dumps the Ruby lexer's token
# stream (type/value/offset — the exact structs lexer.rb emits, minus
# the dropped ws/comment) for a .parg source, as the fixture the Rust
# lexer's parity test diffs against:
#
#   ruby tools/dump_parg_tokens.rb corpus/conformance/x.parg \
#     parsanol/tests/fixtures/token_parity/x.tokens.json
#
# Regenerate whenever the Ruby lexer changes; the Rust test fails on
# any drift until the fixtures are refreshed in the same PR.
$LOAD_PATH.unshift File.expand_path("../parsanol", __dir__) if ENV["PARSANOL_LIB"]
require "parsanol"
require "json"

source_path = ARGV.fetch(0)
out_path = ARGV.fetch(1)
document = File.read(source_path)
tokens = Parsanol::PARG::Lexer.new(document).tokens.map do |token|
  { "type" => token.type.to_s, "value" => token.value, "offset" => token.offset }
end
File.write(out_path, JSON.generate({ "source" => document, "tokens" => tokens }))
puts "#{source_path}: #{tokens.length} tokens"
