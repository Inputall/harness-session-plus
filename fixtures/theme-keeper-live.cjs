// Real keeper + real Harness appearance acceptance, without restarting Harness or touching sessions.
const fs=require('node:fs'),path=require('node:path'),{spawn}=require('node:child_process');
const pause=ms=>new Promise(r=>setTimeout(r,ms));
(async()=>{
 const root=path.resolve('target/theme-keeper-acceptance');fs.mkdirSync(root,{recursive:true});
 const config=path.join(root,'active-theme.json');if(fs.existsSync(config))throw Error('Previous acceptance config exists');
 const exe=path.resolve('desktop/src-tauri/target/release/hsp-desktop.exe');
 const targets=(await (await fetch('http://127.0.0.1:9223/json/list')).json()).filter(t=>t.type==='page'&&t.url==='dsh-app://app/');if(targets.length!==1)throw Error('Ambiguous Harness');
 const ws=new WebSocket(targets[0].webSocketDebuggerUrl);await new Promise((r,j)=>{ws.onopen=r;ws.onerror=j;});
 let next=1;const pending=new Map();ws.onmessage=e=>{const v=JSON.parse(e.data),p=pending.get(v.id);if(!p)return;pending.delete(v.id);clearTimeout(p.timer);v.error||v.result?.exceptionDetails?p.reject(Error(JSON.stringify(v))):p.resolve(v.result);};
 const evaluate=expression=>new Promise((resolve,reject)=>{const id=next++,timer=setTimeout(()=>{pending.delete(id);reject(Error('Timeout'));},15000);pending.set(id,{resolve:v=>resolve(v.result.value),reject,timer});ws.send(JSON.stringify({id,method:'Runtime.evaluate',params:{expression,returnByValue:true,awaitPromise:true}}));});
 const waitFor=async fn=>{for(let i=0;i<80;i++){if(await fn())return;await pause(200);}throw Error('Acceptance timeout');};
 const children=[];let saved=false;
 try{
  await evaluate(`(()=>{if(location.href!=='dsh-app://app/'||window.__hspKeeperAcceptance)throw Error('Unsafe target');const old=window.__HARNESS_SESSION_PLUS_SKIN__;window.__hspKeeperAcceptance={old,css:old?.node.textContent};return true;})()`);saved=true;
  const skin=JSON.parse(fs.readFileSync('desktop/ui/skins.json','utf8'))[0];skin.persistenceRevision='keeper-acceptance';skin.wallpaperClarity=85;
  fs.writeFileSync(config,JSON.stringify({options:{port:9223,target_id:'stale-id-before-restart'},skin,revision:skin.persistenceRevision}));
  const keeper=spawn(exe,['--theme-keeper',root],{windowsHide:true,stdio:'ignore'});children.push(keeper);
  await waitFor(async()=>await evaluate(`window.__HARNESS_SESSION_PLUS_SKIN__?.revision==='keeper-acceptance'`));
  const duplicate=spawn(exe,['--theme-keeper',root],{windowsHide:true,stdio:'ignore'});children.push(duplicate);await waitFor(async()=>duplicate.exitCode!==null);
  // Simulate losing the document's overlay; do not reload a user's live conversation.
  await evaluate(`(()=>{window.__HARNESS_SESSION_PLUS_SKIN__.node.remove();delete window.__HARNESS_SESSION_PLUS_SKIN__;return true;})()`);
  await waitFor(async()=>await evaluate(`window.__HARNESS_SESSION_PLUS_SKIN__?.revision==='keeper-acceptance'`));
  fs.unlinkSync(config);await waitFor(async()=>keeper.exitCode!==null);
  const retained=await evaluate(`window.__HARNESS_SESSION_PLUS_SKIN__?.revision==='keeper-acceptance'`);if(!retained)throw Error('Theme was cleared on keeper exit');
  const report={standaloneKeeper:true,staleTargetRebound:true,duplicateExit:duplicate.exitCode,lostOverlayRecovered:true,exitRetainsTheme:retained,keeperStopped:keeper.exitCode,realHarnessRestartTested:false};
  fs.writeFileSync('target/theme-keeper-live-results.json',JSON.stringify(report,null,2));console.log(JSON.stringify(report));
 }finally{
  if(fs.existsSync(config))fs.unlinkSync(config);
  for(const child of children)if(child.exitCode===null){await pause(3500);if(child.exitCode===null)child.kill();}
  if(saved)await evaluate(`(()=>{const saved=window.__hspKeeperAcceptance;window.__HARNESS_SESSION_PLUS_SKIN__?.node.remove();if(saved.old){saved.old.node.textContent=saved.css;if(!saved.old.node.isConnected)document.head.append(saved.old.node);window.__HARNESS_SESSION_PLUS_SKIN__=saved.old;}else delete window.__HARNESS_SESSION_PLUS_SKIN__;delete window.__hspKeeperAcceptance;return true;})()`);
  ws.close();
 }
})().catch(e=>{console.error(e);process.exitCode=1;});
