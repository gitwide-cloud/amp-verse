const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm');
function page(initiallySignedIn=false,emptyFile=false,googleCast=false,serverSpeakers=false){
 class Element {constructor(id=''){this.id=id;this.children=[];this.value='';this.textContent='';this.dataset={};this.classList={toggle(){}};}append(...nodes){this.children.push(...nodes);}replaceChildren(...nodes){this.children=nodes;}setAttribute(){}focus(){this.focused=true;}querySelector(){return null;}pause(){}async play(){this.played=true;}load(){}removeAttribute(){}get options(){return this.children;}}
 const elements=new Map(),get=id=>{if(!elements.has(id))elements.set(id,new Element(id));return elements.get(id);};
 let signedIn=initiallySignedIn,failPlaylists=false,serverPlaying=false;
 const calls=[];
 const response=(status,data)=>({status,ok:status>=200&&status<300,text:async()=>typeof data==='string'?data:JSON.stringify(data),json:async()=>data});
 const fetch=async(url,options)=>{calls.push({url,options});assert.equal(options.credentials,'same-origin');if(url==='/api/studio/session'){if(options.headers['x-amp-password']!=='pairing-code')return response(401,'');signedIn=true;return response(200,{signed_in:true});}if(url==='/api/studio/session/logout'){signedIn=false;return response(204,{});}if(url==='/api/status')return response(200,{running:serverPlaying,source:serverPlaying?'player_queue':'',server_output:serverSpeakers,listeners:0,now_playing:serverPlaying?'Generated song':null});if(!signedIn)return response(401,'');if(url==='/api/studio/bootstrap')return response(200,{capabilities:{library:true,google_cast_server_owned:googleCast},library_id:'fixture-library'});if(url==='/api/google-cast/discover')return response(200,{devices:[{id:'speaker-fixture',name:'Generated kitchen speaker'}]});if(url==='/api/google-cast/start')return response(200,{session:{state:'loading',message:'Waiting for receiver'}});if(url==='/api/google-cast/status')return response(200,{server_owned:true,session:{state:'loading',device:'Generated kitchen speaker',message:'Waiting for receiver playback confirmation'}});if(url==='/api/google-cast/stop')return response(200,{session:{state:'stopping'}});if(url==='/api/studio/playlists')return failPlaylists?response(503,{error:'Saved playlists are busy. Wait a moment and retry.'}):response(200,{playlists:[],smart_revision:{playlists:[]}});if(url==='/api/station/playlists')return failPlaylists?response(503,{error:'Saved playlists are busy. Wait a moment and retry.'}):response(200,{playlists:[]});if(url.startsWith('/api/player/audio?')&&options.method==='HEAD')return response(emptyFile?422:200,{});if(url.startsWith('/api/player/playlist?'))return response(200,{library_id:'fixture-library',kind:'playlist',selection_mode:'ordered',tracks:[{path:'artist/song.wav',title:'Generated song'},{path:'artist/song.wav',title:'Generated song'}]});if(url==='/api/player/play'){serverPlaying=true;return response(204,{});}if(url==='/api/player/queue')return response(204,{});if(url==='/api/stop'){serverPlaying=false;return response(204,{});}if(url.startsWith('/api/studio/tracks'))return response(200,{tracks:emptyFile?[{id:0,path:'empty.wav',title:'Empty fixture'}]:[{id:17,path:'artist/song.wav',title:'Generated song'}],total:1});throw Error('Unexpected route '+url);};
 const sandbox={document:{getElementById:get,createElement:()=>new Element(),querySelectorAll:()=>[]},window:{addEventListener(){},confirm:()=>true},location:{hash:''},history:{replaceState(){}},fetch,AbortController,clearTimeout,setTimeout(){},Audio:Element,URL,console};
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

test('selecting an empty audio file gives repair guidance instead of starting silent playback',async()=>{
 const p=page(true,true);await p.settle();
 function find(n){if(n.textContent==='Play'&&n.onclick)return n;for(const c of n.children||[]){const result=find(c);if(result)return result;}}
 const button=find(p.get('tracks'));assert(button);await button.onclick();await p.settle();
 assert.match(p.get('message').textContent,/audio file is empty.*repair/i);
 assert(p.calls.some(c=>c.url==='/api/player/audio?library_id=fixture-library&path=empty.wav'&&c.options.method==='HEAD'));
 assert.equal(p.get('player-audio').src,undefined);
 assert.equal(p.get('access-panel').open,false);
});

test('Library Play and Queue honor Server speakers with stable paths and explicit Stop',async()=>{
 const p=page(true);await p.settle();p.get('player-output').value='server';p.get('player-output').onchange();assert.equal(p.get('remote-output').value,'server');
 const buttons=p.get('tracks').children[0].children[3].children[0].children;
 await buttons.find(b=>b.textContent==='Play').onclick();
 const play=p.calls.find(c=>c.url==='/api/player/play');assert.deepEqual(JSON.parse(play.options.body),{library_id:'fixture-library',tracks:['artist/song.wav'],server_speakers:true});assert.equal(p.get('player-audio').played,undefined);
 await buttons.find(b=>b.textContent==='Queue').onclick();const queue=p.calls.find(c=>c.url==='/api/player/queue');assert.deepEqual(JSON.parse(queue.options.body),{library_id:'fixture-library',tracks:['artist/song.wav']});
 await p.get('player-server-stop').onclick();assert(p.calls.some(c=>c.url==='/api/stop'));assert.equal(p.get('message').textContent,'Server playback stopped.');
});

test('This device saved playlist streams privately without changing the global station',async()=>{
 const p=page(true);await p.settle();p.get('player-output').value='device';p.get('player-playlist').value='normal_fixture';await p.get('player-playlist-start').onclick();assert(p.calls.some(c=>c.url==='/api/player/playlist?library_id=fixture-library&playlist_id=normal_fixture'));assert(!p.calls.some(c=>c.url==='/api/player/play'||c.url==='/api/stop'));assert.equal(p.get('player-audio').played,true);assert.equal(p.get('player-audio').src,'/api/player/audio?library_id=fixture-library&path=artist%2Fsong.wav');assert.equal(p.get('queue').children.length,1);
});

test('Google Cast controls use server discovery/lifecycle and never browser playback or source takeover',async()=>{
 const p=page(true,false,true);await p.settle();assert.equal(p.get('google-cast-panel').hidden,false);
 await p.get('google-cast-find').onclick();assert.equal(p.get('google-cast-device').children[0].textContent,'Generated kitchen speaker');p.get('google-cast-device').value='speaker-fixture';
 await p.get('google-cast-start').onclick();const start=p.calls.find(c=>c.url==='/api/google-cast/start');assert.deepEqual(JSON.parse(start.options.body),{device_id:'speaker-fixture'});assert.match(p.get('google-cast-status').textContent,/Waiting for receiver/);assert.doesNotMatch(p.get('message').textContent,/Playing on speaker/);
 await p.get('google-cast-stop').onclick();assert(p.calls.some(c=>c.url==='/api/google-cast/stop'));assert(!p.calls.some(c=>c.url==='/api/player/play'||c.url==='/api/start'||c.url==='/api/stop'));assert.equal(p.get('player-audio').played,undefined);assert.equal(p.get('station-audio').played,undefined);assert(p.calls.every(c=>!c.url.includes('stream/')&&!c.url.includes('password=')));assert.match(p.get('message').textContent,/Server program.*kept/);
});

test('Cast Broadcast playlist always targets server even while Player chooses this device',async()=>{
 const p=page(true);await p.settle();p.get('player-output').value='device';p.get('station-playlist').value='smart_RnB';await p.get('station-start').onclick();
 const request=p.calls.find(c=>c.url==='/api/player/play');assert.deepEqual(JSON.parse(request.options.body),{library_id:'fixture-library',playlist_id:'smart_RnB',shuffle:false,server_speakers:false});assert.equal(p.get('player-output').value,'device');assert.equal(p.get('player-audio').played,undefined);assert(!p.calls.some(c=>c.url.startsWith('/api/player/playlist?')));
});

test('Cast Broadcast preserves existing physical server speakers while Player remains local',async()=>{
 const p=page(true,false,false,true);await p.settle();p.get('player-output').value='device';p.get('station-playlist').value='smart_RnB';await p.get('station-start').onclick();assert.equal(JSON.parse(p.calls.find(c=>c.url==='/api/player/play').options.body).server_speakers,true);assert.equal(p.get('player-audio').played,undefined);
});

{
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm');
const source=fs.readFileSync(__dirname+'/studio.js','utf8');
function client(fetch,extra={}){let signIns=0;const context={fetch,AbortController,setTimeout,clearTimeout,Date,Error,signInRequired(){signIns++;},...extra};vm.createContext(context);vm.runInContext(source.slice(source.indexOf('function catalogAbort('),source.indexOf('async function audioURL(')),context);return {api:context.api,signIns:()=>signIns};}
function response(status,data,retryAfter='0'){return {status,ok:status>=200&&status<300,headers:{get:name=>name==='Retry-After'?retryAfter:null},text:async()=>JSON.stringify(data),json:async()=>data};}
test('known catalog contention retries the same authenticated GET and recovers without any mutation',async()=>{const calls=[];const c=client(async(url,options)=>{calls.push({url,options});return calls.length===1?response(503,{error:'Cannot read playlists',code:'catalog_busy'}):response(200,{playlists:[{id:'normal_saved'}]});});const result=await c.api('studio/playlists');assert.equal(result.playlists[0].id,'normal_saved');assert.equal(calls.length,2);assert(calls.every(c=>c.url==='/api/studio/playlists'&&c.options.method==='GET'&&c.options.credentials==='same-origin'&&c.options.cache==='no-store'&&!c.options.body));});
test('busy retries stop at three attempts and retain the existing server error message',async()=>{let calls=0;const c=client(async()=>{calls++;return response(503,{error:'Cannot read playlists',code:'catalog_busy'});});await assert.rejects(c.api('studio/playlists'),{message:'Cannot read playlists'});assert.equal(calls,3);});
test('POST, other GETs, unauthorized, permanent errors and network errors are never retried',async()=>{for(const [path,body,status,data] of [['studio/playlists',{name:'draft'},503,{code:'catalog_busy',error:'Cannot read playlists'}],['studio/bootstrap',undefined,503,{code:'catalog_busy',error:'Cannot read playlists'}],['studio/playlists',undefined,401,{}],['studio/playlists',undefined,503,{code:'playlist_read_failed',kind:'parse',error:'Cannot read playlists'}],['studio/playlists',undefined,409,{error:'Conflict'}]]){let calls=0;const c=client(async()=>{calls++;return response(status,data);});await assert.rejects(c.api(path,body));assert.equal(calls,1);assert.equal(c.signIns(),status===401?1:0);}let calls=0;const c=client(async()=>{calls++;throw Error('private connection details');});await assert.rejects(c.api('studio/playlists'),/Cannot reach AMP Verse/);assert.equal(calls,1);});
test('unauthorized response after one transient retry immediately returns to sign in',async()=>{let calls=0;const c=client(async()=>++calls===1?response(503,{code:'catalog_busy'}):response(401,{}));await assert.rejects(c.api('studio/playlists'),/Sign in/);assert.equal(calls,2);assert.equal(c.signIns(),1);});
test('abort cancels backoff and in-flight GET without another request or sign-in',async()=>{for(const inFlight of [false,true]){const controller=new AbortController();let calls=0;const c=client(async(url,options)=>{calls++;if(!inFlight)return response(503,{code:'catalog_busy'});return new Promise((resolve,reject)=>options.signal.addEventListener('abort',()=>{const e=Error('cancelled');e.name='AbortError';reject(e);},{once:true}));});const request=c.api('studio/playlists',undefined,undefined,{signal:controller.signal});setTimeout(()=>controller.abort(),20);await assert.rejects(request,e=>e.name==='AbortError');assert.equal(calls,1);assert.equal(c.signIns(),0);}});
test('an already aborted read makes no request and elapsed budget prevents another retry',async()=>{const controller=new AbortController();controller.abort();let calls=0;let c=client(async()=>{calls++;return response(200,{});});await assert.rejects(c.api('studio/playlists',undefined,undefined,{signal:controller.signal}),e=>e.name==='AbortError');assert.equal(calls,0);let now=0;c=client(async()=>{calls++;now=11950;return response(503,{code:'catalog_busy',error:'Cannot read playlists'},'1');},{Date:{now:()=>now}});await assert.rejects(c.api('studio/playlists'),/Cannot read playlists/);assert.equal(calls,1);});
test('total deadline aborts a hung catalog fetch instead of leaving retries alive',async()=>{let calls=0;const c=client(async(url,options)=>{calls++;return new Promise((resolve,reject)=>options.signal.addEventListener('abort',()=>reject(Error('aborted')),{once:true}));},{setTimeout:(fn,ms)=>setTimeout(fn,ms===12000?20:ms)});await assert.rejects(c.api('studio/playlists'),/server is busy/);assert.equal(calls,1);});

test('catalog deadline also covers a response body that never finishes',async()=>{let calls=0;const c=client(async(url,options)=>{calls++;return {status:200,ok:true,json:()=>new Promise((resolve,reject)=>options.signal.addEventListener('abort',()=>reject(Error('aborted body')),{once:true}))};},{setTimeout:(fn,ms)=>setTimeout(fn,ms===12000?20:ms)});await assert.rejects(c.api('studio/playlists'),/server is busy/);assert.equal(calls,1);});

}
