// Run against wasm-bindgen --target web output in a real browser or Node.
// Fixture mirrors portable/parser/compatibility_tests.rs's causal lookahead test.
export async function runCompatibilityChecks(engine) {
  const checks=[];
  const assert=(ok,label)=>{if(!ok)throw Error(label);checks.push(label);};
  const canonical=value=>Array.isArray(value)?value.map(canonical):value&&typeof value==='object'?
    Object.fromEntries(Object.keys(value).sort().map(key=>[key,canonical(value[key])])):value;
  for (const [prefix,input,causal] of [['a','ab?',1],['α','αb?',2]]) {
    const envelope={shape:'parsanol-tree/v2',grammar:'DiagnosticPolicy',version:'1.0.0',
      default_entry:'main',entries:{main:{root:'main',grammar:{root:6,atoms:[
        {Str:{pattern:prefix+'b'}},{Str:{pattern:'c'}},{Sequence:{atoms:[0,1]}},
        {Lookahead:{atom:2,positive:false}},{Str:{pattern:prefix}},{Str:{pattern:'!'}},
        {Sequence:{atoms:[3,4,5]}}]}}}};
    const digest=await crypto.subtle.digest('SHA-256',new TextEncoder().encode(JSON.stringify(canonical(envelope))));
    envelope.checksum='sha256:'+Array.from(new Uint8Array(digest),byte=>byte.toString(16).padStart(2,'0')).join('');
    const artifact=new engine.PargArtifactJs(JSON.stringify(envelope));
    try {
      for (const [policy,offset] of [['strict',causal+1],['parslet',causal]]) {
        const result=JSON.parse(artifact.parseShapeWithCompatibility('main',input,policy));
        assert(result.ok===false && result.error.offset===offset,`${prefix} ${policy} byte offset`);
      }
      const accepted=JSON.parse(artifact.parseShapeWithCompatibility('main',prefix+'!','parslet'));
      assert(accepted.ok===true,`${prefix} valid input`);
      assert(JSON.stringify(accepted.shape)===artifact.parseShape('main',prefix+'!'),`${prefix} strict success unchanged`);
      let refused=false;try{artifact.parseShape('main',input);}catch{refused=true;}
      assert(refused,`${prefix} strict API still rejects`);
      let unknown=false;try{artifact.parseShapeWithCompatibility('main',input,'unknown');}catch{unknown=true;}
      assert(unknown,`${prefix} unknown policy rejected`);
      let entry=false;try{artifact.parseShapeWithCompatibility('missing',input,'parslet');}catch{entry=true;}
      assert(entry,`${prefix} unknown entry rejected`);
    } finally {artifact.free();}
  }
  // Generic recursive source fixture exercises the same supported rule budget
  // without parsing or scanning mathematical input in the consumer.
  const recursive={shape:'parsanol-tree/v2',grammar:'Recursive',version:'1.0.0',default_entry:'main',entries:{main:{root:'main',grammar:{root:0,atoms:[{Entity:{atom:1}},{Alternative:{atoms:[2,5]}},{Sequence:{atoms:[3,0,4]}},{Str:{pattern:'('}},{Str:{pattern:')'}},{Str:{pattern:'x'}}]}}}};
  const recursiveDigest=await crypto.subtle.digest('SHA-256',new TextEncoder().encode(JSON.stringify(canonical(recursive))));
  recursive.checksum='sha256:'+Array.from(new Uint8Array(recursiveDigest),byte=>byte.toString(16).padStart(2,'0')).join('');
  const nested=new engine.PargArtifactJs(JSON.stringify(recursive));
  try {
    for(const depth of [64,128,256]){
      nested.parseShape('main','('.repeat(depth)+'x'+')'.repeat(depth));
      nested.parseShape('main','x');checks.push('valid nesting and reuse '+depth);
    }
    const excessive='('.repeat(1200)+'x'+')'.repeat(1200);
    for(const method of ['parseShape','parseShapeRecovering']){
      let resource;try{nested[method]('main',excessive);}catch(error){resource=error;}
      assert(resource?.category==='recursion_limit'&&resource.max_depth===1000&&resource.name==='RecursionLimitExceeded',method+' typed guard');
      nested.parseShape('main','x');checks.push(method+' resource reuse');
    }
    const resource=JSON.parse(nested.parseShapeWithCompatibility('main',excessive,'strict'));
    assert(resource.ok===false&&resource.error.category==='recursion_limit'&&resource.error.offset===null,'compatibility typed guard');
    nested.parseShape('main','x');checks.push('compatibility resource reuse');
  } finally {nested.free();}
  return checks;
}
