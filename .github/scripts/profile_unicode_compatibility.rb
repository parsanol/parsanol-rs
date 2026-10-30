# Diagnostic only: no product changes, native compilation, or full spec suite.
require 'bundler/setup'
require 'json'
require 'digest'
require 'timeout'
$stdout.sync = true
root = ENV.fetch('PROFILE_ROOT')
mode = ENV.fetch('PROFILE_MODE').to_sym
$LOAD_PATH.unshift(File.join(root, 'lib'))
require 'plurimath'

module Probe
  class << self
    attr_accessor :enabled, :counters

    def record(name)
      return yield unless enabled

      started = Process.clock_gettime(Process::CLOCK_MONOTONIC)
      begin
        yield
      ensure
        entry = (counters[name] ||= { calls: 0, seconds: 0.0 })
        entry[:calls] += 1
        entry[:seconds] += Process.clock_gettime(Process::CLOCK_MONOTONIC) - started
      end
    end

    def wrap(object, method, label)
      object.singleton_class.prepend(Module.new do
        define_method(method) do |*args, **kwargs, &block|
          Probe.record(label) { super(*args, **kwargs, &block) }
        end
      end)
    end

    def measure
      self.counters = {}
      self.enabled = true
      before = GC.stat(:total_allocated_objects)
      started = Process.clock_gettime(Process::CLOCK_MONOTONIC)
      value = yield
      [value, { seconds: Process.clock_gettime(Process::CLOCK_MONOTONIC) - started,
                allocated_objects: GC.stat(:total_allocated_objects) - before,
                counters: counters }]
    ensure
      self.enabled = false
    end
  end
end

def portable(value)
  case value
  when Hash
    ['hash', value.map { |k, v| [[k.class.name, k.to_s], portable(v)] }.sort_by { |kv| kv[0] }]
  when Array then ['array', value.map { |v| portable(v) }]
  else
    if value.class.name.end_with?('::Slice')
      ['slice', value.to_s, value.offset]
    else
      value
    end
  end
end

if mode != :parslet
  abort 'wrong native version' unless Gem.loaded_specs.fetch('parsanol').version.to_s == '1.3.116'
  abort 'native backend unavailable' unless Parsanol::Native.available?
  gem_path = Gem.loaded_specs.fetch('parsanol').full_gem_path
  binaries = $LOADED_FEATURES.grep(/parsanol_native\.(so|bundle|dll)$/)
  abort 'wrong native binary source' if binaries.empty? || binaries.any? { |path| !path.start_with?(gem_path + '/') }
  if ENV['PROFILE_EXPECT_GEM']
    abort 'candidate gem source was not selected' unless gem_path == ENV.fetch('PROFILE_EXPECT_GEM')
  end
  Probe.wrap(Parsanol::Native::Parser, :grammar_handle, :handle_total)
  Probe.wrap(Parsanol::GrammarSerializer, :serialize, :serialization_including_lazy_grammar_build)
  Probe.wrap(Parsanol::Native, :_register_grammar, :native_registration)
  Probe.wrap(Parsanol::Native, :_parse_handle_with_compatibility, :native_execution_and_batch_return)
  Probe.wrap(Parsanol::Native::BatchDecoder, :decode_and_flatten, :ruby_batch_decode)
end

metadata = {
  kind: 'metadata', mode: mode, root: root, ruby: RUBY_DESCRIPTION,
  parser_ancestors: Plurimath::UnicodeMath::Parse.ancestors.first(4).map(&:name),
  parser_feature: $LOADED_FEATURES.grep(%r{/unicode_math/parse\.rb$}),
  plurimath_feature: $LOADED_FEATURES.grep(%r{/plurimath\.rb$}),
  parsanol: Gem.loaded_specs['parsanol']&.version&.to_s,
  binaries: $LOADED_FEATURES.grep(/parsanol_native\.(so|bundle|dll)$/),
  lock_sha256: Digest::SHA256.file(ENV.fetch('BUNDLE_GEMFILE') + '.lock').hexdigest,
  scope: 'One representative input, three repetitions; diagnostic, not a suite-wide speedup benchmark. Nested counters overlap.'
}
puts JSON.generate(metadata)

Timeout.timeout(Integer(ENV.fetch('PROFILE_TIMEOUT_SECONDS', '85')).clamp(1, 300)) do
  input = 'M_1 M_2'
  text, preprocessing = Probe.measure { Plurimath::UnicodeMath::Parser.new(input).text }
  puts JSON.generate(kind: 'preprocessing', input: input, text: text, timing: preprocessing)
  [:fresh, :reused].each do |lifecycle|
    parser = Plurimath::UnicodeMath::Parse.new if lifecycle == :reused
    if parser
      mode == :parslet ? parser.parse(text) : parser.parse(text, mode: mode)
    end
    3.times do |iteration|
      selected, setup = Probe.measure { parser || Plurimath::UnicodeMath::Parse.new }
      tree, parsing = Probe.measure do
        mode == :parslet ? selected.parse(text) : selected.parse(text, mode: mode)
      end
      formula, transforming = Probe.measure do
        nodes = Plurimath::UnicodeMath::Transform.new.apply(tree)
        Plurimath::Math::Formula.new(Array(nodes))
      end
      output, rendering = Probe.measure { formula.to_latex }
      calls = parsing[:counters].fetch(:native_execution_and_batch_return, {}).fetch(:calls, 0)
      abort 'expected actual native execution' if mode == :native && calls != 1
      abort 'unexpected native execution' if mode != :native && calls != 0
      row = { kind: 'sample', mode: mode, lifecycle: lifecycle, iteration: iteration,
              setup: setup, parsing: parsing, transforming: transforming, rendering: rendering,
              raw_sha256: Digest::SHA256.hexdigest(JSON.generate(portable(tree))), latex: output }
      puts JSON.generate(row)
    end
  end
end
