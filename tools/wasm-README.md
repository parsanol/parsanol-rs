# @parsanol/wasm

Parsanol PEG parser — WebAssembly bindings (checksummed
parsanol-tree/v2 artifacts; parse, shape, recovery, bindings,
schema types).

## Browser

```js
import init, { PargArtifactJs } from "@parsanol/wasm";
await init(); // fetches parsanol_bg.wasm next to the module
const parg = new PargArtifactJs(artifactJson);
const bound = parg.parseAndBind("identifier", "iso 12345");
```

## Node

The default init fetches relative to the module, which Node's fetch
does not serve; read the bytes and hand them to `initSync` (plain
`initSync(bytes)` works but warns as deprecated):

```js
import { initSync, PargArtifactJs } from "@parsanol/wasm";
import { readFileSync } from "node:fs";

initSync({ module: readFileSync("node_modules/@parsanol/wasm/parsanol_bg.wasm") });
const parg = new PargArtifactJs(artifactJson);
```

## Notes

- `parseAndBind` returns `{}` when the entry declares no
  `bindings` block — the parse succeeded, nothing was bound.
- The async bootstrap is the package DEFAULT export
  (`__wbg_init(module_or_path?)`); `setPanicHook` installs the panic
  hook directly and is called by the bootstrap.
