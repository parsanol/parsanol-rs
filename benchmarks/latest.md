# Benchmark receipts

- commit: `25a3ce9286419e317b9216f05f2139f04a948a57`
- rustc: `rustc 1.98.0 (88d9e12ae 2026-08-18) (Homebrew)`
- machine: `Darwin 23.1.0 arm64`
- date: 2026-11-12
- method: criterion, bounded CI run (sample-size 10, 1s warm-up, 3s measure); the reported number is the criterion estimate in ns/iter

## libraries — parsanol native vs the parslet-compatibility path

The campaign bar ("beat parslet on both paths") measured on the
same wire: the native engine and the parslet-compatibility
transformation over identical grammars and inputs.

| benchmark | ns/iter |
|---|---:|
| json/parsanol/tiny | 3190 |
| json/parsanol/small | 3155 |
| json/parsanol-parslet/small | 3161 |
| expression/parsanol/tiny | 1711 |
| expression/parsanol/medium | 1817 |
| expression/parsanol/large | 1909 |
| express/parsanol/tiny | 1839 |
| express/parsanol/small | 2002 |
| throughput/parsanol_json | 3135 |

## comparison — summary groups (serialize vs direct paths)

| benchmark | ns/iter |
|---|---:|
| json/A_serialize_null | 1672 |
| json/B_direct_null | 1662 |
| json/A_serialize_42 | 2388 |
| json/B_direct_42 | 2317 |
| calculator/A_serialize_simple | 4269 |
| calculator/B_direct_simple | 4098 |
| calculator/A_serialize_add | 5742 |
| calculator/B_direct_add | 5640 |
| calculator/A_serialize_complex | 7035 |
| calculator/B_direct_complex | 7107 |
| csv/A_serialize_small | 2009 |
| csv/B_direct_small | 1960 |
| csv/A_serialize_large | 2581 |
| csv/B_direct_large | 2346 |
| summary/A_total_serialize | 18856 |
| summary/B_total_direct | 18739 |
