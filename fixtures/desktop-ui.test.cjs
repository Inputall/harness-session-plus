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
  const nav = ['overview','trash','connection','capabilities','settings'].map(page=>{const el=new Element();el.dataset.page=page;return el;});
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
  env.nav[1].events.click();assert.equal(env.elements['page-trash'].hidden,false);assert.equal(env.elements['page-overview'].hidden,true);
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
  const records=[{sessionId:'id-a',title:'<img src=x onerror=bad()>',workspaceTitle:'workspace',movedAt:1,state:'archived'},{sessionId:'id-b',title:'same',workspaceTitle:'workspace',movedAt:2,state:'unknown'}];
  const env=environment((command,args)=>{if(command==='refresh_trash')return restored?[]:records;if(command==='restore_sessions'){assert.deepEqual(Array.from(args.sessionIds),['id-a']);restored=true;return {outcomes:[{ok:true,sessionId:'id-a'}]};}throw Error('Unexpected command');});
  await env.settle();await env.fire('refresh-trash');
  const rows=env.elements['trash-list'].children;
  assert.equal(rows.length,2);assert.equal(rows[0].children[1].children[0].textContent,records[0].title);
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
