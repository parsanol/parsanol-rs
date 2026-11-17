# The 5-minute quickstart

Every command below is verified by CI against the **published**
packages (the snippet-rot gate: `.github/workflows/quickstart-rot.yml`
runs this page's commands on every main push — the docs cannot rot).

One grammar — a four-function calculator — compiled once, then parsed
from every runtime through the same checksummed artifact.

## 0. The grammar

Save as `calculator.parg`:

```parg
grammar Calculator version "1.0.0" {
  expression = (term / digit) as expression
  term = (digit ws "+" ws expression) as term
  digit = (1*%x30-39) as digit
  ws = *( %x20 )
  entry main: expression
}
```

## 1. Compile it (the checksummed artifact)

```sh
gem install parsanol
parsanol parg compile calculator.parg -o calculator.json
# calculator.json  sha256:553da414…  warnings=0
```

The artifact embeds the grammar's tests, the tables, and a checksum
covering everything — every engine below verifies it before parsing,
and every engine builds byte-identical trees.

## 2. Parse from Ruby

```sh
gem install parsanol
ruby -r parsanol -e '
a = Parsanol::PARG::Artifact.from_json(File.read("calculator.json"))
puts a.parse("main", "2+3").inspect
'
# {expression: {term: {digit: "2"@0, expression: {digit: "3"@2}}}}
```

## 3. Parse from TypeScript (Node >= 22)

```sh
npm install parsanol
node --input-type=module -e '
import { PargRuntime } from "parsanol";
const rt = PargRuntime.fromFile("calculator.json");
console.log(JSON.stringify(rt.parseShape("2+3")));
'
```

## 4. Parse from Python

```sh
pip install parsanol
python3 -c '
from parsanol import PargArtifact
a = PargArtifact.from_path("calculator.json")
print(a.parse("2+3"))
'
```

## 5. Parse from Rust

```sh
cargo add parsanol
```

```rust,ignore
let artifact_text = std::fs::read_to_string("calculator.json")?;
let artifact = parsanol::PargArtifact::from_json(&artifact_text)?;
let shape = artifact.parse_shape("main", "2+3")?;
println!("{shape}");
```

## What just happened

- One grammar, one checksummed artifact — **sealed**: the checksum
  attests every byte any engine will read.
- Four runtimes — **byte-identical trees**: the Ruby tree above and
  the Node/Python/Rust trees compare equal, per keystroke, per case,
  in CI.
- The same artifact runs the grammar's embedded tests, drives the
  recovery envelopes on corrupted input, and renders back
  (generation-in-reverse: the parse tree is the document).

Next: [BENCHMARKS.md](BENCHMARKS.md) for the industry head-to-head,
[PARG-SHAPE-v2.md](PARSANOL-SHAPE-v2.md) for the artifact contract,
and the `flavors/` registry for forty-plus shipping grammars.
