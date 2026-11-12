'use strict';
const $=id=>document.getElementById(id);
let session=null,busy=false,refreshing=false,current=null,generation=0;
const buttons=['play','pause','next','auto','crossfader','quieter','louder'];
function controls(){for(const id of buttons)$(id).disabled=busy||!session||!current;$('connect').disabled=busy;$('password').disabled=busy;}
async function read(path,body){
 const response=await fetch(path,{method:body?'POST':'GET',cache:'no-store',headers:{'content-type':'application/json','x-amp-password':$('password').value,...(session?{'x-amp-session':session}:{})},...(body?{body:JSON.stringify(body)}:{}),signal:AbortSignal.timeout(8000)});
 if(!response.ok)throw Error(response.status===401?'Password or session changed. Reconnect to this PC.':(await response.text())||'PC unavailable');
 return response.json();
}
function render(state){
 const amp=state.amp||{};current=amp;
 $('song').textContent=amp.current_song||'No track loaded';$('playing').textContent=amp.playback_state||'Stopped';
 $('auto').textContent=amp.auto_dj_enabled?'Auto DJ: ON':'Auto DJ: OFF';$('auto').setAttribute('aria-pressed',String(!!amp.auto_dj_enabled));
 if(document.activeElement!==$('crossfader'))$('crossfader').value=Number.isFinite(amp.crossfader)?amp.crossfader:0.5;
 controls();
}
async function connect(){
 const attempt=++generation;busy=true;session=null;current=null;controls();$('status').textContent='Connecting…';
 try{const info=await read('/api/v1/info');if(attempt!==generation)return;session=info.session;const state=await read('/api/v1/state');if(attempt!==generation)return;
  render(state);$('connection').textContent='Connected to '+location.host;$('status').textContent='Ready.';
 }catch(error){session=null;current=null;$('connection').textContent='Not connected';$('status').textContent=error.message||String(error);}
 finally{busy=false;controls();}
}
async function refresh(){
 if(!session||busy||refreshing)return;refreshing=true;const attempt=generation;
 try{const state=await read('/api/v1/state');if(attempt===generation&&!busy)render(state);}
 catch(error){if(attempt===generation&&!busy){session=null;current=null;$('connection').textContent='Disconnected';$('status').textContent=error.message||String(error);controls();}}
 finally{refreshing=false;}
}
async function send(action){
 if(!session||busy||!current)return;busy=true;controls();$('status').textContent='Sending…';
 const id=Date.now()+'-'+Array.from(crypto.getRandomValues(new Uint8Array(12)),b=>b.toString(16).padStart(2,'0')).join('');
 try{const reply=await read('/api/v1/command',{id,...action});if(!reply.ok)throw Error(reply.error||'Command could not be applied.');$('status').textContent=reply.status||'Applied.';render(await read('/api/v1/state'));}
 catch(error){$('status').textContent=(error.message||String(error))+' Refresh before trying again.';}
 finally{busy=false;controls();}
}
$('connect').onclick=connect;
$('play').onclick=()=>send({action:'play',deck:'auto'});
$('pause').onclick=()=>send({action:'pause',deck:'auto'});
$('next').onclick=()=>send(current?.auto_dj_enabled?{action:'mix_next'}:{action:'player_next'});
$('auto').onclick=()=>send({action:'auto_dj',enabled:!current?.auto_dj_enabled});
$('crossfader').onchange=()=>send({action:'set_crossfader',value:Number($('crossfader').value)});
$('quieter').onclick=()=>send({action:'adjust_volume',delta:-0.05});
$('louder').onclick=()=>send({action:'adjust_volume',delta:0.05});
setInterval(refresh,1500);
connect();
