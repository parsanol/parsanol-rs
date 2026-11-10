#!/usr/bin/env python3
"""Run the explicit selector through real target-web bindings in headless Chrome."""
from pathlib import Path
import argparse,html,http.server,json,os,re,shutil,subprocess,tempfile,threading,hashlib
ROOT=Path(__file__).resolve().parents[1]
PAGE='<!doctype html><meta charset="utf-8"><body>pending<script type="module">\nimport initialize,* as engine from \'/wasm/parsanol.js\';\nimport {runCompatibilityChecks} from \'/compatibility.mjs\';\ntry {\n await initialize();\n const checks=await runCompatibilityChecks(engine);\n const response=await fetch(\'/artifact.json\');\n if(response.ok){\n  const artifact=new engine.PargArtifactJs(await response.text());\n  try {\n   for(const [text,strict,causal]of [[\'a/\',2,1],[\'α a/\',5,4]]){\n    for(const [mode,offset]of [[\'strict\',strict],[\'parslet\',causal]]){\n     const result=JSON.parse(artifact.parseShapeWithCompatibility(\'dot\',text,mode));\n     if(result.ok!==false||result.error.offset!==offset)throw Error(text+\' \'+mode+\' offset\');\n     checks.push(\'pinned Amath \'+text+\' \'+mode+\' offset\');\n    }\n   }\n   for(const depth of [64,128,256]){\n    const text=\'(\'.repeat(depth)+\'x\'+\')\'.repeat(depth);\n    for(const method of [\'parseShape\',\'parseShapeRecovering\',\'parseShapeWithCompatibility\']){\n     let value,error;try{value=method===\'parseShapeWithCompatibility\'?JSON.parse(artifact[method](\'dot\',text,\'parslet\')):artifact[method](\'dot\',text);}catch(failure){error=failure;}\n     if(depth===64){if(error||value?.ok===false)throw Error(\'valid Amath depth \'+depth+\' \'+method);}\n     else if(method===\'parseShapeWithCompatibility\'){\n      if(value?.error?.category!==\'call_depth_limit\'||value.error.max_depth!==1024||value.error.offset!==null)throw Error(\'Amath typed guard \'+method);\n     }else if(error?.category!==\'call_depth_limit\'||error.max_depth!==1024||error.name!==\'CallDepthLimitExceeded\')throw Error(\'Amath typed guard \'+method);\n     checks.push(\'Amath depth \'+depth+\' \'+method);\n     artifact.parseShape(\'dot\',\'x\');checks.push(\'Amath depth reuse \'+depth+\' \'+method);\n    }\n   }\n   for(const text of [\'sqrt(x)\',\'\']){\n    const result=JSON.parse(artifact.parseShapeWithCompatibility(\'dot\',text,\'parslet\'));\n    if(!result.ok)throw Error(\'valid Amath \'+text);checks.push(\'pinned Amath success \'+text);\n   }\n  } finally {artifact.free();}\n }\n document.body.textContent=JSON.stringify({ok:true,checks});\n} catch(error){document.body.textContent=JSON.stringify({ok:false,error:String(error),stack:error?.stack});}\n</script>'

def main():
 parser=argparse.ArgumentParser();parser.add_argument('--web-dir',type=Path,required=True);parser.add_argument('--artifact',type=Path);parser.add_argument('--evidence-dir',type=Path,required=True);args=parser.parse_args()
 browser=os.environ.get('CHROME_BIN')or shutil.which('google-chrome')or shutil.which('chromium')or shutil.which('chromium-browser')
 if not browser:raise RuntimeError('real Chrome/Chromium required')
 args.evidence_dir.mkdir(parents=True,exist_ok=True);requests=[]
 class Handler(http.server.BaseHTTPRequestHandler):
  def do_GET(self):
   path=None;content_type='text/javascript';body=None
   if self.path=='/':body=PAGE.encode();content_type='text/html; charset=utf-8'
   elif self.path=='/compatibility.mjs':path=ROOT/'parsanol/tests/wasm/compatibility.mjs'
   elif self.path=='/artifact.json'and args.artifact:path=args.artifact;content_type='application/json'
   elif self.path.startswith('/wasm/'):
    candidate=(args.web_dir/self.path.removeprefix('/wasm/')).resolve()
    if candidate.is_relative_to(args.web_dir.resolve()):path=candidate
    if self.path.endswith('.wasm'):content_type='application/wasm'
   if path and path.is_file():body=path.read_bytes()
   requests.append(dict(path=self.path,status=200 if body is not None else 404))
   if body is None:self.send_error(404);return
   self.send_response(200);self.send_header('Content-Type',content_type);self.end_headers();self.wfile.write(body)
  def log_message(self,*args):pass
 server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler);threading.Thread(target=server.serve_forever,daemon=True).start()
 try:
  with tempfile.TemporaryDirectory(prefix='parsanol-compatibility-chrome-')as temporary:
   command=[browser,'--headless','--no-sandbox','--disable-gpu','--disable-dev-shm-usage','--disable-background-networking','--disable-extensions','--no-first-run','--user-data-dir='+temporary,'--virtual-time-budget=45000','--dump-dom',f'http://127.0.0.1:{server.server_port}/']
   result=subprocess.run(command,capture_output=True,text=True,timeout=120)
 finally:server.shutdown()
 (args.evidence_dir/'chrome.html').write_text(result.stdout);(args.evidence_dir/'chrome.stderr').write_text(result.stderr)
 match=re.search(r'<body[^>]*>(.*?)</body>',result.stdout,re.S)
 if not match:raise RuntimeError('Chrome returned no document body')
 report=json.loads(html.unescape(match.group(1)));report.update(browser=subprocess.check_output([browser,'--version'],text=True).strip(),browser_exit=result.returncode,requests=requests,files={p.name:hashlib.sha256(p.read_bytes()).hexdigest()for p in args.web_dir.iterdir()if p.is_file()})
 if args.artifact:report['artifact_sha256']=hashlib.sha256(args.artifact.read_bytes()).hexdigest()
 (args.evidence_dir/'result.json').write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n');print(json.dumps(report,ensure_ascii=False))
 if result.returncode or not report['ok']:raise SystemExit(1)
if __name__=='__main__':main()
