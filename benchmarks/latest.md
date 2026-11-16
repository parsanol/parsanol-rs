# Benchmark receipts

- commit: `ca838d0f7483865bb2551f98da1039d488245082`
- rustc: `rustc 1.100.0 (0ef1bc386 2026-11-09)`
- machine: `GitHub Actions 1000069890`
- date: 2026-11-16
- method: criterion, bounded CI run (sample-size 10, 1s warm-up, 3s measure); the reported number is the criterion estimate in ns/iter

## libraries — parsanol native vs the parslet-compatibility path

The campaign bar ("beat parslet on both paths") measured on the
same wire: the native engine and the parslet-compatibility
transformation over identical grammars and inputs.

| benchmark | ns/iter |
|---|---:|
| json/parsanol/tiny | 1709 |
| json/parsanol/small | 1921 |
| json/parsanol-parslet/small | 1933 |
| expression/parsanol/tiny | 539 |
| expression/parsanol/medium | 691 |
| expression/parsanol/large | 764 |
| express/parsanol/tiny | 777 |
| express/parsanol/small | 951 |
| throughput/parsanol_json | 1911 |

## comparison — summary groups (serialize vs direct paths)

| benchmark | ns/iter |
|---|---:|
| json/A_serialize_null | 1088 |
| json/B_direct_null | 1060 |
| json/A_serialize_42 | 1413 |
| json/B_direct_42 | 1274 |
| calculator/A_serialize_simple | 3548 |
| calculator/B_direct_simple | 3385 |
| calculator/A_serialize_add | 4783 |
| calculator/B_direct_add | 4680 |
| calculator/A_serialize_complex | 5451 |
| calculator/B_direct_complex | 5439 |
| csv/A_serialize_small | 828 |
| csv/B_direct_small | 765 |
| csv/A_serialize_large | 1262 |
| csv/B_direct_large | 1102 |
| summary/A_total_serialize | 14409 |
| summary/B_total_direct | 13930 |
