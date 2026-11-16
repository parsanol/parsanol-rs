// The tree-sitter runner for the industry benchmark suite (rs#526).
//
// Usage: run_tree_sitter.js <corpus-dir> <rounds>
//
// Requires: npm i tree-sitter tree-sitter-json (the C grammar compiled
// by node-gyp — the canonical tree-sitter usage). Prints the same line
// protocol as run_parsanol.
const Parser = require("tree-sitter");
const JSONGrammar = require("tree-sitter-json");
const fs = require("fs");
const path = require("path");

function main() {
  const [corpusDir, roundsArg] = process.argv.slice(2);
  if (!corpusDir || !roundsArg) {
    console.error("usage: run_tree_sitter.js <corpus-dir> <rounds>");
    process.exit(2);
  }
  const rounds = parseInt(roundsArg, 10);
  const parser = new Parser();
  parser.setLanguage(JSONGrammar);

  const docs = fs
    .readdirSync(corpusDir)
    .filter((f) => f.endsWith(".json"))
    .sort()
    .map((f) => {
      const text = fs.readFileSync(path.join(corpusDir, f), "utf8");
      // validity cross-check: the corpus must be real JSON
      JSON.parse(text);
      return { name: f, text };
    });
  if (docs.length === 0) {
    console.error("no corpus documents");
    process.exit(2);
  }

  // warmup + identity: the tree must cover the whole document
  let totalBytes = 0;
  for (const { text } of docs) {
    totalBytes += text.length;
    const tree = parser.parse(text);
    if (tree.rootNode.hasError || tree.rootNode.endIndex !== text.length) {
      console.error("tree-sitter failed to parse a corpus doc cleanly");
      process.exit(1);
    }
  }

  let totalNs = 0n;
  for (const { name, text } of docs) {
    let best = Infinity;
    for (let i = 0; i < rounds; i++) {
      const t = process.hrtime.bigint();
      const tree = parser.parse(text);
      if (tree.rootNode.hasError) process.exit(1);
      const ns = Number(process.hrtime.bigint() - t);
      if (ns < best) best = ns;
    }
    totalNs += BigInt(best);
    console.log(`${name} ${text.length} ${best}`);
  }
  console.log(`TOTAL ${totalBytes} ${totalNs}`);
}

main();
