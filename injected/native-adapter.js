(() => {
  // Pinned to installed 0.2.0-rc.2; Rust verifies the ASAR digest before using this adapter.
  const rpc = async (endpoint, args, expectedOrigin) => {
    if(expectedOrigin && (await window.dshDesktopBoot.ready()).streamBaseUrl!==expectedOrigin)throw Error('Host generation changed');
    const rpcId = crypto.randomUUID();
    const response = await fetch(`/api/${endpoint}`, {method:'POST', credentials:'include', redirect:'error',
      signal:AbortSignal.timeout(5000), headers:{'content-type':'application/json'},
      body:JSON.stringify({type:'client-request',rpcId,method:endpoint,payload:{args}})});
    const value=await response.json();
    if(expectedOrigin && (await window.dshDesktopBoot.ready()).streamBaseUrl!==expectedOrigin)throw Error('Host generation changed');
    if(!response.ok||value.type!=='server-response'||value.rpcId!==rpcId)throw Error('Native protocol mismatch');
    if(value.result?.error){const error=Error(value.result.error.code||'Native request refused');error.searchDisabled=endpoint==='session/search'&&String(value.result.error.message).includes('openAt "never"');throw error;}
    if(!value.result || !Object.hasOwn(value.result,'value'))throw Error('Native result unavailable');
    return value.result.value;
  };
  const baseline = async () => {
    const boot=await window.dshDesktopBoot.ready();
    const origin=new URL(boot.streamBaseUrl);
    if(origin.protocol!=='http:'||origin.hostname!=='127.0.0.1'||origin.username||origin.password)throw Error('Host origin mismatch');
    return new Promise((resolve,reject)=>{
      const ws=new WebSocket(`ws://${origin.host}/api/remote.mux`), streamId=crypto.randomUUID();
      const finish=(error,value)=>{clearTimeout(timer);if(ws.readyState===1)ws.send(JSON.stringify({type:'cancel',streamId}));ws.close();error?reject(error):resolve(value);};
      const timer=setTimeout(()=>finish(Error('Native baseline timeout')),5000);
      ws.onopen=()=>ws.send(JSON.stringify({type:'open',streamId,endpoint:'workspace/follow',payload:{args:{}}}));
      ws.onmessage=e=>{try{const v=JSON.parse(e.data);if(v.streamId!==streamId)return;if(v.type==='error')finish(Error(v.error?.code||'Native baseline refused'));else if(v.type==='item'&&v.value?.type==='baseline')finish(null,v.value.value);}catch{finish(Error('Invalid baseline'));}};
      ws.onerror=()=>finish(Error('Native stream disconnected'));
    });
  };
  const snapshot=async()=>{
    const hostOrigin=(await window.dshDesktopBoot.ready()).streamBaseUrl;
    const [workspaces,sessions]=await Promise.all([baseline(),rpc('session/list',{_request:{}})]);
    if((await window.dshDesktopBoot.ready()).streamBaseUrl!==hostOrigin)throw Error('Host generation changed');
    if(!Array.isArray(workspaces.items)||!Array.isArray(workspaces.archivedSessionIds)||!Array.isArray(sessions.items))throw Error('Unsupported list');
    return {hostOrigin,workspaces, sessions:sessions.items.map(s=>({sessionId:s.sessionId,title:s.projections?.values?.title??'',cwd:s.cwd,running:s.running,blank:s.blank,updatedAt:s.updatedAt,origin:s.origin,cursor:s.projections?.kind==='sequenced'?s.projections.asOfSeq:null}))};
  };
  return {snapshot,baseline,rpc};
})()
