// Focused regression of the same target-web selector used in the browser.
import {readFileSync} from 'node:fs';
import {resolve,join} from 'node:path';
import {pathToFileURL} from 'node:url';
import {runCompatibilityChecks} from '../parsanol/tests/wasm/compatibility.mjs';
const directory=resolve(process.argv[2]);
const engine=await import(pathToFileURL(join(directory,'parsanol.js')).href);
await engine.default({module_or_path:readFileSync(join(directory,'parsanol_bg.wasm'))});
const checks=await runCompatibilityChecks(engine);
console.log(JSON.stringify({ok:true,node:process.version,checks}));
