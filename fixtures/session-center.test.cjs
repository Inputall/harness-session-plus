// Adapter protocol/DOM fixtures only. Native acceptance is recorded separately.
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm');
const script=fs.readFileSync('injected/session-center.js','utf8');
function env({archived=false,disabled=false,fail=false,projectionKind='sequenced'}={}){
  const calls=[],opened=[];
  const sessions=[{sessionId:'a',title:'same title',running:false,blank:false},{sessionId:'b',title:'same title',running:true,blank:false}];
  const records=[{type:'event',event:{seq:1,type:'user/message',surfaceOp:'append',data:{content:[{type:'text',text:'find needle <script>bad()</script>'},{type:'image',data:'secret'}]}}},{type:'event',event:{seq:2,type:'assistant/message',surfaceOp:'append',data:{message:{content:[{type:'text',text:'result'}]}}}},{type:'event',event:{seq:3,type:'system/message',surfaceOp:'append',data:{content:[{type:'text',text:'hidden system'}]}}},{type:'event',event:{seq:4,type:'turn/end',data:{reason:{kind:'aborted'}}}}];
  const adapter={snapshot:async()=>({hostOrigin:'http://127.0.0.1:123',sessions,workspaces:{items:[{title:'workspace',sessionIds:['a','b']}],archivedSessionIds:archived?['a']:[]}}),rpc:async(name,args,origin)=>{
    calls.push({name,args,origin});assert.equal(origin,'http://127.0.0.1:123');
    if(name==='session/search'){if(disabled){const e=Error('gateway/internal');e.searchDisabled=true;throw e;}if(fail)throw Error('failed');return {items:[{sessionId:'a',snippet:'needle'}],hasMore:false};}
    if(name==='session/projections')return projectionKind==='sequenced'?{kind:'sequenced',asOfSeq:4,values:{tokenUsage:{uncachedInputTokens:125,outputTokens:30,cacheReadTokens:40,cacheWriteTokens:5,unexpected:999},contextPressure:{pressureTokens:170,projectedTokens:180,contextWindow:32768},modelSelection:{lastUsed:{provider:'provider-x',model:'model-y',reasoningEffort:'high',unexpected:'drop'},next:null},permissions:{currentValue:'workspace-write',unexpected:true},plan:{active:true,pending:false,extra:'ignored'}}}:projectionKind==='migration-required'?{kind:'migration-required',values:{tokenUsage:{uncachedInputTokens:999}}}:null;
    if(name==='skills/list')return {skills:[{name:'review',description:'Review files',modelInvocable:true,path:'C:\\skills\\review.md'},{name:'hidden',description:'',modelInvocable:false,extra:'drop'}]};
    if(name==='session/page'){assert.equal(args.request.address.kind,'session');assert.equal(args.request.maxMessages,20);return {records,hasMore:false};}
    throw Error('Unexpected endpoint '+name);
  }};
  const browser={open:id=>opened.push(id),searchSessions(){},useSessions(){},useWorkspaces(){}};
  const row={__reactFiber$x:{memoizedProps:{node:{id:'a',running:false,completed:false,pendingInteraction:'question'}},return:{memoizedProps:browser}}};
  const context=vm.createContext({document:{querySelectorAll:()=>[row]},window:{dshDesktopBoot:{ready:async()=>({streamBaseUrl:'http://127.0.0.1:123'})}},Date});
  const run=vm.runInContext(script,context);
  return {run:r=>run(adapter,r),calls,opened};
}
test('native hits keep unique IDs; detail reads only plain user/assistant text',async()=>{
  const e=env();const r=await e.run({action:'search',query:'needle'});assert.equal(r.searchMode,'native');assert.equal(r.hits[0].sessionId,'a');
  const d=await e.run({action:'detail',sessionId:'b'});assert.equal(d.messages.length,2);assert.match(d.messages[0].text,/<script>/);assert(!JSON.stringify(d.messages).includes('secret'));assert(!JSON.stringify(d.messages).includes('hidden system'));
  assert(e.calls.filter(c=>c.name==='session/page').every(c=>c.args.request.address.sessionId==='b'));
  assert.equal(d.lastTurn.reason,'aborted');
  assert.deepEqual(JSON.parse(JSON.stringify(d.stats)),{tokenUsage:{uncachedInputTokens:125,outputTokens:30,cacheReadTokens:40,cacheWriteTokens:5},contextPressure:{pressureTokens:170,projectedTokens:180,contextWindow:32768},modelSelection:{lastUsed:{provider:'provider-x',model:'model-y',reasoningEffort:'high'},next:null},permissionPreset:'workspace-write',planMode:{active:true,pending:false}});
});
test('migration-required projections never expose cached values as live statistics',async()=>{
  const e=env({projectionKind:'migration-required'}),d=await e.run({action:'detail',sessionId:'a'});
  assert.equal(d.projectionState,'migration-required');assert.equal(d.stats,null);assert.equal(d.messages.length,0);assert.equal(d.hasMore,true);
  assert(!e.calls.some(c=>c.name==='session/page'));
});
test('skills catalog is read-only, session-bound, and metadata is bounded',async()=>{
  const e=env(),r=await e.run({action:'skills',sessionId:'b'});
  assert.deepEqual(JSON.parse(JSON.stringify(r.skills)),[{name:'review',description:'Review files',whenToUse:'',modelInvocable:true},{name:'hidden',description:'',whenToUse:'',modelInvocable:false}]);
  const call=e.calls.find(c=>c.name==='skills/list');assert.equal(call.args.request.sessionId,'b');
});
test('disabled index falls back to bounded recent pages; arbitrary failures never silently fall back',async()=>{
  const e=env({disabled:true}),r=await e.run({action:'search',query:'needle'});assert.equal(r.searchMode,'recent');assert.equal(r.scanned,2);assert.equal(r.hits.length,2);assert.equal(r.failed,0);
  assert(e.calls.every(c=>['session/search','session/projections','session/page'].includes(c.name)));
  const broken=env({fail:true});await assert.rejects(()=>broken.run({action:'search',query:'needle'}));assert.equal(broken.calls.length,1);
});
test('navigation uses identity, never title; archived and unknown IDs cannot open',async()=>{
  const e=env();await e.run({action:'open',sessionId:'b'});assert.deepEqual(e.opened,['b']);
  await assert.rejects(()=>e.run({action:'open',sessionId:'same title'}));
  const archived=env({archived:true});await assert.rejects(()=>archived.run({action:'open',sessionId:'a'}));assert.equal(archived.opened.length,0);
  const list=await e.run({action:'list'});assert.equal(list.sessions[0].pending,'question');assert.equal(list.sessions[0].statusObserved,true);assert.equal(list.sessions[1].statusObserved,false);
});
