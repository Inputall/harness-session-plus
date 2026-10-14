// Explicit native Tauri acceptance with temporary local WebView debugging.
const fs=require('node:fs');
(async()=>{
 const targets=(await (await fetch('http://127.0.0.1:9224/json/list')).json()).filter(t=>t.type==='page'&&t.url==='http://tauri.localhost/');
 if(targets.length!==1)throw Error('Ambiguous desktop');
 const ws=new WebSocket(targets[0].webSocketDebuggerUrl);await new Promise((r,j)=>{ws.onopen=r;ws.onerror=j;});let next=1;const pending=new Map();
 ws.onmessage=e=>{const v=JSON.parse(e.data),p=pending.get(v.id);if(!p)return;pending.delete(v.id);clearTimeout(p.timer);v.error||v.result?.exceptionDetails?p.reject(Error(JSON.stringify(v))):p.resolve(v.result);};
 const call=(method,params)=>new Promise((resolve,reject)=>{const id=next++,timer=setTimeout(()=>reject(Error('Timeout')),65000);pending.set(id,{resolve,reject,timer});ws.send(JSON.stringify({id,method,params}));});
 const evaluate=async expression=>(await call('Runtime.evaluate',{expression,awaitPromise:true,returnByValue:true})).result.value;
 try{
  const local=await evaluate(`(async()=>{navigate('skins');await loadSkins();while(skinState.loading)await new Promise(r=>setTimeout(r,20));return {count:skinState.catalog.length,cards:document.querySelectorAll('.skin-card').length,first:skinState.catalog[4].name};})()`);
  if(local.count<44||local.cards!==12)throw Error('Catalog mismatch');
  const preview=await evaluate(`(async()=>{const images=[...document.querySelectorAll('.skin-wallpaper-preview')];for(const i of images)i.loading='eager';await Promise.all(images.map(i=>i.decode()));return {decoded:images.length,css:getComputedStyle(document.querySelector('.skin-card')).getPropertyValue('--skin-bg')};})()`);
  if(preview.decoded!==8||!preview.css)throw Error('Preview mismatch');
  const appearance=await evaluate(`(async()=>{document.getElementById('skin-filter').value='builtin';renderSkins();document.querySelector('.skin-card').click();await skinAction('apply');const applied=document.getElementById('skin-current').textContent;await skinAction('reset');return {applied,reset:document.getElementById('skin-current').textContent,feedback:document.getElementById('skin-feedback').textContent};})()`);
  if(appearance.applied!=='桥本有菜'||appearance.reset!=='Harness 默认')throw Error('Desktop apply/reset failed');
  const community=await evaluate(`(async()=>{await loadCommunity(0);const first=skinState.communityItems.find(s=>!s.conversionReason);await invoke('community_install',{versionId:first.id});skinState.catalog=[];await loadSkins();return {total:skinState.communityTotal,installed:skinState.catalog.find(s=>s.communityVersion===first.id)?.name};})()`);
  if(!community.installed)throw Error('Community install missing from native library');
  await evaluate(`(()=>{document.getElementById('skin-filter').value='market';document.getElementById('skin-search').value='';skinState.page=0;renderSkins();document.querySelector('.theme-toolbar').scrollIntoView();return true;})()`);
  const screenshot=await call('Page.captureScreenshot',{format:'png',captureBeyondViewport:false});fs.writeFileSync('target/skin-center-0.5.0.png',Buffer.from(screenshot.data,'base64'));
  fs.writeFileSync('target/desktop-theme-live-results.json',JSON.stringify({local,preview,appearance,community},null,2));console.log(JSON.stringify({local,preview,appearance,community}));
  await evaluate(`window.__TAURI__.core.invoke('quit_app')`).catch(()=>{});
 }finally{ws.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
