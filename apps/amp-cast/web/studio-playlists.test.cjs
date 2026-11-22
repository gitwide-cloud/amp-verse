'use strict';
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm');
const clone=value=>JSON.parse(JSON.stringify(value));
class Element {
 constructor(){this.children=[];this.dataset={};this.style={};this.value='';this.hidden=false;this.classList={toggle(){}};this.paused=true;this.duration=100;this.readyState=1;}
 append(...nodes){this.children.push(...nodes)}replaceChildren(...nodes){this.children=nodes}setAttribute(k,v){this[k]=v}focus(){}pause(){this.paused=true}querySelectorAll(){return []}get options(){return this.children}
}
function page(){
 const html=fs.readFileSync(__dirname+'/studio.html','utf8'),elements=new Map([...html.matchAll(/id="([^"]+)"/g)].map(m=>[m[1],new Element()]));
 const get=id=>{assert(elements.has(id),'HTML is missing '+id);return elements.get(id)};
 const normal={id:'normal_saved',name:'Ordered saved',kind:'playlist',revision:{revision:4},tracks:[0,0],track_paths:['known.mp3','missing/song.mp3','known.mp3'],companion_available:true};
 const rules={id:'demo',name:'Smart saved',style:'steady',genres:['R&B','club music shit','club'],target_energy:null,filters:[{type:'Flag',field:'Analyzed',value:true,future_filter:{keep:1}}],playlist_sources:[{name:'Opaque source',songs:[],future_source:{keep:2}}],energy_progression:{target:7,transitions:4,hold:true,arc:'Linear',future_hint:{keep:3}},future_rule:{keep:4}};
 let data={playlists:[normal,{id:'smart_demo',name:rules.name,kind:'smart',tracks:[0],rules:clone(rules)}],smart_revision:{version:2,_amp_sync:{revision:9},future_container:{keep:5},playlists:[rules]}};
 const requests=[];let rejectSave=false,confirm=false;
 const response=(status,value)=>({ok:status<400,status,json:async()=>clone(value),text:async()=>JSON.stringify(value)});
 const fetch=async(url,options={})=>{requests.push({url,options});
  if(url==='/api/studio/bootstrap')return response(200,{library_id:'library',password_required:false,capabilities:{}});
  if(url.startsWith('/api/studio/tracks?'))return response(200,{tracks:[{id:20573,path:'far/song-20573.mp3',title:'Outside any initial catalog page'}],total:20574});
  if(url==='/api/status')return response(200,{listeners:0,running:false});
  if(url==='/api/studio/playlists'){
   if(options.method==='POST')return response(rejectSave?409:200,rejectSave?{error:'Playlist changed; reload before saving'}:{id:JSON.parse(options.body).id});
   return response(200,data);
  }
  throw Error('Unexpected request '+url);
 };
 const context={document:{getElementById:get,createElement:()=>new Element(),querySelectorAll:()=>[]},window:{confirm:()=>confirm,addEventListener(){}},fetch,location:{hash:''},history:{replaceState(){}},setTimeout(){},Audio:Element,URL,console};
 vm.runInNewContext(fs.readFileSync(__dirname+'/studio.js','utf8'),context);
 return {get,requests,data,rules,normal,setReject:value=>rejectSave=value,setConfirm:value=>confirm=value,settle:async()=>{for(let i=0;i<5;i++)await new Promise(resolve=>setImmediate(resolve));}};
}
const edits=p=>p.get('saved-playlists').children.map(row=>row.children.find(n=>n.onclick));
const posts=p=>p.requests.filter(r=>r.url==='/api/studio/playlists'&&r.options.method==='POST').map(r=>JSON.parse(r.options.body));

test('Library lists saved playlists before songs and normal edit retains every path, order and duplicate across refresh/conflict',async()=>{
 const html=fs.readFileSync(__dirname+'/studio.html','utf8');assert(html.indexOf('id="saved-playlists"')<html.indexOf('id="tracks"'));
 const p=page();await p.settle();assert.equal(edits(p).length,2);await edits(p)[0].onclick();assert.equal(p.get('saved-playlist-entries').children.length,3);
 p.get('saved-playlist-name').value='Changed name';p.get('saved-playlist-name').oninput();
 await p.get('saved-playlists-refresh').onclick();assert.equal(p.get('saved-playlist-name').value,'Changed name','list refresh cannot replace open draft');
 await edits(p)[1].onclick();assert.equal(p.get('saved-playlist-name').value,'Changed name','declined switch keeps the original draft');
 const middle=p.get('saved-playlist-entries').children[1];await middle.children[1].children.find(n=>n.textContent==='Move up').onclick();
 const libraryButtons=p.get('tracks').children[0].children[3].children[0].children;await libraryButtons.find(n=>n.textContent==='Add to playlist').onclick();
 p.setReject(true);await p.get('saved-playlist-form').onsubmit({preventDefault(){}});
 assert.equal(p.get('saved-playlist-editor').hidden,false);assert.equal(p.get('saved-playlist-name').value,'Changed name');assert.equal(p.get('saved-playlist-entries').children.length,4);assert.match(p.get('message').textContent,/reload/);
 const rejected=posts(p).at(-1);assert.deepEqual(rejected.track_paths,['missing/song.mp3','known.mp3','known.mp3','far/song-20573.mp3']);assert.deepEqual(rejected.revision,{revision:4});assert.equal(rejected.library_id,'library');
 p.setReject(false);await p.get('saved-playlist-form').onsubmit({preventDefault(){}});assert.equal(p.get('saved-playlist-editor').hidden,true);assert.deepEqual(posts(p).at(-1).track_paths,rejected.track_paths);
 assert(!p.requests.some(r=>/\/api\/(player|start|stop|station\/play)/.test(r.url)),'editing must never change playback');
});

test('Smart rename keeps every genre and nested opaque rule with captured container revision',async()=>{
 const p=page();await p.settle();await edits(p)[1].onclick();assert.equal(p.get('saved-playlist-genres').value,'R&B\nclub music shit\nclub');
 p.get('saved-playlist-name').value='Smart renamed';p.get('saved-playlist-name').oninput();
 await p.get('saved-playlists-refresh').onclick();p.data.smart_revision._amp_sync.revision=10;
 await p.get('saved-playlist-form').onsubmit({preventDefault(){}});const saved=posts(p).at(-1);
 assert.equal(saved.id,'smart_demo');assert.equal(saved.revision._amp_sync.revision,9);assert.deepEqual(saved.rules.genres,p.rules.genres);
 for(const field of ['filters','playlist_sources','energy_progression','future_rule'])assert.deepEqual(saved.rules[field],p.rules[field]);
 assert.equal(saved.rules.name,'Smart renamed');assert.equal(saved.rules.id,'demo');
});

test('Cancel/New require explicit discard and long names use a short accessible Edit action',async()=>{
 const p=page();await p.settle();p.normal.name='<script>'+ 'x'.repeat(180);await p.get('saved-playlists-refresh').onclick();
 assert.equal(edits(p)[0].textContent,'Edit');assert.equal(edits(p)[0]['aria-label'],'Edit saved playlist '+p.normal.name);
 await edits(p)[0].onclick();p.get('saved-playlist-name').value='Unsaved';p.get('saved-playlist-name').oninput();
 p.get('saved-playlist-new').onclick();assert.equal(p.get('saved-playlist-editor').hidden,false);
 p.setConfirm(true);p.get('saved-playlist-cancel').onclick();assert.equal(p.get('saved-playlist-editor').hidden,true);assert.equal(posts(p).length,0);
});
