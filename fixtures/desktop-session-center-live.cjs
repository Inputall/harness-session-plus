// Real Tauri IPC + pinned Harness read-only acceptance. Never sends prompts or archives/deletes.
const fs=require('node:fs');
(async()=>{
 const targets=(await(await fetch('http://127.0.0.1:9224/json/list')).json()).filter(t=>t.type==='page'&&t.url==='http://tauri.localhost/');
 if(targets.length!==1)throw Error('Expected one local desktop test target');
 const ws=new WebSocket(targets[0].webSocketDebuggerUrl);await new Promise((r,j)=>{ws.onopen=r;ws.onerror=j;});let next=1;const pending=new Map();
 ws.onmessage=e=>{const v=JSON.parse(e.data),p=pending.get(v.id);if(!p)return;pending.delete(v.id);clearTimeout(p.timer);v.error||v.result?.exceptionDetails?p.reject(Error(JSON.stringify(v))):p.resolve(v.result);};
 const call=(method,params)=>new Promise((resolve,reject)=>{const id=next++,timer=setTimeout(()=>reject(Error('Timeout')),45000);pending.set(id,{resolve,reject,timer});ws.send(JSON.stringify({id,method,params}));});
 const evaluate=async expression=>(await call('Runtime.evaluate',{expression,awaitPromise:true,returnByValue:true})).result.value;
 try {
  const result=await evaluate(String.raw`(async()=>{
    const pause=()=>new Promise(r=>setTimeout(r,100));
    navigate('sessions');for(let i=0;i<200&&!document.querySelector('.session-card');i++)await pause();
    if(!document.querySelector('.session-card'))throw Error(document.getElementById('session-error').textContent||'No session cards');
    const cards=document.querySelectorAll('.session-card').length;
    document.getElementById('session-search').value='你好';document.getElementById('session-search-form').requestSubmit();
    for(let i=0;i<200&&document.getElementById('session-search-button').disabled;i++)await pause();
    const search=document.getElementById('session-count').textContent;
    if(!document.querySelector('.session-card'))throw Error('No body hit');
    document.querySelector('.session-card').click();for(let i=0;i<200&&document.getElementById('session-copy').disabled;i++)await pause();
    const prompt=document.getElementById('session-prompt').value;
    if(!prompt.includes('历史对话引用'))throw Error('No context');
    const exported=await invoke('session_export',{content:'# Acceptance only\nNo conversation content.'});
    const list=await invoke('session_query',{request:{action:'list'},expectedProfile:null});
    const active=list.sessions.find(s=>!s.archived&&!s.blank&&s.origin!=='subagent');
    if(!active)throw Error('No unarchived nonblank session for navigation test');
    document.getElementById('session-clear').click();
    [...document.querySelectorAll('.session-card')].find(c=>c.dataset.sessionId===active.sessionId).click();
    for(let i=0;i<200&&document.getElementById('session-watch-add').disabled;i++)await pause();
    document.getElementById('session-watch-add').click();for(let i=0;i<200&&!document.getElementById('session-watch-status').textContent.includes('1');i++)await pause();
    const watch=document.getElementById('session-watch-status').textContent;
    document.getElementById('session-watch-stop').click();await pause();
    const stop=document.getElementById('session-watch-status').textContent;
    document.getElementById('toast').textContent='';document.getElementById('session-open').click();
    for(let i=0;i<100&&!document.getElementById('toast').textContent.includes('已请求切换');i++)await pause();
    const navigation=document.getElementById('toast').textContent;
    let notification;try{await invoke('session_notification_test');notification='Windows API accepted';}catch(e){notification=e.message;}
    return {cards,search,excerptCount:document.querySelectorAll('.session-message').length,promptLength:prompt.length,exported:!!exported,watch,stop,navigation,notification,error:document.getElementById('session-error').textContent};
  })()`);
  if(result.error||!result.exported||!result.promptLength||!result.watch.includes('1')||result.stop!=='已停止'||!result.navigation.includes('已请求切换'))throw Error(JSON.stringify(result));
  const screenshot=await call('Page.captureScreenshot',{format:'png',captureBeyondViewport:false});fs.writeFileSync('target/session-center-live.png',Buffer.from(screenshot.data,'base64'));
  fs.writeFileSync('target/session-center-live-results.json',JSON.stringify(result,null,2));console.log(result);
 }finally{ws.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
