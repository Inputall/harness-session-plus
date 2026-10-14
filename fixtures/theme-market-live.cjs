// Explicit appearance acceptance on unique local Harness. Never touches session APIs.
const fs=require('node:fs'),path=require('node:path');
(async()=>{
 const root=path.resolve(__dirname,'..');
 const targets=(await (await fetch('http://127.0.0.1:9223/json/list',{redirect:'error'})).json()).filter(t=>t.type==='page'&&t.url==='dsh-app://app/');
 if(targets.length!==1)throw Error('Ambiguous Harness page');
 const endpoint=new URL(targets[0].webSocketDebuggerUrl);if(endpoint.hostname!=='127.0.0.1'||endpoint.port!=='9223'||endpoint.protocol!=='ws:')throw Error('Invalid endpoint');
 const socket=new WebSocket(endpoint);await new Promise((resolve,reject)=>{socket.onopen=resolve;socket.onerror=reject;});
 const pending=new Map();let next=1;
 socket.onmessage=e=>{const v=JSON.parse(e.data);const p=pending.get(v.id);if(!p)return;pending.delete(v.id);clearTimeout(p.timer);v.error||v.result?.exceptionDetails?p.reject(Error(v.result?.result?.description||JSON.stringify(v.error))):p.resolve(v.result.result.value);};
 const evaluate=expression=>new Promise((resolve,reject)=>{const id=next++;const timer=setTimeout(()=>{pending.delete(id);reject(Error('Timed out'));},20000);pending.set(id,{resolve,reject,timer});socket.send(JSON.stringify({id,method:'Runtime.evaluate',params:{expression:`(()=>{if(location.href!=='dsh-app://app/')throw Error('Wrong page');return (${expression});})()`,awaitPromise:true,returnByValue:true}}));});
 const script=fs.readFileSync(path.join(root,'injected/skin.js'),'utf8');
 const read="(()=>{const c=getComputedStyle(document.body);return {background:c.backgroundImage,color:c.backgroundColor,accent:c.getPropertyValue('--dsw-alias-state-business-primary'),installed:!!window.__HARNESS_SESSION_PLUS_SKIN__};})()";
 const before=await evaluate(read);if(before.installed){socket.close();throw Error('Existing skin: preserve current user appearance');}
 const results=[];let after;
 const color=s=>s.startsWith('rgba(')?'rgba('+s.slice(5,-1).split(',').map(v=>Number(v.trim())).join(',')+')':s.length===4?'#'+s.slice(1).split('').map(c=>c+c).join(''):s.toLowerCase();
 try{
  for(const source of JSON.parse(fs.readFileSync(path.join(root,'desktop/ui/themes/sources.json'),'utf8'))){
   const dir=path.join(root,'assets/themes',source.id);const config=JSON.parse(fs.readFileSync(path.join(dir,'theme.json'),'utf8'));const c=config.colors;
   const bytes=fs.readFileSync(path.join(dir,source.image_file));const mime=source.image_file.endsWith('.jpg')?'jpeg':'png';
   const skin={id:source.id,bg:color(c.background),surface:color(c.panel),hover:color(c.panelAlt),text:color(c.text),muted:color(c.muted),accent:color(c.accent),border:color(c.line),wallpaper:`data:image/${mime};base64,${bytes.toString('base64')}`};
   const request=JSON.stringify({action:'apply',skin});
   const result=await evaluate(`(async()=>{const result=(${script})(${request});const c=getComputedStyle(document.body);const img=new Image();img.src=window.__HARNESS_SESSION_PLUS_SKIN__.node.textContent.match(/url\\("([^\"]+)"\\)/)[1];await img.decode();return {...result,decoded:[img.naturalWidth,img.naturalHeight],accent:c.getPropertyValue('--dsw-alias-state-business-primary').trim(),wallpaperVisible:c.backgroundImage!=='none'};})()`);
   if(result.accent!==skin.accent||!result.wallpaperVisible||!result.installed)throw Error(`Mismatch ${source.id}`);
   results.push(result);console.log(`Verified ${source.id}`);
  }
 }finally{await evaluate(`(${script})({action:'reset'})`);after=await evaluate(read);socket.close();}
 const restored=JSON.stringify(before)===JSON.stringify(after);
 fs.writeFileSync(path.join(root,'target/all-themes-live-results.json'),JSON.stringify({before,after,restored,results},null,2));
 if(!restored)throw Error('Restore mismatch');
})().catch(e=>{console.error(e);process.exitCode=1;});
