const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm');
function page(initiallySignedIn=false){
 class Element {constructor(id=''){this.id=id;this.children=[];this.value='';this.textContent='';this.dataset={};this.classList={toggle(){}};}append(...nodes){this.children.push(...nodes);}replaceChildren(...nodes){this.children=nodes;}setAttribute(){}focus(){this.focused=true;}querySelector(){return null;}pause(){}load(){}removeAttribute(){}get options(){return this.children;}}
 const elements=new Map(),get=id=>{if(!elements.has(id))elements.set(id,new Element(id));return elements.get(id);};
 let signedIn=initiallySignedIn,failPlaylists=false;
 const calls=[];
 const response=(status,data)=>({status,ok:status>=200&&status<300,text:async()=>typeof data==='string'?data:JSON.stringify(data),json:async()=>data});
 const fetch=async(url,options)=>{calls.push({url,options});assert.equal(options.credentials,'same-origin');if(url==='/api/studio/session'){if(options.headers['x-amp-password']!=='pairing-code')return response(401,'');signedIn=true;return response(200,{signed_in:true});}if(url==='/api/studio/session/logout'){signedIn=false;return response(204,{});}if(url==='/api/status')return response(200,{running:false,listeners:0});if(!signedIn)return response(401,'');if(url==='/api/studio/bootstrap')return response(200,{capabilities:{library:true}});if(url==='/api/station/playlists')return failPlaylists?response(503,{error:'Saved playlists are busy. Wait a moment and retry.'}):response(200,{playlists:[]});if(url.startsWith('/api/studio/tracks'))return response(200,{tracks:[],total:0});throw Error('Unexpected route '+url);};
 const sandbox={document:{getElementById:get,createElement:()=>new Element(),querySelectorAll:()=>[]},window:{addEventListener(){},confirm:()=>true},location:{hash:''},history:{replaceState(){}},fetch,setTimeout(){},Audio:Element,URL,console};
 vm.runInNewContext(fs.readFileSync(__dirname+'/studio.js','utf8'),sandbox);
 return {get,calls,setFail:()=>{failPlaylists=true;},settle:()=>new Promise(resolve=>setImmediate(resolve))};
}
test('initial unauthorized opens sign-in without password retry loop; login clears password and refresh uses cookie',async()=>{
 const p=page();await p.settle();assert.equal(p.get('access-panel').open,true);assert.match(p.get('message').textContent,/Sign in/);assert.equal(p.calls.filter(c=>c.url==='/api/studio/session').length,0);
 p.get('password').value='pairing-code';await p.get('access').onsubmit({preventDefault(){}});await p.settle();assert.equal(p.get('password').value,'');assert.equal(p.get('access-panel').open,false);assert.match(p.get('connection').textContent,/Signed in/);
 const afterLogin=p.calls.slice(p.calls.findIndex(c=>c.url==='/api/studio/session')+1);assert(afterLogin.every(c=>!Object.hasOwn(c.options.headers,'x-amp-password')));assert(p.calls.every(c=>!c.url.includes('pairing-code')&&!c.url.includes('ticket=')));
 const refreshed=page(true);await refreshed.settle();assert.equal(refreshed.get('access-panel').open,false);assert(refreshed.calls.every(c=>!Object.hasOwn(c.options.headers,'x-amp-password')));
 await p.get('sign-out').onclick();assert.equal(p.get('access-panel').open,true);assert.match(p.get('message').textContent,/Signed out/);
});
test('structured server failure remains actionable after successful authentication',async()=>{
 const p=page(true);await p.settle();p.setFail();await p.get('navigation').children.find(b=>b.dataset.module==='cast').onclick();await p.settle();assert.equal(p.get('message').textContent,'Saved playlists are busy. Wait a moment and retry.');assert.equal(p.get('access-panel').open,false);
});
