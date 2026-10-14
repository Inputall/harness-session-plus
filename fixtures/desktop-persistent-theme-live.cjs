// Explicit native desktop quit + appearance-only recovery test. Preserves existing appearance.
const fs=require('node:fs'),path=require('node:path'),{spawn}=require('node:child_process');
const pause=ms=>new Promise(r=>setTimeout(r,ms));
const connect=async(port,url)=>{
 const targets=(await(await fetch(`http://127.0.0.1:${port}/json/list`)).json()).filter(t=>t.type==='page'&&t.url===url);if(targets.length!==1)throw Error('Ambiguous target');
 const ws=new WebSocket(targets[0].webSocketDebuggerUrl);await new Promise((r,j)=>{ws.onopen=r;ws.onerror=j;});let next=1;const pending=new Map();
 ws.onmessage=e=>{const v=JSON.parse(e.data),p=pending.get(v.id);if(!p)return;pending.delete(v.id);clearTimeout(p.timer);v.error||v.result?.exceptionDetails?p.reject(Error(JSON.stringify(v))):p.resolve(v.result.result.value);};
 return {ws,eval:expression=>new Promise((resolve,reject)=>{const id=next++,timer=setTimeout(()=>{pending.delete(id);reject(Error('Timeout'));},15000);pending.set(id,{resolve,reject,timer});ws.send(JSON.stringify({id,method:'Runtime.evaluate',params:{expression,returnByValue:true,awaitPromise:true}}));})};
};
(async()=>{
 const config=path.join(process.env.APPDATA,'com.harness-session-plus.manager','active-theme.json');if(fs.existsSync(config))throw Error('Preserve user saved theme; acceptance requires no active persistence');
 const harness=await connect(9223,'dsh-app://app/');let ui=await connect(9224,'http://tauri.localhost/'),saved=false;
 const waitFor=async fn=>{for(let i=0;i<80;i++){try{if(await fn())return;}catch{}await pause(200);}throw Error('Acceptance timeout');};
 try{
  await harness.eval(`(()=>{if(window.__hspDesktopKeeperAcceptance)throw Error('Already testing');const old=window.__HARNESS_SESSION_PLUS_SKIN__;window.__hspDesktopKeeperAcceptance={old,css:old?.node.textContent};return true;})()`);saved=true;
  const applied=await ui.eval(`window.__TAURI__.core.invoke('skin_action',{action:'apply',skinId:'midnight',wallpaperClarity:85})`);
  if(!applied.persistent)throw Error('Persistence not saved');
  const disk=JSON.parse(fs.readFileSync(config,'utf8'));if(disk.skin.wallpaperClarity!==85)throw Error('Clarity not saved');
  await ui.eval(`window.__TAURI__.core.invoke('quit_app')`).catch(()=>{});ui.ws.close();await pause(2000);
  const retained=await harness.eval(`window.__HARNESS_SESSION_PLUS_SKIN__?.revision===${JSON.stringify(disk.revision)}`);if(!retained)throw Error('Quit removed skin');
  await harness.eval(`(()=>{window.__HARNESS_SESSION_PLUS_SKIN__.node.remove();delete window.__HARNESS_SESSION_PLUS_SKIN__;return true;})()`);
  await waitFor(()=>harness.eval(`window.__HARNESS_SESSION_PLUS_SKIN__?.revision===${JSON.stringify(disk.revision)}`));
  const child=spawn(path.resolve('desktop/src-tauri/target/release/hsp-desktop.exe'),[],{windowsHide:true,stdio:'ignore',env:{...process.env,WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:'--remote-debugging-port=9224 --remote-debugging-address=127.0.0.1'}});
  await waitFor(async()=>{ui=await connect(9224,'http://tauri.localhost/');return true;});
  await waitFor(()=>ui.eval(`!!window.__TAURI__?.core?.invoke`));
  await ui.eval(`window.__TAURI__.core.invoke('skin_action',{action:'reset',skinId:null,wallpaperClarity:70})`);
  await pause(3500);if(fs.existsSync(config))throw Error('Reset did not disable persistence');
  if(await harness.eval(`!!window.__HARNESS_SESSION_PLUS_SKIN__`))throw Error('Keeper restored after reset');
  await ui.eval(`window.__TAURI__.core.invoke('quit_app')`).catch(()=>{});ui.ws.close();await waitFor(()=>child.exitCode!==null);
  const report={nativeApplySaved:true,claritySaved:85,nativeQuitRetained:true,recoveredAfterToolExit:true,nativeResetStopsKeeper:true,realHarnessRestartTested:false};
  fs.writeFileSync('target/desktop-persistent-theme-live.json',JSON.stringify(report,null,2));console.log(JSON.stringify(report));
 }finally{
  ui?.ws.close();if(fs.existsSync(config))fs.unlinkSync(config);await pause(3500);
  if(saved)await harness.eval(`(()=>{const saved=window.__hspDesktopKeeperAcceptance;window.__HARNESS_SESSION_PLUS_SKIN__?.node.remove();if(saved.old){saved.old.node.textContent=saved.css;if(!saved.old.node.isConnected)document.head.append(saved.old.node);window.__HARNESS_SESSION_PLUS_SKIN__=saved.old;}else delete window.__HARNESS_SESSION_PLUS_SKIN__;delete window.__hspDesktopKeeperAcceptance;return true;})()`);
  harness.ws.close();
 }
})().catch(e=>{console.error(e);process.exitCode=1;});
