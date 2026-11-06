// Integration check for an admitted Android test player, through its real WebView/Rust bridge.
// Usage: node tools/test/android-client.mjs emulator-5584
// Use the dedicated VM and generated Android-join fixture; never a real user's profile.
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const serial = process.argv[2];
if (!/^emulator-\d+$/.test(serial ?? '')) throw Error('Use a dedicated Android test emulator.');
const pkg = 'com.dicefoxcreations.anotherworldbuilder';
const adb = (...args) => execFileSync('adb', ['-s', serial, ...args], {encoding:'utf8',timeout:30000}).trim();
const pid = adb('shell','pidof',pkg).split(' ')[0];
const hostPort = adb('forward','tcp:0',`localabstract:webview_devtools_remote_${pid}`);
let ws; const forwards=[hostPort];
try {
  const pages=await(await fetch(`http://127.0.0.1:${hostPort}/json/list`)).json();
  ws=new WebSocket(pages.find(p=>p.type==='page').webSocketDebuggerUrl);
  await new Promise((ok,no)=>{ws.onopen=ok;ws.onerror=no;});
  let next=0; const pending=new Map();
  ws.onmessage=e=>{const r=JSON.parse(e.data);if(r.id){const p=pending.get(r.id);pending.delete(r.id);clearTimeout(p.timer);r.error?p.reject(r.error):p.resolve(r.result);}};
  const send=(method,params)=>new Promise((resolve,reject)=>{const id=++next;const timer=setTimeout(()=>{pending.delete(id);reject(Error('Timed out: '+method));},45000);pending.set(id,{resolve,reject,timer});ws.send(JSON.stringify({id,method,params}));});
  const evaluate=async expression=>{
    const r=await send('Runtime.evaluate',{expression:`(async()=>(${expression}))()`,awaitPromise:true,returnByValue:true});
    if(r.exceptionDetails)throw Error(JSON.stringify(r.exceptionDetails));return r.result.value;
  };
  const fixture=JSON.parse(fs.readFileSync(path.join(root,'build/android-join-test/android-join-test.json'),'utf8'));
  const report=await evaluate(`(async()=>{
    const call=window.__TAURI_INTERNALS__.invoke;
    const me=await call('identity_status');
    if(me?.name!=='Android Client Test')throw Error('Use the generated test client profile.');
    const world=${JSON.stringify(fixture.world)};
    const info=await call('world_connect',{world});
    if(info.role!=='player')throw Error('Expected admitted player');
    const remote=(command,args={})=>call('world_call',{world,command,args:{world,...args}});
    const view=await remote('view');const text=JSON.stringify(view);
    if(!text.includes('Willkommen am Hafen!')||text.includes('schlafender Drache'))throw Error('Player visibility failed');
    const article=${JSON.stringify(fixture.article)};
    const original='Diese Welt liegt auf dem Android-Server. Willkommen am Hafen!';
    const marker='Android client write verified.';
    const opened=await remote('document_open',{article});
    if(!opened.code.includes(original))throw Error('Fixture public text missing');
    await remote('document_save',{article,text:opened.code.replace(original,original+' '+marker),base:opened.base,session:'android-client-integration'});
    try {
      if(!JSON.stringify(await remote('view')).includes(marker))throw Error('Player edit did not reach the server');
    } finally {
      const edited=await remote('document_open',{article});
      if(edited.code.includes(marker)) await remote('document_save',{article,text:edited.code.replace(original+' '+marker,original),base:edited.base,session:'android-client-integration'});
    }
    if(JSON.stringify(await remote('view')).includes(marker))throw Error('Fixture text not restored');
    let denied=false;
    try{await remote('world_icon_set',{icon:'X'});}catch(e){denied=String(e).includes('GM')||String(e).includes('game master');}
    if(!denied)throw Error('Expected explicit GM permission denial');
    const network=await call('network_settings');
    if(network.port!==7447||!network.active_port||network.active_port===7447)throw Error('Host did not resolve the server port conflict');
    return {identity:me.id,world,role:info.role,publicVisible:true,gmSecretHidden:true,playerWrite:true,playerEditReadBack:true,fixtureTextRestored:true,gmWriteDenied:denied,configuredPort:network.port,activePort:network.active_port,clientCode:network.code};
  })()`);
  const forward=port=>{const local=adb('forward','tcp:0',`tcp:${port}`);forwards.push(local);return local;};
  const clientCode=(await(await fetch(`http://127.0.0.1:${forward(report.activePort)}/`)).text()).trim();
  const serverCode=(await(await fetch(`http://127.0.0.1:${forward(7447)}/`)).text()).trim();
  if(clientCode!==report.clientCode||clientCode.split('@')[0]===serverCode.split('@')[0])throw Error('TCP discovery codes crossed');
  report.serverCode=serverCode;report.lookupUsesActivePort=true;
  const ui=await evaluate(`document.body.innerText`);
  if(!ui.includes('Willkommen am Hafen!')||ui.includes('schlafender Drache'))throw Error('The visible player article is missing or exposes the GM secret');
  report.articleVisible=true;
  const directory=path.join(root,'build/android-client');fs.mkdirSync(directory,{recursive:true});
  fs.writeFileSync(path.join(directory,'integration-result.json'),JSON.stringify(report,null,2)+'\n');
  fs.writeFileSync(path.join(directory,'client-world.png'),execFileSync('adb',['-s',serial,'exec-out','screencap','-p']));
  console.log('PASS: real Android client, admitted player, visible article, permissions, parallel hosting and distinct TCP/UDP lookup ports.');
  console.log(JSON.stringify(report,null,2));
} finally {
  ws?.close();for(const port of forwards){try{adb('forward','--remove',`tcp:${port}`);}catch{}}
}
