"""The tree-sitter column of the recovery table (rs#304): the same
seeded corruption stream over the kv language, driven through
tree-sitter's own error recovery. Requires: the tree-sitter CLI
(npx tree-sitter-cli), the kv grammar in tree-sitter's DSL (the
repo root of the run: grammar.js with source = repeat(pair),
pair = seq(/[a-z0-9]+/, '=', /[0-9]+/, ';') and a generated parser -
`tree-sitter generate`), and the LCG from tests/recovery_metrics.rs.
The same
seeded corruption stream, the same kv language, tree-sitter's own
error recovery. Recovered = the tree's coverage extends past the
first error: at least one clean `pair` node begins after the error
node's end."""
import json, subprocess, re, os

class Lcg:
    def __init__(self, seed): self.state = seed
    def next_u64(self):
        self.state = ((self.state * 6364136223846793005) + 1442695040888963407) & 0xFFFFFFFFFFFFFFFF
        return self.state >> 11

KINDS = ["delete", "substitute", "insert", "duplicate"]
def corrupt(text, seed, count):
    rng = Lcg(0x5EED0000 + seed)
    data = list(text.encode())
    for _ in range(count):
        if not data: return bytes([120]).decode()
        kind = KINDS[rng.next_u64() % 4]
        pos = rng.next_u64() % len(data)
        while (data[pos] & 0xC0) == 0x80 and pos > 0: pos -= 1
        if kind == "delete": del data[pos]
        elif kind == "substitute": data[pos] = 122
        elif kind == "insert": data.insert(pos, 122)
        else: data.insert(pos, data[pos])
    return bytes(data).decode("utf-8", errors="replace")

clean = "a=1;b=2;"
# tree-sitter's kv: pairs separated implicitly; the parsanol fixture's
# clean input parses to two pairs.
def tree(text):
    open("/tmp/ts-input.txt", "w").write(text)
    out = subprocess.run(
        ["npx", "--yes", "tree-sitter-cli", "parse", "/tmp/ts-input.txt"],
        capture_output=True, text=True, cwd="/tmp/ts-kv")
    return out.stdout + out.stderr

def recovered(sexp):
    # The error node's end offset, then a clean pair starting after it.
    err = re.search(r"\(ERROR \[0, \d+\] - \[0, (\d+)\)", sexp)
    missing = "MISSING" in sexp
    pairs = [int(a) for a in re.findall(r"\(pair \[0, (\d+)\] - \[0, \d+\)", sexp)]
    if err is None and not missing:
        return True  # parsed clean
    limit = int(err.group(1)) if err else 0
    return any(start >= limit for start in pairs)

# The clean tree's pair count is the precision denominator (2).
results = {"clean": 0, "recovered": 0, "abandoned": 0, "precision_sum": 0.0, "precision_n": 0}
for seed in range(64):
    corrupted = corrupt(clean, seed, 1 + (seed % 3))
    sexp = tree(corrupted)
    if recovered(sexp):
        results["recovered"] += 1
        kept = len(re.findall(r"\(pair \[", sexp))
        results["precision_sum"] += min(kept, 2) / 2
        results["precision_n"] += 1
    else:
        results["abandoned"] += 1
sexp = tree(clean)
if "(pair" in sexp and "ERROR" not in sexp: results["clean"] = 1
results["precision"] = round(results["precision_sum"] / results["precision_n"], 2) if results["precision_n"] else None
print(json.dumps(results))
