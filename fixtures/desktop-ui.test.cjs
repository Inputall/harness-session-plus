// DOM/IPC controller mocks only; these are not native-host or browser acceptance tests.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const html = fs.readFileSync(path.join(__dirname,'../desktop/ui/index.html'),'utf8');
const source = fs.readFileSync(path.join(__dirname,'../desktop/ui/app.js'),'utf8');
function environment(responder) {
  class Element {
    constructor(id='') {this.id=id;this.dataset={};this.children=[];this.events={};this.value='';this.disabled=false;this.hidden=false;this.checked=false;this.textContent='';this.classes=new Set();this.classList={toggle:(name,on)=>{if(on===undefined)on=!this.classes.has(name);on?this.classes.add(name):this.classes.delete(name);}};}
    addEventListener(name,fn){this.events[name]=fn;}
    focus(){this.focused=true;}click(){return this.events.click?.({preventDefault(){}});}
    setAttribute(){} removeAttribute(){}
    append(...nodes){this.children.push(...nodes);} prepend(node){this.children.unshift(node);}
    replaceChildren(...nodes){this.children=[...nodes];} add(node){this.children.push(node);}
    showModal(){this.open=true;}close(){this.open=false;}
    remove(){} get lastElementChild(){return {remove:()=>this.children.pop()};}
  }
  const elements = Object.fromEntries([...html.matchAll(/\bid="([^"]+)"/g)].map(match=>[match[1],new Element(match[1])]));
  const pages=Object.values(elements).filter(e=>e.id.startsWith('page-'));
  elements['session-filter'].value='all';
  const nav = ['overview','sessions','trash','connection','capabilities','skins','settings'].map(page=>{const el=new Element();el.dataset.page=page;return el;});
  const actions=['diagnose','inject_test','remove_injection','probe_capabilities'].map(action=>{const el=new Element();el.dataset.action=action;return el;});
  const navShortcuts=['connection','capabilities'].map(page=>{const el=new Element();el.dataset.nav=page;return el;});
  const calls=[];const confirmations=[];const intervals=new Map();let nextTimer=1;
  const settings={port:9223,target_id:null,executable:'C:\\Harness.exe',theme:'dark'};
  const document={getElementById:id=>elements[id],createElement:()=>new Element(),documentElement:{dataset:{}},querySelector:()=>null,querySelectorAll:selector=>({'.page':pages,'.nav-item':nav,'[data-page]':nav,'[data-nav]':navShortcuts,'[data-action]':actions}[selector]||[])};
  const invoke=async(command,args)=>{calls.push({command,args});if(command==='load_settings')return settings;if(command==='save_settings')return args.settings;return responder(command,args);};
  const listeners={},windowEvents={};
  const storage=new Map();const localStorage={getItem:key=>storage.get(key)||null,setItem:(key,value)=>storage.set(key,String(value))};
  const context=vm.createContext({document,window:{confirm:text=>{confirmations.push(text);return true;},addEventListener:(name,fn)=>{windowEvents[name]=fn;},__TAURI__:{core:{invoke},event:{listen:async(name,fn)=>{listeners[name]=fn;}}}},localStorage,Option:class extends Element {constructor(text,value){super();this.textContent=text;this.value=value;}},setTimeout:()=>nextTimer++,clearTimeout:()=>{},setInterval:fn=>{const id=nextTimer++;intervals.set(id,fn);return id;},clearInterval:id=>intervals.delete(id),Date,console});
  vm.runInContext(source,context);
  return {elements,nav,actions,calls,confirmations,intervals,context,listeners,windowEvents,storage,fire:async(id,event='click')=>{await elements[id].events[event]({preventDefault(){}});},settle:()=>new Promise(resolve=>setImmediate(resolve))};
}
const diagnostic={browser:'Mock/1',injection:{installed:false,dataOperations:false},pageRecognized:true};
test('theme UI uses neutral labels without changing stored theme identity',async()=>{
 assert(!/codex/i.test(html));
 const skin={id:'codex-snow-skin',name:'CodexPlusPlus Snow Skin',description:'由 Codex++ 转换的明亮主题',author:'Author / Codex contributors',license:'MIT',source:'https://github.com/example/Codex-Skin',converted:true};
 const env=environment(command=>command==='theme_library'?[skin]:{installed:true,active:skin.id,wallpaper:true});
 await env.settle();env.nav.find(n=>n.dataset.page==='skins').events.click();await env.settle();
 const card=env.elements['skin-catalog'].children[0];
 const text=node=>[node.textContent,...node.children.map(text)].join(' ');
 assert(!/codex/i.test(text(card)));
 await card.events.click();await env.fire('skin-apply');
 for(const id of ['skin-selection','skin-description','skin-source-info','skin-current','skin-feedback'])assert(!/codex/i.test(env.elements[id].textContent));
 assert.equal(env.calls.find(c=>c.command==='skin_action').args.skinId,'codex-snow-skin');
});
test('wallpaper clarity label and explicit apply send the selected numeric value',async()=>{
 const env=environment(command=>command==='theme_library'?[{id:'test',name:'Test'}]:{installed:true,active:'test',wallpaper:true});
 await env.settle();env.nav.find(n=>n.dataset.page==='skins').events.click();await env.settle();
 await env.elements['skin-catalog'].children[0].events.click();
 env.elements['skin-clarity'].value='85';await env.fire('skin-clarity','input');
 assert.equal(env.elements['skin-clarity-value'].textContent,'85%');
 assert.equal(env.calls.filter(c=>c.command==='skin_action').length,0);
 await env.fire('skin-apply');
 assert.equal(env.calls.find(c=>c.command==='skin_action').args.wallpaperClarity,85);
});
test('community pagination installs without applying; local search and pages remain bounded',async()=>{
  const catalog=Array.from({length:40},(_,i)=>({id:`theme-${i}`,name:`Theme ${i}`,category:'market'}));
  const env=environment((command,args)=>{if(command==='theme_library')return catalog;if(command==='community_page')return {total:741,items:[{id:'ver_test',name:'Community',authorDisplayName:'Author',license:'MIT',conversionReason:''}]};if(command==='community_install')return {id:'community-ver-test'};throw Error('Must not apply during install');});
  await env.settle();env.nav.find(n=>n.dataset.page==='skins').events.click();await env.settle();
  assert.equal(env.elements['skin-catalog'].children.length,12);await env.fire('skin-next');assert.equal(env.elements['skin-catalog'].children[0].children[1].textContent,'Theme 12');
  env.elements['skin-search'].value='Theme 39';await env.fire('skin-search','input');assert.equal(env.elements['skin-catalog'].children.length,1);
  await env.fire('community-refresh');assert.match(env.elements['community-info'].textContent,/741/);
  await env.elements['community-list'].children[0].children[3].events.click();await env.settle();
  assert.equal(env.calls.filter(c=>c.command==='community_install').length,1);assert.equal(env.calls.filter(c=>c.command==='skin_action').length,0);
  await env.fire('community-next');assert.equal(env.calls.filter(c=>c.command==='community_page').at(-1).args.offset,48);
});
test('local theme import saves conversion without automatically applying it',async()=>{
  const imported={id:'imported-test',name:'Local theme',converted:true};
  const env=environment((command,args)=>{if(command==='import_theme'){assert.equal(args.imageBase64,null);assert.equal(args.config,'{"schemaVersion":1}');return imported;}if(command==='theme_library')return [imported];throw Error('Import must not mutate Harness');});
  await env.settle();env.elements['theme-config-file'].files=[{size:19,text:async()=>'{"schemaVersion":1}'}];
  await env.fire('theme-import-form','submit');
  assert.equal(env.elements['skin-catalog'].children.length,1);assert.equal(env.elements['skin-apply'].disabled,true);
  assert.equal(env.calls.filter(c=>c.command==='skin_action').length,0);assert.match(env.elements['skin-feedback'].textContent,/Local theme/);
});
test('skin preview requires explicit apply and reset uses separate appearance IPC',async()=>{
  const catalog=JSON.parse(fs.readFileSync(path.join(__dirname,'../desktop/ui/skins.json'),'utf8'));
  const env=environment((command,args)=>{if(command==='theme_library')return catalog;assert.equal(command,'skin_action');return {supported:true,active:args.action==='apply'?args.skinId:null,installed:args.action==='apply'};});
  await env.settle();env.nav.find(n=>n.dataset.page==='skins').events.click();await env.settle();
  assert.equal(env.elements['skin-catalog'].children.length,4);
  env.elements['skin-catalog'].children[1].events.click();assert.equal(env.calls.filter(c=>c.command==='skin_action').length,0);
  await env.fire('skin-apply');assert.equal(env.elements['skin-current'].textContent,'暮色紫');
  const call=env.calls.find(c=>c.command==='skin_action');assert.equal(call.args.skinId,'lavender');assert.equal(call.args.action,'apply');
  await env.fire('skin-reset');assert.equal(env.elements['skin-current'].textContent,'Harness 默认');
});
test('recommendation opens the fixed backend website command and recovers after failure',async()=>{
  let finish;
  const env=environment((command,args)=>{assert.equal(command,'open_recommendation');assert.equal(Object.keys(args).length,0);return new Promise((resolve,reject)=>{finish=reject;});});
  await env.settle();const first=env.fire('open-recommendation');await env.settle();
  assert.equal(env.elements['open-recommendation'].disabled,true);
  await env.fire('open-recommendation');assert.equal(env.calls.filter(c=>c.command==='open_recommendation').length,1);
  finish({message:'Browser unavailable'});await first;
  assert.equal(env.elements['open-recommendation'].disabled,false);
  assert.equal(env.elements.toast.textContent,'Browser unavailable');
});
test('navigation and IPC results update UI without enabling mutation commands',async()=>{
  const env=environment((command,args)=>{assert.equal(command,'desktop_action');return args.action==='inject_test'?{installed:true,testButtons:1}:diagnostic;});
  await env.settle();await vm.runInContext('run("diagnose")',env.context);
  assert.equal(env.elements['metric-connection'].textContent,'已连接');
  assert.equal(env.elements['browser-version'].textContent,'Mock/1');
  await vm.runInContext('run("inject_test")',env.context);
  assert.equal(env.elements['metric-injection'].textContent,'已安装');
  assert.match(env.elements['injection-detail'].textContent,/1 个/);
  env.nav.find(n=>n.dataset.page==='trash').events.click();assert.equal(env.elements['page-trash'].hidden,false);assert.equal(env.elements['page-overview'].hidden,true);
  assert(env.calls.every(call=>!['archive_session','restore_session','permanently_delete_session'].includes(call.command)));
});
test('ambiguous connection is not success and requires explicit target selection',async()=>{
  const env=environment(()=>Promise.reject({message:'明确选择页面',target_ids:['page-a','page-b']}));await env.settle();
  await vm.runInContext('run("diagnose")',env.context);
  assert.equal(env.elements['metric-connection'].textContent,'未连接');assert.equal(env.elements['metric-injection'].textContent,'状态未知');
  assert.equal(env.elements['page-settings'].hidden,false);assert.equal(env.elements.target.children.length,3);
});
test('monitor switches off and pauses after three consecutive failures',async()=>{
  let fail=false;const env=environment(()=>fail?Promise.reject({message:'断线'}):diagnostic);await env.settle();
  await vm.runInContext('run("diagnose")',env.context);
  env.elements['auto-check'].checked=true;await env.fire('auto-check','change');assert.equal(env.intervals.size,1);
  env.elements['auto-check'].checked=false;await env.fire('auto-check','change');assert.equal(env.intervals.size,0);
  env.elements['auto-check'].checked=true;await env.fire('auto-check','change');fail=true;
  for(let i=0;i<3;i++)await vm.runInContext('run("diagnose",true)',env.context);
  assert.equal(env.intervals.size,0);assert.equal(env.elements['auto-check'].checked,false);
});
test('double-click does not start parallel actions; cleanup failure offers explicit exit',async()=>{
  let finish;const env=environment(()=>new Promise(resolve=>{finish=resolve;}));await env.settle();
  const first=vm.runInContext('run("inject_test")',env.context);await env.settle();
  await vm.runInContext('run("inject_test")',env.context);
  assert.equal(env.calls.filter(c=>c.command==='desktop_action').length,1);
  finish({installed:true,testButtons:0});await first;
  env.listeners['cleanup-failed']({payload:'无法确认卸载，请刷新页面'});
  assert.equal(env.elements['cleanup-dialog'].open,true);assert.match(env.elements['cleanup-message'].textContent,/刷新/);
});
test('recycle selection restores only selected IDs and renders title as text',async()=>{
  let restored=false;
  const records=[{sessionId:'id-a',title:'Codex <img src=x onerror=bad()>',workspaceTitle:'workspace',movedAt:1,state:'archived'},{sessionId:'id-b',title:'same',workspaceTitle:'workspace',movedAt:2,state:'unknown'}];
  const env=environment((command,args)=>{if(command==='refresh_trash')return restored?[]:records;if(command==='restore_sessions'){assert.deepEqual(Array.from(args.sessionIds),['id-a']);restored=true;return {outcomes:[{ok:true,sessionId:'id-a'}]};}throw Error('Unexpected command');});
  await env.settle();await env.fire('refresh-trash');
  const rows=env.elements['trash-list'].children;
  assert.equal(rows.length,2);assert.equal(rows[0].children[1].children[0].textContent,'编程助手 <img src=x onerror=bad()>');
  assert.equal(records[0].title,'Codex <img src=x onerror=bad()>');
  assert.equal(rows[1].children[0].disabled,true);assert.equal(rows[1].children[2].disabled,true);
  const check=rows[0].children[0];check.checked=true;check.events.change();
  assert.equal(env.elements['restore-selected'].disabled,false);
  await env.fire('restore-selected');assert.equal(restored,true);assert.equal(env.elements['restore-selected'].disabled,true);
});
test('permanent deletion check is read-only and displays backend refusal',async()=>{
  const env=environment((command,args)=>{
    if(command==='refresh_trash')return [{sessionId:'owned-id',title:'test',workspaceTitle:'ws',movedAt:1,state:'archived'}];
    assert.equal(command,'check_permanent_delete');assert.deepEqual(Array.from(args.sessionIds),['owned-id']);
    return {supported:false,dryRun:true,reason:'No durable deletion API',targets:[{sessionId:'owned-id',title:'test',eligible:false,reason:'Deletion refused'}]};
  });
  await env.settle();await env.fire('refresh-trash');
  const check=env.elements['trash-list'].children[0].children[0];check.checked=true;check.events.change();
  await env.fire('check-delete');assert.equal(env.elements['delete-report'].hidden,false);
  assert.equal(env.elements['delete-report'].children[1].textContent,'No durable deletion API');
  assert(env.calls.every(c=>!['delete_sessions','permanently_delete_session'].includes(c.command)));
});

const sessionSource=fs.readFileSync(path.join(__dirname,'../desktop/ui/session-center.js'),'utf8');
test('Ctrl+K opens a searchable session switcher and selects by real session ID',async()=>{
 const sessions=[{sessionId:'real-a',title:'Alpha task',workspace:'Workspace A',running:false,archived:false},{sessionId:'real-b',title:'Beta task',workspace:'Workspace B',running:false,archived:false}];
 const env=environment((command,args)=>{
  if(command==='session_query'&&args.request.action==='list')return {sessions,profile:'p'};
  if(command==='session_query'&&args.request.action==='detail')return {session:sessions.find(s=>s.sessionId===args.request.sessionId),messages:[],throughSeq:1,projectionState:'sequenced',profile:'p'};
  if(command==='session_query'&&args.request.action==='skills')return {skills:[]};
  if(command==='session_query'&&args.request.action==='jobs')return {jobs:[]};
  if(command==='favorite_list')return [];
  if(command==='session_tags_list')return [{sessionId:'real-a',tags:['focus']}];
  if(command==='workspace_git_status')return {clean:true,branch:'main',cwd:'D:/workspace',files:[],commits:[]};
  if(command==='checkpoint_list')return [];
  throw Error(command);
 });
 vm.runInContext(sessionSource,env.context);await env.settle();await env.settle();
 let prevented=false;env.windowEvents.keydown({ctrlKey:true,altKey:false,metaKey:false,key:'k',target:{matches:()=>false},preventDefault(){prevented=true;}});await env.settle();await env.settle();
 assert.equal(prevented,true);assert.equal(env.nav.find(n=>n.dataset.page==='sessions').classes.has('active'),true);assert.equal(env.elements['session-switcher-dialog'].open,true);
 env.elements['session-switcher-search'].value='focus';await env.fire('session-switcher-search','input');
 assert.equal(env.elements['session-switcher-results'].children.length,1);assert.equal(env.elements['session-switcher-results'].children[0].children[0].textContent,'Alpha task');
 env.elements['session-switcher-search'].value='beta';await env.fire('session-switcher-search','input');
 assert.equal(env.elements['session-switcher-results'].children.length,1);
 await env.elements['session-switcher-results'].children[0].events.click();await env.settle();
 assert.equal(env.elements['session-switcher-dialog'].open,false);assert.equal(env.elements['session-detail-title'].textContent,'Beta task');
 const detail=env.calls.find(c=>c.command==='session_query'&&c.args.request.action==='detail');assert.equal(detail.args.request.sessionId,'real-b');assert.equal(detail.args.expectedProfile,'p');
});
test('task observer summarizes verified session states and records only captured watcher activity',async()=>{
 const sessions=[
  {sessionId:'run-a',title:'Running task',workspace:'Project A',running:true,archived:false,statusObserved:true},
  {sessionId:'wait-b',title:'Needs approval',workspace:'Project B',running:true,archived:false,statusObserved:true,pending:'approval'},
  {sessionId:'done-c',title:'Finished task',workspace:'Project C',running:false,archived:false,statusObserved:true,completed:true},
  {sessionId:'unknown-d',title:'Unobserved task',workspace:'Project D',running:false,archived:false,statusObserved:false},
  {sessionId:'archived-e',title:'Archived',workspace:'Old',running:false,archived:true,statusObserved:true}
 ];
 const env=environment((command,args)=>{
  if(command==='session_query'&&args.request.action==='list')return {sessions,profile:'p'};
  if(command==='session_query'&&args.request.action==='detail'){const session=sessions.find(item=>item.sessionId===args.request.sessionId);return {session,messages:[],throughSeq:1,projectionState:'sequenced',profile:'p'};}
  if(command==='session_query'&&args.request.action==='skills')return {skills:[]};
  if(command==='session_query'&&args.request.action==='jobs')return {jobs:[]};
  if(['favorite_list','session_tags_list','session_notes_list','checkpoint_list','prompt_template_list'].includes(command))return [];
  throw Error(command);
 });
 vm.runInContext(sessionSource,env.context);await env.settle();await env.settle();env.nav.find(n=>n.dataset.page==='sessions').events.click();await env.settle();
 assert.equal(env.elements['session-observer-running'].textContent,'2');assert.equal(env.elements['session-observer-waiting'].textContent,'1');assert.equal(env.elements['session-observer-completed'].textContent,'1');assert.equal(env.elements['session-observer-unknown'].textContent,'1');
 assert.equal(env.elements['session-observer-queue'].children.length,3);assert.equal(env.elements['session-observer-queue'].children[0].children[1].textContent,'等待审批');
 assert.equal(env.elements['session-pending-center-list'].children.length,1);assert.match(env.elements['session-pending-center-status'].textContent,/1 个已观测待处理/);
 await env.elements['session-observer-queue'].children[0].events.click();await env.settle();assert.equal(env.elements['session-detail-title'].textContent,'Needs approval');
 env.listeners['session-notice']({payload:{profile:'p',sessionId:'wait-b',title:'Needs approval',kind:'等待审批'}});assert.equal(env.elements['session-observer-activity'].children.length,1);assert.equal(env.elements['session-observer-activity'].children[0].children[2].textContent,'等待审批');
 env.listeners['session-notice']({payload:{profile:'other',sessionId:'x',title:'Other',kind:'等待回答'}});assert.equal(env.elements['session-observer-activity'].children.length,1);
 assert(env.calls.every(call=>!['session_send','job_cancel','job_kill','session_inject','session_steer'].includes(call.command)));
});
test('cross-session job overview scans bounded non-archived sessions, limits concurrency, and hides commands',async()=>{
 const sessions=Array.from({length:22},(_,i)=>({sessionId:`run-${i}`,title:`Running ${i}`,workspace:'Project',running:true,archived:false}));sessions.push({sessionId:'idle',title:'Idle',workspace:'Project',running:false,archived:false});
 let active=0,maxActive=0;const env=environment(async(command,args)=>{
  if(command==='session_query'&&args.request.action==='list')return {sessions,profile:'p'};
  if(command==='session_query'&&args.request.action==='jobs'){
   active++;maxActive=Math.max(maxActive,active);await new Promise(resolve=>setTimeout(resolve,1));active--;
   return {jobs:[{id:`job-${args.request.sessionId}`,kind:'shell',command:'sensitive command',status:'running',progress:'checks in progress',startedAt:100}]};
  }
  if(command==='session_query'&&args.request.action==='detail')return {session:sessions.find(s=>s.sessionId===args.request.sessionId),messages:[],throughSeq:0,projectionState:'sequenced'};
  if(command==='session_query'&&args.request.action==='skills')return {skills:[]};
  if(command==='session_query'&&args.request.action==='jobs')return {jobs:[]};
  if(['favorite_list','session_tags_list','session_notes_list','checkpoint_list','prompt_template_list'].includes(command))return [];
  throw Error(command);
 });
 vm.runInContext(sessionSource,env.context);await env.settle();await env.settle();env.nav.find(n=>n.dataset.page==='sessions').events.click();await env.settle();
 await env.fire('session-observer-jobs-refresh');
 assert.equal(env.calls.filter(c=>c.command==='session_query'&&c.args.request.action==='jobs').length,20);assert(maxActive<=3);assert.match(env.elements['session-observer-jobs-status'].textContent,/前 20\/23 个会话/);
 assert.equal(env.elements['session-observer-jobs'].children.length,20);assert.doesNotMatch(env.elements['session-observer-jobs'].children[0].textContent,/sensitive command/);assert.match(env.elements['session-observer-jobs'].children[0].children[0].children[2].textContent,/checks in progress/);
 await env.elements['session-observer-jobs'].children[0].children[0].events.click();assert.match(env.elements['session-detail-title'].textContent,/Running/);
 assert(env.calls.every(call=>!['job_cancel','job_kill','session_send','session_inject','session_steer'].includes(call.command)));
});
test('job details show verified timestamps and long-running warning without reading task output',async()=>{
 const session={sessionId:'job-session',title:'Long task',workspace:'Project',running:true,archived:false};let jobReads=0;
 const env=environment((command,args)=>{
  if(command==='session_query'&&args.request.action==='list')return {sessions:[session],profile:'p'};
  if(command==='session_query'&&args.request.action==='detail')return {session,messages:[],throughSeq:1,projectionState:'sequenced'};
  if(command==='session_query'&&args.request.action==='skills')return {skills:[]};
  if(command==='session_query'&&args.request.action==='jobs'){jobReads++;return {jobs:[{id:'job-1',kind:'shell',command:'hidden from overview',status:'running',progress:'step 2/4',startedAt:Date.now()-3600000,finishedAt:null,retainedBytes:12}]};}
  if(['favorite_list','session_tags_list','session_notes_list','checkpoint_list','prompt_template_list'].includes(command))return [];
  throw Error(command);
 });
 vm.runInContext(sessionSource,env.context);await env.settle();await env.settle();env.nav.find(n=>n.dataset.page==='sessions').events.click();await env.settle();await env.elements['session-results'].children[0].events.click();await env.settle();
 const jobs=env.elements['session-jobs'],text=n=>[n.textContent,...n.children.map(text)].join(' ');assert.equal(jobReads,1);assert.match(text(jobs.children[0]),/运行超过 30 分钟/);assert.match(text(jobs.children[0]),/开始/);assert.match(text(jobs.children[0]),/已运行 1 小时/);assert.match(text(jobs.children[0]),/step 2\/4/);
 assert(env.calls.every(call=>!['job_output','job_cancel','job_kill'].includes(call.command)));
});
test('run statistics sum only available persisted counters and disclose bounded sample gaps',async()=>{
 const sessions=Array.from({length:22},(_,i)=>({sessionId:`stats-${i}`,title:`Stats ${i}`,workspace:'Project',running:false,archived:false}));let active=0,maxActive=0;
 const env=environment(async(command,args)=>{
  if(command==='session_query'&&args.request.action==='list')return {sessions,profile:'stats-profile'};
  if(command==='session_query'&&args.request.action==='detail'){
   active++;maxActive=Math.max(maxActive,active);await new Promise(resolve=>setTimeout(resolve,1));active--;
   const i=Number(args.request.sessionId.slice(6));return {session:sessions[i],projectionState:i===1?'unavailable':'sequenced',stats:{tokenUsage:i===1?{}:{uncachedInputTokens:10,outputTokens:4,cacheReadTokens:2,cacheWriteTokens:1}}};
  }
  if(command==='session_query'&&args.request.action==='skills')return {skills:[]};
  if(command==='session_query'&&args.request.action==='jobs')return {jobs:[]};
  if(['favorite_list','session_tags_list','session_notes_list','checkpoint_list','prompt_template_list'].includes(command))return [];
  throw Error(command);
 });
 vm.runInContext(sessionSource,env.context);await env.settle();await env.settle();env.nav.find(n=>n.dataset.page==='sessions').events.click();await env.settle();
 await env.fire('session-run-stats-refresh');await env.settle();
 assert.equal(env.calls.filter(c=>c.command==='session_query'&&c.args.request.action==='detail').length,20);assert(maxActive<=3);
 const text=n=>[n.textContent,...n.children.map(text)].join(' ');assert.match(text(env.elements['session-run-stats-results']),/190 · 1 个会话缺失/);assert.match(env.elements['session-run-stats-status'].textContent,/20\/22 个会话/);assert.match(env.elements['session-run-stats-status'].textContent,/stats-profile/);
});
test('prompt templates save locally, filter, insert without sending, and delete by template ID',async()=>{
 const session={sessionId:'session-1',title:'Current task',workspace:'Project',running:false,archived:false};
 const templates=[];
 const env=environment((command,args)=>{
  if(command==='session_query'&&args.request.action==='list')return {sessions:[session],profile:'p'};
  if(command==='session_query'&&args.request.action==='detail')return {session,messages:[],throughSeq:1,projectionState:'sequenced',profile:'p'};
  if(command==='session_query'&&args.request.action==='skills')return {skills:[]};
  if(command==='session_query'&&args.request.action==='jobs')return {jobs:[]};
  if(command==='favorite_list'||command==='session_tags_list'||command==='checkpoint_list')return [];
  if(command==='prompt_template_list')return templates;
  if(command==='prompt_template_save'){const saved={...args.template,id:args.template.id||'template-1',updatedAt:123};templates.splice(0,templates.length,saved);return saved;}
  if(command==='prompt_template_delete'){const i=templates.findIndex(item=>item.id===args.id);if(i>=0)templates.splice(i,1);return null;}
  if(command==='workspace_git_status')return {clean:true,branch:'main',cwd:'D:/workspace',files:[],commits:[]};
  throw Error(command);
 });
 vm.runInContext(sessionSource,env.context);env.context.window.sessionCenterOpen();await env.settle();await env.settle();
 await env.elements['session-results'].children[0].events.click();await env.settle();await env.settle();await env.settle();
 await env.fire('session-template-new');env.elements['session-template-name'].value='Review';env.elements['session-template-category'].value='Quality';env.elements['session-template-content'].value='Inspect the diff and run focused tests.';
 let prevented=false;await env.elements['session-template-form'].events.submit({preventDefault(){prevented=true;}});await env.settle();
 assert.equal(prevented,true);assert.equal(env.elements['session-template-dialog'].open,false);assert.equal(templates.length,1);
 env.elements['session-template-search'].value='quality';await env.fire('session-template-search','input');
 const row=env.elements['session-templates'].children[0];assert.equal(row.children[0].children[0].textContent,'Review');
 await row.children[1].children[0].events.click();
 assert.equal(env.elements['session-next-task'].value,'Inspect the diff and run focused tests.');assert.match(env.elements['session-prompt'].value,/本次请求：Inspect the diff and run focused tests\./);
 assert.equal(env.calls.some(c=>c.command==='session_query'&&c.args.request.action==='send'),false);
 await row.children[1].children[2].events.click();await env.settle();assert.equal(templates.length,0);
 assert.ok(env.calls.some(c=>c.command==='prompt_template_delete'&&c.args.id==='template-1'));
});
test('session notes load by profile and real ID, update the continuation prompt, and save locally',async()=>{
 const session={sessionId:'real-session-id',title:'Implementation',workspace:'Project',running:false,archived:false};
 let saved=null;
 const env=environment((command,args)=>{
  if(command==='session_query'&&args.request.action==='list')return {sessions:[session],profile:'profile-a'};
  if(command==='session_query'&&args.request.action==='detail')return {session,messages:[],throughSeq:4,projectionState:'sequenced',profile:'profile-a'};
  if(command==='session_query'&&args.request.action==='skills')return {skills:[]};
  if(command==='session_query'&&args.request.action==='jobs')return {jobs:[]};
  if(command==='favorite_list'||command==='session_tags_list'||command==='checkpoint_list'||command==='prompt_template_list')return [];
  if(command==='session_notes_list')return [{profile:'profile-a',sessionId:'real-session-id',content:'Keep compatibility with the existing file format.',updatedAt:1}];
  if(command==='session_notes_save'){saved=args.note;return null;}
  if(command==='workspace_git_status')return {clean:true,branch:'main',cwd:'D:/workspace',files:[],commits:[]};
  throw Error(command);
 });
 vm.runInContext(sessionSource,env.context);env.context.window.sessionCenterOpen();await env.settle();await env.settle();
 await env.elements['session-results'].children[0].events.click();await env.settle();await env.settle();
 assert.equal(env.elements['session-note'].value,'Keep compatibility with the existing file format.');
 assert.match(env.elements['session-prompt'].value,/本机备注（用户记录/);assert.match(env.elements['session-prompt'].value,/Keep compatibility with the existing file format\./);
 env.elements['session-note'].value='Preserve the public JSON shape.';await env.fire('session-note','input');
 assert.match(env.elements['session-prompt'].value,/Preserve the public JSON shape\./);
 await env.fire('session-note-save');
 assert.equal(saved.profile,'profile-a');assert.equal(saved.sessionId,'real-session-id');assert.equal(saved.content,'Preserve the public JSON shape.');
});
test('late session-note loading does not overwrite an in-progress edit',async()=>{
 const session={sessionId:'session-1',title:'Current',workspace:'Project',running:false,archived:false};let resolveNotes;
 const pendingNotes=new Promise(resolve=>{resolveNotes=resolve;});
 const env=environment((command,args)=>{
  if(command==='session_query'&&args.request.action==='list')return {sessions:[session],profile:'p'};
  if(command==='session_query'&&args.request.action==='detail')return {session,messages:[],throughSeq:1,projectionState:'sequenced',profile:'p'};
  if(command==='session_query'&&args.request.action==='skills')return {skills:[]};
  if(command==='session_query'&&args.request.action==='jobs')return {jobs:[]};
  if(command==='session_notes_list')return pendingNotes;
  if(command==='favorite_list'||command==='session_tags_list'||command==='checkpoint_list'||command==='prompt_template_list')return [];
  if(command==='workspace_git_status')return {clean:true,branch:'main',cwd:'D:/workspace',files:[],commits:[]};
  throw Error(command);
 });
 vm.runInContext(sessionSource,env.context);env.context.window.sessionCenterOpen();await env.settle();await env.settle();
 await env.elements['session-results'].children[0].events.click();await env.settle();
 env.elements['session-note'].value='Unsaved draft';await env.fire('session-note','input');
 resolveNotes([{profile:'p',sessionId:'session-1',content:'Older saved note',updatedAt:1}]);await env.settle();await env.settle();
 assert.equal(env.elements['session-note'].value,'Unsaved draft');assert.match(env.elements['session-prompt'].value,/Unsaved draft/);
});
test('session favorites are profile-scoped, filter by identity, and stay local',async()=>{
 const first={sessionId:'a',title:'Pinned task',workspace:'ws',running:false,archived:false};
 const second={sessionId:'b',title:'Other task',workspace:'ws',running:false,archived:false};
 let favorite=[];
 const env=environment((command,args)=>{
  if(command==='session_query'&&args.request.action==='list')return {sessions:[first,second],profile:'p'};
  if(command==='session_query'&&args.request.action==='detail')return {session:args.request.sessionId==='a'?first:second,messages:[],throughSeq:1,profile:'p',projectionState:'sequenced',stats:{}};
  if(command==='favorite_list'){assert.equal(args.profile,'p');return favorite;}
  if(command==='favorite_toggle'){assert.equal(args.favorite.profile,'p');favorite=favorite.length?[]:[args.favorite];return !favorite.length?false:true;}
  throw Error(command);
 });
 vm.runInContext(sessionSource,env.context);await env.settle();env.nav.find(n=>n.dataset.page==='sessions').events.click();await env.settle();await env.settle();
 await env.elements['session-results'].children[0].events.click();await env.settle();assert.equal(env.elements['session-favorite-toggle'].textContent,'收藏会话');
 await env.fire('session-favorite-toggle');await env.settle();assert.equal(env.elements['session-favorite-toggle'].textContent,'取消收藏');
 env.elements['session-filter'].value='favorite';env.elements['session-filter'].events.change();assert.equal(env.elements['session-results'].children.length,1);assert.equal(env.elements['session-results'].children[0].dataset.sessionId,'a');
 await env.elements['session-results'].children[0].events.click();await env.settle();await env.fire('session-favorite-toggle');await env.settle();assert.equal(env.elements['session-favorite-toggle'].textContent,'收藏会话');
 assert(env.calls.every(c=>!['delete_sessions','archive_session','restore_session'].includes(c.command)));
});
test('session tags save locally, search by tag and clear without mutating sessions',async()=>{
 const first={sessionId:'a',title:'Task alpha',workspace:'ws',running:false,archived:false};
 const second={sessionId:'b',title:'Task beta',workspace:'ws',running:false,archived:false};
 let tags=[];
 const env=environment((command,args)=>{
  if(command==='session_query'&&args.request.action==='list')return {sessions:[first,second],profile:'p'};
  if(command==='session_query'&&args.request.action==='detail')return {session:args.request.sessionId==='a'?first:second,messages:[],throughSeq:1,profile:'p',projectionState:'sequenced',stats:{}};
  if(command==='session_tags_list')return tags;
  if(command==='session_tags_save'){assert.equal(args.item.profile,'p');tags=tags.filter(item=>item.sessionId!==args.item.sessionId);if(args.item.tags.length)tags.push(args.item);return null;}
  throw Error(command);
 });
 vm.runInContext(sessionSource,env.context);await env.settle();env.nav.find(n=>n.dataset.page==='sessions').events.click();await env.settle();await env.settle();
 await env.elements['session-results'].children[0].events.click();await env.settle();
 env.elements['session-tags-input'].value='release, follow-up';await env.fire('session-tags-save');
 assert.deepEqual(Array.from(tags[0].tags),['release','follow-up']);const text=node=>[node.textContent,...node.children.map(text)].join(' ');assert.match(text(env.elements['session-results'].children[0]),/release/);
 env.elements['session-search'].value='follow-up';await env.fire('session-search','input');assert.equal(env.elements['session-results'].children.length,1);
 env.elements['session-tags-input'].value='release, release';await env.fire('session-tags-save');assert.deepEqual(Array.from(tags[0].tags),['release']);
 env.elements['session-tags-input'].value='';await env.fire('session-tags-save');assert.equal(tags.length,0);
 assert(env.calls.every(c=>!['delete_sessions','archive_session','restore_session'].includes(c.command)));
});
test('batch organizer applies favorites and additive tags per session without entering compare mode',async()=>{
 const sessions=['a','b'].map((id,i)=>({sessionId:id,title:`Task ${id}`,workspace:'ws',running:false,archived:false}));
 let favorites=[],tags=[];
 const env=environment((command,args)=>{
  if(command==='session_query'&&args.request.action==='list')return {sessions,profile:'p'};
  if(command==='session_query'&&args.request.action==='detail')return {session:sessions.find(s=>s.sessionId===args.request.sessionId),messages:[],throughSeq:1,profile:'p'};
  if(command==='favorite_list')return favorites;
  if(command==='favorite_toggle'){const id=args.favorite.sessionId;favorites=favorites.some(x=>x.sessionId===id)?favorites.filter(x=>x.sessionId!==id):[...favorites,args.favorite];return favorites.some(x=>x.sessionId===id);}
  if(command==='session_tags_list')return tags;
  if(command==='session_tags_save'){tags=tags.filter(x=>x.sessionId!==args.item.sessionId);if(args.item.tags.length)tags.push(args.item);return null;}
  throw Error(command);
 });
 vm.runInContext(sessionSource,env.context);await env.settle();env.nav.find(n=>n.dataset.page==='sessions').events.click();await env.settle();await env.settle();
 await env.fire('session-batch-mode');
 const rows=()=>env.elements['session-results'].children.filter(x=>x.dataset.sessionId);
 await rows()[0].events.click();await rows()[1].events.click();assert.equal(env.elements['session-batch-count'].textContent,'已选择 2 个会话');
 await env.fire('session-batch-favorite');assert.equal(favorites.length,2);
 env.elements['session-batch-tags'].value='release, urgent';await env.fire('session-batch-tag-save');assert.equal(tags.length,2);assert.deepEqual(Array.from(tags[0].tags),['release','urgent']);
 assert.equal(env.elements['session-compare-run'].disabled,true);assert.equal(env.elements['session-batch-actions'].hidden,false);
 await env.fire('session-batch-unfavorite');assert.equal(favorites.length,0);
 await env.fire('session-batch-mode');assert.equal(env.elements['session-batch-actions'].hidden,true);
 assert(env.calls.every(c=>!['delete_sessions','archive_session','restore_session'].includes(c.command)));
});
test('batch tag limit failure leaves existing tags intact and continues other sessions',async()=>{
 const sessions=['a','b'].map(id=>({sessionId:id,title:`Task ${id}`,workspace:'ws',running:false,archived:false}));
 let tags=[{profile:'p',sessionId:'b',tags:Array.from({length:8},(_,i)=>`tag${i}`)}];
 const env=environment((command,args)=>{
  if(command==='session_query'&&args.request.action==='list')return {sessions,profile:'p'};
  if(command==='session_query'&&args.request.action==='detail')return {session:sessions.find(s=>s.sessionId===args.request.sessionId),messages:[],throughSeq:1,profile:'p'};
  if(command==='favorite_list')return [];
  if(command==='session_tags_list')return tags;
  if(command==='session_tags_save'){tags=tags.filter(x=>x.sessionId!==args.item.sessionId);if(args.item.tags.length)tags.push(args.item);return null;}
  throw Error(command);
 });
 vm.runInContext(sessionSource,env.context);await env.settle();env.nav.find(n=>n.dataset.page==='sessions').events.click();await env.settle();await env.settle();
 await env.fire('session-batch-mode');const rows=()=>env.elements['session-results'].children.filter(x=>x.dataset.sessionId);await rows()[0].events.click();await rows()[1].events.click();
 env.elements['session-batch-tags'].value='urgent';await env.fire('session-batch-tag-save');
 assert.deepEqual(Array.from(tags.find(x=>x.sessionId==='a').tags),['urgent']);assert.deepEqual(Array.from(tags.find(x=>x.sessionId==='b').tags),Array.from({length:8},(_,i)=>`tag${i}`));
 assert.match(env.elements['session-notification-feedback'].textContent,/Task b/);
});
test('batch context export writes one bounded markdown file and skips unsequenced sessions',async()=>{
 const sessions=['a','b'].map(id=>({sessionId:id,title:`Task ${id}`,workspace:'ws',running:false,archived:false}));let exported;
 const env=environment((command,args)=>{
  if(command==='session_query'&&args.request.action==='list')return {sessions,profile:'p'};
  if(command==='session_query'&&args.request.action==='detail')return args.request.sessionId==='a'?{session:sessions[0],messages:[{seq:4,role:'user',text:'verified question'},{seq:5,role:'tool',text:'hidden tool'}],throughSeq:5,hasMore:true,projectionState:'sequenced',profile:'p'}:{session:sessions[1],messages:[{seq:2,role:'assistant',text:'unverified'}],projectionState:'unavailable',profile:'p'};
  if(command==='session_export'){exported=args.content;return 'D:/exports/batch.md';}
  if(command==='favorite_list')return [];
  if(command==='session_tags_list')return [];
  throw Error(command);
 });
 vm.runInContext(sessionSource,env.context);await env.settle();env.nav.find(n=>n.dataset.page==='sessions').events.click();await env.settle();await env.settle();await env.fire('session-batch-mode');
 const rows=()=>env.elements['session-results'].children.filter(x=>x.dataset.sessionId);await rows()[0].events.click();await rows()[1].events.click();await env.fire('session-batch-export');
 assert.match(exported,/Task a/);assert.match(exported,/verified question/);assert.match(exported,/截止事件：5/);assert.doesNotMatch(exported,/hidden tool|unverified/);
 assert.equal(env.calls.filter(c=>c.command==='session_export').length,1);assert.equal(env.calls.filter(c=>c.command==='session_query'&&c.args.request.action==='detail').length,2);
});
test('session comparison is profile-bound, read-only and limited to two sessions',async()=>{
 const sessions=['a','b','c'].map((id,i)=>({sessionId:id,title:`Task ${id}`,workspace:'ws',running:false,archived:false}));
 const detailCalls=[];
 const env=environment((command,args)=>{
  if(command==='session_query'&&args.request.action==='list')return {sessions,profile:'p'};
  if(command==='session_query'&&args.request.action==='detail'){
   detailCalls.push(args.request.sessionId);const session=sessions.find(s=>s.sessionId===args.request.sessionId);
   return {session,messages:[],throughSeq:1,profile:'p',projectionState:'sequenced',stats:{modelSelection:{lastUsed:{provider:'local',model:`model-${session.sessionId}`}},permissionPreset:'workspace-write',planMode:{active:false,pending:false},tokenUsage:{uncachedInputTokens:100,outputTokens:20}}};
  }
  throw Error(command);
 });
 vm.runInContext(sessionSource,env.context);await env.settle();env.nav.find(n=>n.dataset.page==='sessions').events.click();await env.settle();
 await env.elements['session-results'].children[0].events.click();await env.settle();
 assert.equal(env.elements['session-detail-title'].textContent,'Task a');
 await env.fire('session-compare-mode');
 const rows=()=>env.elements['session-results'].children.filter(n=>n.dataset.sessionId);
 await rows()[0].events.click();await rows()[1].events.click();assert.equal(env.elements['session-compare-run'].disabled,false);
 await rows()[2].events.click();assert.equal(env.elements['session-compare-run'].disabled,false);
 detailCalls.length=0;await env.fire('session-compare-run');
 assert.deepEqual(detailCalls,['a','b']);assert.equal(env.elements['session-compare-panel'].hidden,false);
 const usageText=n=>[n.textContent,...n.children.map(usageText)].join(' ');
 assert.match(usageText(env.elements['session-compare-results']),/model-a/);assert.match(usageText(env.elements['session-compare-results']),/model-b/);
 assert.match(usageText(env.elements['session-compare-results']),/100/);
 assert(env.calls.filter(c=>c.command==='session_query'&&c.args.request.action==='detail').every(c=>c.args.expectedProfile==='p'));
 assert(env.calls.every(c=>!['delete_sessions','archive_session','restore_session'].includes(c.command)));
});
test('session center searches body, copies bounded context, exports and navigates by ID',async()=>{
 const s={sessionId:'a',title:'Codex++ <img onerror=x>',workspace:'ws',cwd:'D:/work',running:false,archived:false};
 const env=environment((command,args)=>{
  if(command==='session_query'){
   if(args.request.action==='list')return {sessions:[s],profile:'profile-a'};
   if(args.request.action==='search')return {sessions:[s],profile:'profile-a',hits:[{sessionId:'a',snippet:'needle in body'}],searchMode:'recent',scanned:1,hasMore:true};
   if(args.request.action==='detail')return {session:s,messages:[{seq:1,role:'user',text:'needle in body'},{seq:3,role:'assistant',text:'verified response'}],lastTurn:{seq:4,reason:'aborted'},throughSeq:4,hasMore:true,profile:'profile-a',projectionState:'sequenced',stats:{permissionPreset:'workspace-write',planMode:{active:true,pending:false},tokenUsage:{uncachedInputTokens:125,outputTokens:30},contextPressure:{pressureTokens:160},modelSelection:{lastUsed:{provider:'provider-x',model:'model-y'},next:null}}};
   if(args.request.action==='skills')return {skills:[{name:'review',description:'Read-only review',whenToUse:'Before changes',modelInvocable:true}]};
   if(args.request.action==='jobs')return {jobs:[{id:'job-1',kind:'shell',command:'npm test',status:'running',progress:'running tests',detail:'',retainedBytes:42}]};
   if(args.request.action==='open')return {opened:true};
  }
  if(command==='session_export')return 'D:/export.md';
  if(command==='session_watch')return null;
  throw Error(command);
 });
 let copied='';env.context.navigator={clipboard:{writeText:async text=>{copied=text;}}};vm.runInContext(sessionSource,env.context);await env.settle();
 env.nav.find(n=>n.dataset.page==='sessions').events.click();await env.settle();
 assert.equal(env.elements['session-results'].children[0].children[0].textContent,'编程助手 <img onerror=x>');
 env.elements['session-search'].value='needle';await env.fire('session-search-form','submit');
 assert.match(env.elements['session-search-status'].textContent,/1 个正文命中/);
 assert.match(env.elements['session-count'].textContent,/索引关闭/);
 await env.elements['session-results'].children[0].events.click();await env.settle();
 await env.settle();
 assert.equal(env.elements['session-detail-title'].textContent,'编程助手 <img onerror=x>');
 assert.match(env.elements['session-timeline-status'].textContent,/3 条可验证记录.*最近一页/);assert.equal(env.elements['session-excerpts'].children.length,3);assert.equal(env.elements['session-excerpts'].children[0].children[0].children[0].textContent,'#1 · 用户');assert.equal(env.elements['session-excerpts'].children[2].children[0].children[0].textContent,'#4 · 轮次结束');assert.equal(env.elements['session-excerpts'].children[2].children[1].textContent,'结束原因（原生值）：aborted');
 const usageText=n=>[n.textContent,...n.children.map(usageText)].join(' ');assert.match(usageText(env.elements['session-usage']),/workspace-write/);assert.match(usageText(env.elements['session-usage']),/计划模式/);assert.match(usageText(env.elements['session-usage']),/125/);assert.match(usageText(env.elements['session-usage']),/Read-only review/);
 assert.match(usageText(env.elements['session-jobs']),/npm test/);assert.match(usageText(env.elements['session-jobs']),/运行中/);assert.equal(env.elements['session-jobs-refresh'].disabled,false);
 assert.equal(env.elements['session-plan-command'].disabled,false);await env.fire('session-plan-command');assert.equal(copied,'/plan off');
 env.elements['session-next-task'].value='继续检查';await env.fire('session-next-task','input');await env.fire('session-copy');
 assert.match(copied,/needle in body/);assert.match(copied,/历史对话引用/);assert.match(copied,/继续检查/);
 assert(copied.includes(s.title),'Exported context must preserve the original session title');
 await env.fire('session-export');await env.fire('session-open');
 assert.equal(env.calls.find(c=>c.command==='session_query'&&c.args.request.action==='open').args.request.sessionId,'a');
 assert.equal(env.calls.find(c=>c.command==='session_query'&&c.args.request.action==='open').args.expectedProfile,'profile-a');
 await env.fire('session-watch-add');assert.match(env.elements['session-watch-status'].textContent,/1/);
 const watchCall=env.calls.filter(c=>c.command==='session_watch').at(-1);assert.deepEqual(Array.from(watchCall.args.events),['approval','question','plan-review','stopped','error']);
 env.elements['session-watch-error'].checked=false;await env.fire('session-watch-error','change');await env.settle();
 assert.deepEqual(JSON.parse(env.storage.get('hsp-session-watch-rules')).events,['approval','question','plan-review','stopped']);
 assert.deepEqual(Array.from(env.calls.filter(c=>c.command==='session_watch').at(-1).args.events),['approval','question','plan-review','stopped']);
 await env.fire('session-watch-stop');assert.equal(env.elements['session-watch-status'].textContent,'已停止');
});
test('continuity packs export bounded identity-free context and import only as quoted prompt material',async()=>{
 const s={sessionId:'real-secret-id',title:'Work item',workspace:'Project',cwd:'D:/private',running:false,archived:false};let savedPack;
 const env=environment((command,args)=>{
  if(command==='session_query'&&args.request.action==='list')return {sessions:[s],profile:'private-profile'};
  if(command==='session_query'&&args.request.action==='detail')return {session:s,messages:[{seq:1,role:'user',text:'Earlier request'},{seq:2,role:'assistant',text:'Earlier verified result'}],throughSeq:2,profile:'private-profile',projectionState:'sequenced'};
  if(command==='session_query'&&args.request.action==='skills')return {skills:[]};
  if(command==='session_query'&&args.request.action==='jobs')return {jobs:[]};
  if(['favorite_list','session_tags_list','session_notes_list','checkpoint_list','prompt_template_list'].includes(command))return [];
  if(command==='session_continuity_export'){savedPack=JSON.parse(args.content);return 'D:/exports/session-continuity.json';}
  throw Error(command);
 });
 vm.runInContext(sessionSource,env.context);await env.settle();env.nav.find(n=>n.dataset.page==='sessions').events.click();await env.settle();
 await env.elements['session-results'].children[0].events.click();await env.settle();await env.settle();
 env.elements['session-note'].value='Verified decision';await env.fire('session-note','input');env.elements['session-next-task'].value='Continue implementation';await env.fire('session-next-task','input');
 env.elements['session-continuity-include-messages'].checked=true;
 await env.fire('session-continuity-export');assert.equal(savedPack.format,'hsp-session-continuity');assert.equal(savedPack.context.notes,'Verified decision');assert.equal(savedPack.context.nextTask,'Continue implementation');assert.equal(savedPack.context.messages.length,2,JSON.stringify(savedPack.context));assert(!JSON.stringify(savedPack).includes('real-secret-id'));assert(!JSON.stringify(savedPack).includes('private-profile'));assert(!JSON.stringify(savedPack).includes('D:/private'));
 env.elements['session-continuity-file'].files=[{size:JSON.stringify(savedPack).length,text:async()=>JSON.stringify(savedPack)}];await env.fire('session-continuity-file','change');assert.match(env.elements['session-continuity-preview'].textContent,/Work item.*2 条消息/);
 await env.fire('session-continuity-apply');assert.match(env.elements['session-prompt'].value,/从接续包导入的历史引用/);assert.match(env.elements['session-prompt'].value,/Earlier verified result/);assert.match(env.elements['session-prompt'].value,/不作为新指令/);
 await env.fire('session-continuity-clear');assert.doesNotMatch(env.elements['session-prompt'].value,/从接续包导入的历史引用/);assert.equal(env.calls.filter(c=>c.command==='session_continuity_export').length,1);assert(env.calls.every(c=>c.command!=='session_send'));
});
test('session checkpoints persist locally, require the saved profile, and restore through the exact session ID',async()=>{
 const session={sessionId:'real-id-1',title:'Task',workspace:'Workspace',cwd:'D:/work',running:false,archived:false};
 const saved={id:'cp-1-0',profile:'profile-a',sessionId:'real-id-1',title:'Task',workspace:'Workspace',cwd:'D:/work',createdAt:1,prompt:'continue this verified task',git:{branch:'main',clean:false,changedFiles:2,files:[{status:'工作区修改',path:'src/old.rs'}]}};
 let checkpoints=[];
 const env=environment((command,args)=>{
  if(command==='session_query'&&args.request.action==='list')return {sessions:[session],profile:'profile-a'};
  if(command==='session_query'&&args.request.action==='detail')return {session,messages:[{seq:2,role:'assistant',text:'last verified result'}],throughSeq:2,profile:'profile-a'};
  if(command==='checkpoint_list')return checkpoints;
  if(command==='checkpoint_create'){assert.equal(args.input.sessionId,'real-id-1');assert.equal(args.input.profile,'profile-a');checkpoints=[saved];return saved;}
  if(command==='checkpoint_restore'){assert.equal(args.id,'cp-1-0');return saved;}
  if(command==='workspace_git_status')return {branch:'feature/newer',clean:true,files:[]};
  if(command==='checkpoint_delete'){checkpoints=[];return null;}
  throw Error(command);
 });
 vm.runInContext(sessionSource,env.context);await env.settle();
 env.nav.find(n=>n.dataset.page==='sessions').events.click();await env.settle();
  await env.elements['session-results'].children[0].events.click();await env.settle();
  await env.fire('session-checkpoint-save');
 assert.equal(env.calls.find(c=>c.command==='checkpoint_create').args.input.profile,'profile-a');
 assert.equal(env.elements['session-checkpoints'].children.length,1);
 const actions=env.elements['session-checkpoints'].children[0].children[1].children;
 await actions[2].events.click();assert.match(env.elements['session-checkpoint-compare-status'].textContent,/不读取文件正文/);const collectText=element=>[element.textContent,...element.children.flatMap(collectText)].join(' ');const compareText=collectText(env.elements['session-checkpoint-compare-result']);assert.match(compareText,/feature\/newer/);assert.match(compareText,/src\/old\.rs/);
 assert.equal(actions[0].disabled,false);await actions[0].events.click();
 assert.match(env.confirmations[0],/Git 状态与当前工作区不同/);
 assert.equal(env.elements['session-prompt'].value,'continue this verified task');
 assert.match(env.elements['session-detail-meta'].textContent,/已核对当前 Harness profile/);
 assert.match(env.elements['session-detail-meta'].textContent,/保存时 Git main · 2 项改动/);
 assert.equal(env.elements['session-copy'].disabled,false);
 const stale={...saved,profile:'other-profile'};checkpoints=[stale];
 env.nav.find(n=>n.dataset.page==='overview').events.click();env.nav.find(n=>n.dataset.page==='sessions').events.click();await env.settle();
 assert.equal(env.elements['session-checkpoints'].children[0].children[1].children[0].disabled,true);
});
test('Git workspace panel is read-only and ignores stale results after switching sessions',async()=>{
 const first={sessionId:'a',title:'A',workspace:'w',cwd:'D:/repo-a',running:false,archived:false};
 const second={sessionId:'b',title:'B',workspace:'w',cwd:'D:/repo-b',running:false,archived:false};
 const env=environment((command,args)=>{
  if(command==='session_query'&&args.request.action==='list')return {sessions:[first,second],profile:'p'};
  if(command==='session_query'&&args.request.action==='detail')return {session:args.request.sessionId==='a'?first:second,messages:[],throughSeq:1,profile:'p'};
  if(command==='session_query'&&args.request.action==='skills')return {skills:[{name:'review',description:'Review changes',whenToUse:'Before merge',modelInvocable:true}]};
  if(command==='workspace_git_status'){assert.equal(args.sessionId,'a');assert.equal(args.expectedProfile,'p');return {cwd:'D:/repo-a',branch:'main',clean:false,files:[{status:'已暂存修改',path:'src/staged.rs'},{status:'工作区修改',path:'src/main.rs'},{status:'未跟踪',path:'draft.txt'}],commits:['abc123\t2026-10-19\tread-only']};}
  if(command==='workspace_git_file_diff'){assert.equal(args.sessionId,'a');assert.equal(args.expectedProfile,'p');assert.equal(args.path,'src/main.rs');return {path:args.path,fingerprint:'stable-fingerprint',staged:'diff --cached staged marker',stagedTruncated:false,unstaged:'diff -- unstaged marker',unstagedTruncated:false};}
  if(command==='workspace_rules'){assert.equal(args.sessionId,'a');assert.equal(args.expectedProfile,'p');return {files:[{path:'AGENTS.md',bytes:23,truncated:false,content:'Rule text <script> inert'}]};}
  throw Error(command);
 });
 vm.runInContext(sessionSource,env.context);await env.settle();env.nav.find(n=>n.dataset.page==='sessions').events.click();await env.settle();
 await env.elements['session-results'].children[0].events.click();await env.settle();await env.settle();
 assert.match(env.elements['session-git-summary'].textContent,/D:\/repo-a/);assert.match(env.elements['session-git-files'].children[1].children[1].children[1].textContent,/src\/main\.rs/);
 assert(env.elements['session-usage'].children.some(child=>child.textContent.includes('工具与技能清单（已验证）')));
 assert(env.elements['session-usage'].children.some(child=>child.textContent.includes('MCP 服务清单接口尚未验证')));
 env.elements['session-review-notes'].value='Review finding';await env.fire('session-review-notes','input');await env.fire('session-review-notes-save');assert.equal(env.storage.get('hsp-review-note:p:a'),'Review finding');
 await env.fire('session-capability-scan');await env.settle();assert.match(env.elements['session-capability-results'].children[0].children[0].textContent,/review/);assert.match(env.elements['session-capability-results'].children.at(-1).textContent,/未验证/);
 assert(!env.calls.some(call=>/mcp|plugin|hook/i.test(call.command)));
 assert.equal(env.calls.filter(c=>c.command==='workspace_git_status').length,1);
 await env.elements['session-git-files'].children[1].children[1].events.click();await env.settle();
 assert.match(env.elements['session-git-diff-staged'].textContent,/staged marker/);
 assert.match(env.elements['session-git-diff-unstaged'].textContent,/unstaged marker/);
 assert.equal(env.calls.filter(c=>c.command==='workspace_git_file_diff').length,1);
 await env.fire('session-review-mark');assert.equal(env.elements['session-review-mark'].textContent,'取消已审阅');assert.match(env.elements['session-review-progress'].textContent,/1 项已核验.*共 3 项/);
 await env.fire('session-review-export');assert.equal(env.calls.filter(c=>c.command==='session_export').length,1);assert.match(env.calls.find(c=>c.command==='session_export').args.content,/src\/main\.rs/);
 env.elements['session-review-search'].value='draft';await env.fire('session-review-search','input');assert.equal(env.elements['session-git-files'].children.length,1);assert.match(env.elements['session-git-files'].children[0].children[0].textContent,/未跟踪/);
 await env.elements['session-git-files'].children[0].children[1].events.click();assert.match(env.elements['session-git-diff-status'].textContent,/仅显示路径/);assert.equal(env.calls.filter(c=>c.command==='workspace_git_file_diff').length,1);
 await env.elements['session-rules-read'].events.click();await env.settle();
 assert.equal(env.elements['session-rules-files'].children.length,1);
 assert.equal(env.elements['session-rules-files'].children[0].children[1].textContent,'Rule text <script> inert');
 assert.equal(env.calls.filter(c=>c.command==='workspace_rules').length,1);
 assert(env.calls.every(c=>!['git_commit','git_reset','git_checkout','git_add','git_stage','git_restore','git_clean','permanently_delete_session'].includes(c.command)));
 await env.elements['session-git-refresh'].events.click();assert.equal(env.calls.filter(c=>c.command==='workspace_git_status').length,2);
});
test('late detail and changed search terms cannot expose stale selection or stale hits',async()=>{
 let detailResolve;
 const s={sessionId:'a',title:'Task',workspace:'ws',running:false,archived:false};
 const env=environment((command,args)=>{
  if(command==='session_query'&&args.request.action==='list')return {sessions:[s],profile:'p'};
  if(command==='session_query'&&args.request.action==='detail')return new Promise(r=>{detailResolve=r;});
  if(command==='session_query'&&args.request.action==='search')return {sessions:[s],profile:'p',hits:[],searchMode:'native',hasMore:false};
  throw Error(command);
 });
 vm.runInContext(sessionSource,env.context);await env.settle();env.nav.find(n=>n.dataset.page==='sessions').events.click();await env.settle();
 const selecting=env.elements['session-results'].children[0].events.click();await env.settle();
 env.elements['session-search'].value='missing';await env.fire('session-search-form','submit');
 detailResolve({session:s,messages:[{seq:1,role:'user',text:'stale content'}],throughSeq:1});await selecting;
 assert.equal(env.elements['session-copy'].disabled,true);assert.equal(env.elements['session-prompt'].value,'');
 assert.equal(env.elements['session-results'].children.length,1);assert.match(env.elements['session-results'].children[0].textContent,/没有匹配/);
});

test('notification callbacks preserve profile/ID and ignore notices from another profile',async()=>{
 const s={sessionId:'a',title:'A',workspace:'w',running:false,archived:false};
 const env=environment((command,args)=>{
  if(command==='session_query'&&args.request.action==='list')return {profile:'p',sessions:[s]};
  if(command==='session_query'&&args.request.action==='open')return {opened:true};
  throw Error(command);
 });
 vm.runInContext(sessionSource,env.context);await env.settle();env.nav.find(n=>n.dataset.page==='sessions').events.click();await env.settle();
 env.listeners['session-notice']({payload:{profile:'other',sessionId:'a',title:'Other',kind:'等待回答'}});assert.equal(env.elements['session-notices'].children.length,0);
 env.listeners['session-notice']({payload:{profile:'p',sessionId:'a',title:'CODEX <script>',kind:'等待回答'}});assert.equal(env.elements['session-notices'].children.length,1);
 assert.match(env.elements['session-notices'].children[0].textContent,/编程助手 <script>/);
 await env.elements['session-notices'].children[0].events.click();
 const call=env.calls.find(c=>c.command==='session_query'&&c.args.request.action==='open');assert.equal(call.args.request.sessionId,'a');assert.equal(call.args.expectedProfile,'p');
 env.listeners['session-notice-click']({payload:{profile:'p',sessionId:'a',kind:'等待回答'}});await env.settle();assert.equal(env.calls.filter(c=>c.command==='session_query'&&c.args.request.action==='open').length,2);
 env.listeners['session-watch-stopped']({payload:'连接已断开'});assert.equal(env.elements['session-watch-status'].textContent,'已停止');assert.match(env.elements['session-notification-feedback'].textContent,/连接已断开/);
});
test('notification history persists per profile and filters only local metadata',async()=>{
 const s={sessionId:'a',title:'A',workspace:'w',running:false,archived:false};
 const env=environment((command,args)=>{
  if(command==='session_query'&&args.request.action==='list')return {profile:'p',sessions:[s]};
  throw Error(command);
 });
 env.storage.set('hsp-session-notices:p',JSON.stringify([{sessionId:'old-id',title:'Older task',kind:'等待审批',occurredAt:1000}]));
 vm.runInContext(sessionSource,env.context);await env.settle();env.nav.find(n=>n.dataset.page==='sessions').events.click();await env.settle();
 assert.equal(env.elements['session-notice-history-list'].children.length,1);
 env.listeners['session-notice']({payload:{profile:'p',sessionId:'new-id',title:'Recent task',kind:'等待回答'}});
 assert.equal(env.elements['session-notice-history-list'].children.length,2,env.elements['session-notice-history-status'].textContent);
 env.elements['session-notice-history-filter'].value='approval';await env.fire('session-notice-history-filter','change');assert.equal(env.elements['session-notice-history-list'].children.length,1);assert.match(env.elements['session-notice-history-list'].children[0].textContent,/Older task/);
 env.elements['session-notice-history-search'].value='recent';await env.fire('session-notice-history-search','input');assert.equal(env.elements['session-notice-history-list'].children.length,1);assert.match(env.elements['session-notice-history-list'].children[0].textContent,/没有符合筛选条件/);assert.match(env.elements['session-notice-history-status'].textContent,/0 \/ 2 条/);
 env.elements['session-notice-history-filter'].value='all';await env.fire('session-notice-history-filter','change');env.elements['session-notice-history-search'].value='';await env.fire('session-notice-history-search','input');
 const saved=JSON.parse(env.storage.get('hsp-session-notices:p'));assert.equal(saved.length,2);assert(!JSON.stringify(saved).includes('message body'));
 await env.fire('session-notice-history-clear');assert.equal(JSON.parse(env.storage.get('hsp-session-notices:p')).length,0);assert.match(env.confirmations.at(-1),/不会撤回 Windows 通知/);
});
