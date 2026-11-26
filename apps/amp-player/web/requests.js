'use strict';
const $=id=>document.getElementById(id);
let songs=[],selected=null,session=null,busy=false,pending=null;
function matchesTitle(title,query){return String(title||'').toLowerCase().includes(query.trim().toLowerCase());}
function controls(){$('send').disabled=busy||!session||!selected;$('connect').disabled=busy;$('search').disabled=busy||!session;$('within').disabled=busy;$('password').disabled=busy;}
function render(){
 const query=$('search').value,found=songs.filter(song=>matchesTitle(song.title,query));
 if(selected&&!found.some(song=>song.id===selected.id)){selected=null;pending=null;}
 $('songs').replaceChildren();
 $('count').textContent=found.length+' matches'+(found.length>100?' · showing first 100; type more to narrow the list':'');
 for(const song of found.slice(0,100)){
  const item=document.createElement('li'),button=document.createElement('button');
  button.textContent=song.title;button.type='button';button.disabled=busy;button.setAttribute('aria-pressed',String(selected?.id===song.id));
  button.onclick=()=>{selected=song;pending=null;$('status').textContent='';render();};item.append(button);$('songs').append(item);
 }
 $('selected').textContent=selected?'Selected: '+selected.title:'Select a song above.';controls();
}
async function read(path,body){
 const response=await fetch(path,{method:body?'POST':'GET',cache:'no-store',headers:{'content-type':'application/json','x-amp-password':$('password').value,...(session?{'x-amp-session':session}:{})},...(body?{body:JSON.stringify(body)}:{}),signal:AbortSignal.timeout(8000)});
 if(!response.ok)throw Error(response.status===401?'DJ password or session changed. Enter the password and reconnect.':(await response.text())||'DJ unavailable');
 return response.json();
}
async function connect(){
 busy=true;session=null;selected=null;pending=null;songs=[];render();$('status').textContent='Loading the DJ library…';
 try{
  const info=await read('/api/v1/info');session=info.session;
  const catalog=await read('/api/v1/request-songs');if(catalog.session!==session)throw Error('DJ restarted while loading. Connect again.');
  songs=Array.isArray(catalog.songs)?catalog.songs:[];
  $('dj').textContent='Connected to DJ at '+location.host;$('status').textContent=songs.length?'Type a title to find a song.':'The DJ library is not ready yet. Refresh when indexing finishes.';
 }catch(error){session=null;$('dj').textContent='Not connected';$('status').textContent=error.message||String(error);}
 finally{busy=false;render();}
}
async function send(){
 if(!selected||!session||busy)return;
 const within=Number($('within').value);if(!Number.isInteger(within)||within<1||within>5){$('status').textContent='Choose a number from 1 to 5.';return;}
 const song=selected;
 if(!pending||pending.song!==song.id||pending.within!==within)pending={id:Date.now()+'-'+Array.from(crypto.getRandomValues(new Uint8Array(12)),b=>b.toString(16).padStart(2,'0')).join(''),song:song.id,within,action:'request_song'};
 busy=true;render();$('status').textContent='Sending request…';
 try{const reply=await read('/api/v1/command',pending);if(!reply.ok){pending=null;throw Error(reply.error||'The DJ could not accept this request.');}$('status').textContent='Request accepted by DJ: '+song.title;selected=null;pending=null;}
 catch(error){$('status').textContent=(error.message||String(error))+' You can reconnect or retry.';}
 finally{busy=false;render();}
}
$('search').addEventListener('input',render);$('connect').onclick=connect;$('send').onclick=send;
connect();
