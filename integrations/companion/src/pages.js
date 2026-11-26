export function extraPages(){
 const definitions={},groups=[]
 const page=(id,name,buttons)=>{const ids=[];for(const [key,text,actionId,options,color=0xffffff] of buttons){const ident=`${id}_${key}`;ids.push(ident);definitions[ident]={type:'simple',name:text,style:{text,size:'auto',color,bgcolor:0x151a20},steps:[{down:[{actionId,options:Object.fromEntries(Object.entries(options).map(([k,value])=>[k,{value,isExpression:false}]))}],up:[]}],feedbacks:[]}}groups.push({id,name,definitions:ids})}
 page('player','Player',[
  ...['play','pause','stop_player','player_next','previous'].map(o=>[o,o.replaceAll('_',' '),'player',{operation:o}]),
  ['duck','Duck','duck',{level:.2,seconds:1}],['restore','Restore','restore',{}],
  ['down','Volume -','volume',{delta:-.05}],['up','Volume +','volume',{delta:.05}],
  ['in','Fade in','player_fade',{target:1,seconds:3}],['out','Fade out','player_fade',{target:0,seconds:3}],
 ])
 page('auto','Auto DJ and safety',[
  ...['auto_on','auto_off','mix_next','auto_fade','stop_all_decks','stop_samplers','emergency_mute'].map(o=>[o,o.replaceAll('_',' '),'auto',{operation:o}]),
  ['pause','Pause Auto DJ','auto_pause',{paused:true}],['resume','Resume Auto DJ','auto_pause',{paused:false}],
  ['lock','Manual Lock','lock',{enabled:true}],['unlock','Unlock','lock',{enabled:false}],
  ['3sec','Mix 3 seconds','mix_time',{seconds:3}],
 ])
 page('energy','Energy and transitions',[
  ...['hype','steady','chill'].map(style=>[style,style,'style',{style}]),
  ['up','Energy +','energy_step',{delta:1}],['down','Energy -','energy_step',{delta:-1}],
  ['hold','Hold energy','hold',{}],['cancel','Cancel progression','cancel_progression',{}],['shuffle','Smart Shuffle','shuffle_smart',{}],
  ...['off','scratch','stutter','filter','variety'].map(preset=>[preset,`${preset} FX`,'fx',{preset,every:3}]),
 ])
 page('samplers','Sampler banks',[
  ...[0,1].flatMap(side=>Array.from({length:9},(_,i)=>[`${side}_${i}`,`${side?'B':'A'} Pad ${i+1}`,'sampler',{side,pad:i+1},0xca8de8])),
  ['stop','Stop all pads','auto',{operation:'stop_samplers'},0xff6666],
 ])
 page('cast','Broadcast and radio',[
  ['live','Broadcast DJ','broadcast',{operation:'start',source:'dj'}],['player','Broadcast Player','broadcast',{operation:'start',source:'player'}],['stop','Stop broadcast','broadcast',{operation:'stop',source:'dj'}],
  ...['start','stop','restart','server_output_on','server_output_off'].map(operation=>[operation,`Station ${operation.replaceAll('_',' ')}`,'station',{operation}]),
  ['discover','Find speakers','google_cast',{operation:'discover'}],['uncast','Stop speaker','google_cast',{operation:'stop'}],
 ])
 return {definitions,groups}
}
