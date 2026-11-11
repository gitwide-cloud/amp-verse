import {randomUUID} from 'node:crypto'
import WebSocket from 'ws'
export class Client {
  constructor(address,password='',onState=()=>{},onConnection=()=>{}) {this.base=new URL(address.includes('://')?address:`http://${address}`);if(!['http:','https:'].includes(this.base.protocol))throw Error('Use an HTTP/S AMP address');this.password=password;this.onState=onState;this.onConnection=onConnection;this.closed=false;this.session=null}
  async read(path,body,session=true) {
    const response=await fetch(new URL(path,this.base),{method:body===undefined?'GET':'POST',headers:{'Content-Type':'application/json','x-amp-password':this.password,...(this.targets||{}),...(session&&this.session?{'x-amp-session':this.session}:{})},body:body===undefined?undefined:JSON.stringify(body),signal:AbortSignal.timeout(5000),redirect:'error'})
    const text=await response.text();let value;try{value=JSON.parse(text)}catch{value=text}
    if(!response.ok)throw Error(typeof value==='object'?(value.error||`AMP ${response.status}`):String(value));return value
  }
  async connect() {const info=await this.read('/api/v1/info',undefined,false);if(this.closed)return;this.session=info.session;this.info=info;const state=await this.read('/api/v1/state');if(this.closed)return;this.onState(state);const url=new URL('/api/v1/events',this.base);url.protocol=url.protocol==='https:'?'wss:':'ws:';this.ws=new WebSocket(url,{headers:{'x-amp-password':this.password,'x-amp-session':this.session},handshakeTimeout:5000});this.ws.on('message',data=>{if(this.closed)return;try{this.onState(JSON.parse(data.toString()));this.onConnection(true)}catch{}});this.ws.on('error',()=>{});this.ws.on('close',()=>{if(this.closed)return;this.onConnection(false);if(!this.closed)this.timer=setTimeout(()=>this.connect().catch(()=>this.retry()),1500)});this.onConnection(true)}
  retry(){if(!this.closed)this.timer=setTimeout(()=>this.connect().catch(()=>this.retry()),3000)}
  async command(action,fields={}) {const body={id:randomUUID(),action,...fields};let last;for(let attempt=0;attempt<2;attempt++){try{const result=await this.read('/api/v1/command',body);if(!result.ok)throw Error(result.error||'Command failed');return result}catch(error){last=error}}throw last}
  close(){this.closed=true;clearTimeout(this.timer);this.ws?.close()}
}
export function songOptions(songs){return songs.map(s=>({id:s.id,label:`${s.title}${s.artist?` — ${s.artist}`:''}`}))}
export function cueOptions(song){return [{id:'',label:'Prepared cue IN'},...(song?.cues||[]).map(c=>({id:c.id,label:`${c.number?`${c.number} · `:''}${c.name} (${c.seconds.toFixed(2)}s)`}))]}

export async function discover(timeout=1800){
  const {createSocket}=await import('node:dgram');const socket=createSocket('udp4');const found=new Map();
  return new Promise(resolve=>{const finish=()=>{try{socket.close()}catch{}resolve([...found.values()])};socket.on('error',finish);socket.on('message',(data,peer)=>{try{const info=JSON.parse(data.toString());if(info.protocol===1&&['amp-control','amp-cast'].includes(info.service)&&Number.isInteger(info.port)&&info.port>0&&info.port<=65535){const address=`${peer.address}:${info.port}`;found.set(address,{id:address,label:`${info.name||info.service} (${address})`,service:info.service})}}catch{}});socket.bind(0,'0.0.0.0',()=>{socket.setBroadcast(true);for(const host of ['255.255.255.255','127.0.0.1'])for(const [port,query] of [[18790,'AMPVERSE_CONTROL_DISCOVER_V1'],[18787,'AMPVERSE_CAST_DISCOVER_V1']])socket.send(Buffer.from(query),port,host)});setTimeout(finish,timeout).unref()})
}
