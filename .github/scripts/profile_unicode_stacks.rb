require 'bundler/setup'
$LOAD_PATH.unshift(File.join(ENV.fetch('PROFILE_ROOT'), 'lib'))
require 'plurimath'

abort 'candidate gem source was not selected' unless Gem.loaded_specs.fetch('parsanol').full_gem_path == ENV.fetch('PROFILE_EXPECT_GEM')
abort 'native backend unavailable' unless Parsanol::Native.available?

parser = Plurimath::UnicodeMath::Parse.new
text = Plurimath::UnicodeMath::Parser.new('M_1 M_2').text
parser.parse(text, mode: :native)
$stdout.sync = true
puts 'READY: candidate grammar warmed; sampling native parsing only'
Process.kill('STOP', Process.pid)
40.times { parser.parse(text, mode: :native) }
puts 'RESULT: native parsing completed'
