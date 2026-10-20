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
  const calls=[];const intervals=new Map();let nextTimer=1;
  const settings={port:9223,target_id:null,executable:'C:\\Harness.exe',theme:'dark'};
  const document={getElementById:id=>elements[id],createElement:()=>new Element(),documentElement:{dataset:{}},querySelector:()=>null,querySelectorAll:selector=>({'.page':pages,'.nav-item':nav,'[data-page]':nav,'[data-nav]':navShortcuts,'[data-action]':actions}[selector]||[])};
  const invoke=async(command,args)=>{calls.push({command,args});if(command==='load_settings')return settings;if(command==='save_settings')return args.settings;return responder(command,args);};
  const listeners={};
  const context=vm.createContext({document,window:{confirm:()=>true,__TAURI__:{core:{invoke},event:{listen:async(name,fn)=>{listeners[name]=fn;}}}},Option:class extends Element {constructor(text,value){super();this.textContent=text;this.value=value;}},setTimeout:()=>nextTimer++,clearTimeout:()=>{},setInterval:fn=>{const id=nextTimer++;intervals.set(id,fn);return id;},clearInterval:id=>intervals.delete(id),Date,console});
  vm.runInContext(source,context);
  return {elements,nav,actions,calls,intervals,context,listeners,fire:async(id,event='click')=>{await elements[id].events[event]({preventDefault(){}});},settle:()=>new Promise(resolve=>setImmediate(resolve))};
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
   if(args.request.action==='detail')return {session:s,messages:[{seq:1,role:'user',text:'needle in body'}],throughSeq:1,hasMore:true,profile:'profile-a',projectionState:'sequenced',stats:{permissionPreset:'workspace-write',planMode:{active:true,pending:false},tokenUsage:{uncachedInputTokens:125,outputTokens:30},contextPressure:{pressureTokens:160},modelSelection:{lastUsed:{provider:'provider-x',model:'model-y'},next:null}}};
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
 assert.match(env.elements['session-count'].textContent,/索引关闭/);
 await env.elements['session-results'].children[0].events.click();await env.settle();
 await env.settle();
 assert.equal(env.elements['session-detail-title'].textContent,'编程助手 <img onerror=x>');
 const usageText=n=>[n.textContent,...n.children.map(usageText)].join(' ');assert.match(usageText(env.elements['session-usage']),/workspace-write/);assert.match(usageText(env.elements['session-usage']),/计划模式/);assert.match(usageText(env.elements['session-usage']),/125/);assert.match(usageText(env.elements['session-usage']),/Read-only review/);
 assert.match(usageText(env.elements['session-jobs']),/npm test/);assert.match(usageText(env.elements['session-jobs']),/运行中/);assert.equal(env.elements['session-jobs-refresh'].disabled,false);
 assert.equal(env.elements['session-plan-command'].disabled,false);await env.fire('session-plan-command');assert.equal(copied,'/plan off');
 env.elements['session-next-task'].value='继续检查';await env.fire('session-next-task','input');await env.fire('session-copy');
 assert.match(copied,/needle in body/);assert.match(copied,/历史对话引用/);assert.match(copied,/继续检查/);
 assert(copied.includes(s.title),'Exported context must preserve the original session title');
 await env.fire('session-export');await env.fire('session-open');
 assert.equal(env.calls.find(c=>c.command==='session_query'&&c.args.request.action==='open').args.request.sessionId,'a');
 assert.equal(env.calls.find(c=>c.command==='session_query'&&c.args.request.action==='open').args.expectedProfile,'profile-a');
 await env.fire('session-watch-add');assert.match(env.elements['session-watch-status'].textContent,/1/);await env.fire('session-watch-stop');assert.equal(env.elements['session-watch-status'].textContent,'已停止');
});
test('session checkpoints persist locally, require the saved profile, and restore through the exact session ID',async()=>{
 const session={sessionId:'real-id-1',title:'Task',workspace:'Workspace',cwd:'D:/work',running:false,archived:false};
 const saved={id:'cp-1-0',profile:'profile-a',sessionId:'real-id-1',title:'Task',workspace:'Workspace',cwd:'D:/work',createdAt:1,prompt:'continue this verified task'};
 let checkpoints=[];
 const env=environment((command,args)=>{
  if(command==='session_query'&&args.request.action==='list')return {sessions:[session],profile:'profile-a'};
  if(command==='session_query'&&args.request.action==='detail')return {session,messages:[{seq:2,role:'assistant',text:'last verified result'}],throughSeq:2,profile:'profile-a'};
  if(command==='checkpoint_list')return checkpoints;
  if(command==='checkpoint_create'){assert.equal(args.input.sessionId,'real-id-1');assert.equal(args.input.profile,'profile-a');checkpoints=[saved];return saved;}
  if(command==='checkpoint_restore'){assert.equal(args.id,'cp-1-0');return saved;}
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
 assert.equal(actions[0].disabled,false);await actions[0].events.click();
 assert.equal(env.elements['session-prompt'].value,'continue this verified task');
 assert.match(env.elements['session-detail-meta'].textContent,/已核对当前 Harness profile/);
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
  if(command==='workspace_git_status'){assert.equal(args.sessionId,'a');assert.equal(args.expectedProfile,'p');return {cwd:'D:/repo-a',branch:'main',clean:false,files:[{status:'工作区修改',path:'src/main.rs'}],commits:['abc123\t2026-10-19\tread-only']};}
  throw Error(command);
 });
 vm.runInContext(sessionSource,env.context);await env.settle();env.nav.find(n=>n.dataset.page==='sessions').events.click();await env.settle();
 await env.elements['session-results'].children[0].events.click();await env.settle();await env.settle();
 assert.match(env.elements['session-git-summary'].textContent,/D:\/repo-a/);assert.match(env.elements['session-git-files'].children[0].children[1].textContent,/src\/main\.rs/);
 assert.equal(env.calls.filter(c=>c.command==='workspace_git_status').length,1);
 assert(env.calls.every(c=>!['git_commit','git_reset','git_checkout','git_add'].includes(c.command)));
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
