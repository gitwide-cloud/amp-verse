import test from 'node:test'
import assert from 'node:assert/strict'
import http from 'node:http'
import {Client,songOptions,cueOptions} from '../src/client.js'
test('scoped picker uses resource identity and named cues',()=>{assert.deepEqual(songOptions([{id:'audio_hash',title:'Intro'}]),[{id:'audio_hash',label:'Intro'}]);assert.equal(cueOptions({cues:[{id:'drop',number:3,name:'Drop',seconds:12} ]})[1].id,'drop')})
test('a retried command retains its request ID',async()=>{let attempts=[];const server=http.createServer(async(req,res)=>{let body='';for await(const chunk of req)body+=chunk;attempts.push(JSON.parse(body));if(attempts.length===1){req.socket.destroy();return}res.setHeader('Content-Type','application/json');res.end('{"ok":true}')});await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));try{const client=new Client(`127.0.0.1:${server.address().port}`);await client.command('play',{deck:'a3'});assert.equal(attempts.length,2);assert.equal(attempts[0].id,attempts[1].id);assert.equal(attempts[1].deck,'a3')}finally{server.closeAllConnections();await new Promise(resolve=>server.close(resolve))}})
test('module class loads using the official SDK',async()=>{const module=await import('../src/main.js');assert.equal(typeof module.default,'function')})

test('official SDK preset structure and deck action route agree',async()=>{
 const {default:Module}=await import('../src/main.js');const host=Object.create(Module.prototype);host.playlists=[];host.songs=[];host.cues=[];host.state={};let actions,structure,presets,sent;
 host.setActionDefinitions=value=>actions=value;host.setPresetDefinitions=(a,b)=>{structure=a;presets=b};host.send=async(action,fields)=>sent={action,...fields};
 host.actions();host.presets();assert.ok(Object.keys(presets).length>70);assert.equal(structure[0].definitions.length,24);
 for(const preset of structure[0].definitions.map(id=>presets[id])){assert.equal(preset.type,'simple');const command=preset.steps[0].down[0];assert.ok(actions[command.actionId]);const options=Object.fromEntries(Object.entries(command.options).map(([k,v])=>{assert.equal(v.isExpression,false);return [k,v.value]}));await actions[command.actionId].callback({options});assert.equal(sent.deck,options.deck)}
 await actions.deck.callback({options:{deck:'b3',operation:'loop_on'}});assert.deepEqual(sent,{action:'deck_loop',deck:'b3',enabled:true,beats:4});
 await actions.stems.callback({options:{deck:'a2',vocals:0,instruments:.5,beats:1}});assert.deepEqual(sent,{action:'deck_stems',deck:'a2',levels:[0,.5,1]});
})

test('live feedback reflects six decks, samples and confirmed station output',async()=>{
 const {values}=await import('../src/feedback.js');const v=values({deck_layers:[{deck:'B3',state:'playing',sync:true}],samplers:[[{name:'Hit',loaded:true,playing:false,loop:true}]],google_cast:{playback_state:'PLAYING',receiver:{name:'Speaker'}}},{on_air:false,server_output:true,now_playing:'Radio title'});assert.equal(v.b3_state,'playing');assert.equal(v.s1_1_playing,false);assert.equal(v.s1_1_loop,true);assert.equal(v.station_status,'Stopped');assert.equal(v.server_output,'On');assert.equal(v.station_track,'Radio title');assert.equal(v.receiver_state,'PLAYING');
 const {extraPages}=await import('../src/pages.js');const pages=extraPages();assert.equal(pages.groups.length,5);assert.ok(pages.definitions.samplers_0_0.steps[0].down[0].options.pad.value===1);
})
