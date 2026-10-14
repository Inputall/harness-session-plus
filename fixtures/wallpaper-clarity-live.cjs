// Explicit appearance-only acceptance; restores any existing owned skin exactly.
const fs=require('node:fs');
(async()=>{
 const targets=(await (await fetch('http://127.0.0.1:9223/json/list')).json()).filter(t=>t.type==='page'&&t.url==='dsh-app://app/');
 if(targets.length!==1)throw Error('Ambiguous Harness');
 const ws=new WebSocket(targets[0].webSocketDebuggerUrl);await new Promise((r,j)=>{ws.onopen=r;ws.onerror=j;});
 let next=1;const pending=new Map();ws.onmessage=e=>{const v=JSON.parse(e.data),p=pending.get(v.id);if(!p)return;pending.delete(v.id);clearTimeout(p.timer);v.error||v.result?.exceptionDetails?p.reject(Error(JSON.stringify(v))):p.resolve(v.result);};
 const call=(method,params)=>new Promise((resolve,reject)=>{const id=next++,timer=setTimeout(()=>{pending.delete(id);reject(Error('Timeout'));},15000);pending.set(id,{resolve,reject,timer});ws.send(JSON.stringify({id,method,params}));});
 const evaluate=async expression=>(await call('Runtime.evaluate',{expression,returnByValue:true,awaitPromise:true})).result.value;
 const script=fs.readFileSync('injected/skin.js','utf8'),config=JSON.parse(fs.readFileSync('assets/themes/dream-skin-original/theme.json','utf8'));
 const color=s=>s.startsWith('rgba(')?'rgba('+s.slice(5,-1).split(',').map(v=>Number(v.trim())).join(',')+')':s.toLowerCase();
 const c=config.colors,skin={id:config.id,bg:color(c.background),surface:color(c.panel),hover:color(c.panelAlt),text:color(c.text),muted:color(c.muted),accent:color(c.accent),border:color(c.line),wallpaper:'data:image/png;base64,'+fs.readFileSync('assets/themes/dream-skin-original/image.png').toString('base64')};
 let saved=false;const results=[];
 try{
  await evaluate(`(()=>{if(location.href!=='dsh-app://app/'||window.__hspClarityAcceptance)throw Error('Unsafe target');const old=window.__HARNESS_SESSION_PLUS_SKIN__;window.__hspClarityAcceptance={old,css:old?.node.textContent};return true;})()`);saved=true;
  for(const clarity of [0,70,100]){
   const result=await evaluate(`(()=>{const result=(${script})(${JSON.stringify({action:'apply',skin:{...skin,wallpaperClarity:clarity}})});const c=getComputedStyle(document.body);return {...result,panel:c.getPropertyValue('--dsw-alias-bg-layer-1').trim(),sidebar:c.getPropertyValue('--dsw-specific-sidebar-fill').trim(),input:c.getPropertyValue('--dsw-specific-input-major').trim(),menu:c.getPropertyValue('--dsw-specific-menu').trim()};})()`);
   if(!result.sidebar.includes(({0:'0.950',70:'0.460',100:'0.250'})[clarity])||result.input!==skin.surface||result.menu!==skin.surface)throw Error('Unexpected panel coverage');
   results.push(result);
  }
 }finally{
  if(saved)await evaluate(`(()=>{const saved=window.__hspClarityAcceptance,current=window.__HARNESS_SESSION_PLUS_SKIN__;if(saved.old){saved.old.node.textContent=saved.css;window.__HARNESS_SESSION_PLUS_SKIN__=saved.old;}else{current?.node.remove();delete window.__HARNESS_SESSION_PLUS_SKIN__;}delete window.__hspClarityAcceptance;return true;})()`);
  ws.close();
 }
 fs.writeFileSync('target/wallpaper-clarity-live.json',JSON.stringify({results,restored:true},null,2));console.log(JSON.stringify({results,restored:true}));
})().catch(e=>{console.error(e);process.exitCode=1;});
