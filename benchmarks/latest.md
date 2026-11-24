# Benchmark receipts

- commit: `72c6c4c9df279526bf7074393d4f6d4328b045ec`
- rustc: `rustc 1.100.0 (0ef1bc386 2026-11-09)`
- machine: `GitHub Actions 1000079233`
- date: 2026-11-23
- method: criterion, bounded CI run (sample-size 10, 1s warm-up, 3s measure); the reported number is the criterion estimate in ns/iter

## libraries — parsanol native vs the parslet-compatibility path

The campaign bar ("beat parslet on both paths") measured on the
same wire: the native engine and the parslet-compatibility
transformation over identical grammars and inputs.

| benchmark | ns/iter |
|---|---:|
| json/parsanol/tiny | 2206 |
| json/parsanol/small | 2533 |
| json/parsanol-parslet/small | 2552 |
| expression/parsanol/tiny | 782 |
| expression/parsanol/medium | 942 |
| expression/parsanol/large | 1048 |
| express/parsanol/tiny | 1069 |
| express/parsanol/small | 1292 |
| throughput/parsanol_json | 2586 |

## comparison — summary groups (serialize vs direct paths)

| benchmark | ns/iter |
|---|---:|
| json/A_serialize_null | 1457 |
| json/B_direct_null | 1441 |
| json/A_serialize_42 | 1762 |
| json/B_direct_42 | 1680 |
| calculator/A_serialize_simple | 4544 |
| calculator/B_direct_simple | 4419 |
| calculator/A_serialize_add | 6183 |
| calculator/B_direct_add | 5930 |
| calculator/A_serialize_complex | 7280 |
| calculator/B_direct_complex | 6932 |
| csv/A_serialize_small | 1183 |
| csv/B_direct_small | 1053 |
| csv/A_serialize_large | 1845 |
| csv/B_direct_large | 1651 |
| summary/A_total_serialize | 18590 |
| summary/B_total_direct | 18481 |
