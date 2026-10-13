const test=require('node:test'),assert=require('node:assert/strict'),vm=require('node:vm'),fs=require('node:fs');
const source=fs.readFileSync('injected/recycle-menu.js','utf8');
function environment(ids=['id-a']){
 const nodes=[];let observer,clock=0;
 class El{constructor(tag){this.tag=tag;this.dataset={};this.listeners={};this.isConnected=true;this.children=[];}append(x){x.parentElement=this;this.children.push(x);nodes.push(x);}prepend(x){this.append(x);}setAttribute(){}addEventListener(k,h){this.listeners[k]=h;}removeEventListener(k){delete this.listeners[k];}remove(){this.isConnected=false;}querySelectorAll(s){return s==='span'?[{textContent:'归档会话'}]:[native];}querySelector(s){return s==='[data-hsp-recycle]'?this.children.find(x=>x.dataset.hspRecycle):null;}}
 const menu=new El('menu'),native=new El('button');native.parentElement=menu;native.__reactFiber$test=ids.reduce((parent,sessionId)=>({memoizedProps:{sessionId},return:parent}),null);
 const row=new El('row');row.getAttribute=()=>`session:${ids[0]}`;row.querySelector=()=>({textContent:'same title'});
 const document={documentElement:new El('html'),createElement:t=>new El(t),querySelector:()=>null,querySelectorAll:s=>s==='[data-row-key]'?[row]:[menu]};
 const window={confirm:()=>true,alert:()=>{},setTimeout(){},clearTimeout(){}};
 const context=vm.createContext({document,window,crypto:{randomUUID:()=>`r-${++clock}`},Date:{now:()=>clock},MutationObserver:class{constructor(cb){observer=this;this.cb=cb;}observe(){}disconnect(){}},setTimeout:()=>1,clearTimeout:()=>{}});
 vm.runInContext(source,context);return {api:window.__HARNESS_SESSION_PLUS__,button:nodes.find(x=>x.dataset.hspRecycle&&x.tag==='button'),native,window,advance:()=>clock=10000,click(){this.button.listeners.click({preventDefault(){},stopPropagation(){}});}};
}
test('menu action emits the bound ID and waits for Rust acknowledgement',()=>{const e=environment();e.click();assert.equal(e.button.disabled,true);const requests=e.api.drain();assert.equal(requests.length,1);assert.equal(requests[0].sessionId,'id-a');assert.equal(requests[0].action,'archive');e.api.complete(requests[0].requestId,null);assert.equal(e.button.disabled,false);});
test('ambiguous IDs and changed menu binding cannot enqueue an operation',()=>{const e=environment(['a','b']);assert.equal(e.button.disabled,true);e.click();assert.equal(e.api.drain().length,0);const other=environment();other.native.__reactFiber$test.memoizedProps.sessionId='other';other.click();assert.equal(other.api.drain().length,0);});
test('expired management heartbeat and rejected confirmation cannot enqueue',()=>{const e=environment();e.advance();e.click();assert.equal(e.api.drain().length,0);const other=environment();other.window.confirm=()=>false;other.click();assert.equal(other.api.drain().length,0);});
test('uninstall removes handler and disables all queued work',()=>{const e=environment();e.click();e.api.uninstall();assert.equal(e.button.isConnected,false);assert.equal(e.api.drain().length,0);assert.equal(e.api.getStatus().dataOperations,false);});
