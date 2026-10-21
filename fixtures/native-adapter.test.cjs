const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm');
const source=fs.readFileSync('injected/native-adapter.js','utf8');
function adapter(origin='http://127.0.0.1:9223'){
  const sent=[],sockets=[];class WS{constructor(url){this.url=url;this.readyState=0;sockets.push(this);queueMicrotask(()=>{this.readyState=1;this.onopen?.();});}send(value){sent.push(JSON.parse(value));}close(){this.readyState=3;}}
  const context=vm.createContext({window:{dshDesktopBoot:{ready:async()=>({streamBaseUrl:origin})}},WebSocket:WS,crypto:{randomUUID:()=>`stream-${sockets.length}`},URL,AbortSignal,setTimeout,clearTimeout});
  return {adapter:vm.runInContext(source,context),sent,sockets};
}
test('job roster stream opens against loopback, returns only the first complete rows frame, then cancels',async()=>{
  const e=adapter(),pending=e.adapter.streamFirst('job/list',{request:{sessionId:'s-1'}},'http://127.0.0.1:9223');await new Promise(resolve=>setImmediate(resolve));
  const socket=e.sockets[0],open=e.sent[0];assert.equal(socket.url,'ws://127.0.0.1:9223/api/remote.mux');assert.equal(open.endpoint,'job/list');assert.deepEqual(JSON.parse(JSON.stringify(open.payload.args)),{request:{sessionId:'s-1'}});
  socket.onmessage({data:JSON.stringify({type:'item',streamId:open.streamId,value:{type:'rows',jobs:[{id:'j'}]}})});
  assert.deepEqual(JSON.parse(JSON.stringify(await pending)),[{id:'j'}]);assert.deepEqual(e.sent.at(-1),{type:'cancel',streamId:open.streamId});assert.equal(socket.readyState,3);
});
test('stream helper rejects non-loopback and changed Host generations',async()=>{
  await assert.rejects(()=>adapter('http://localhost:9223').adapter.streamFirst('job/list',{},'http://localhost:9223'),/Host origin mismatch/);
  let count=0;const e=adapter();e.adapter.streamFirst('job/list',{},'http://127.0.0.1:9224').catch(()=>{});await new Promise(resolve=>setImmediate(resolve));assert.equal(e.sockets.length,0);
  assert.equal(count,0);
});
test('stream errors do not return partial or fabricated roster values',async()=>{
  const e=adapter(),pending=e.adapter.streamFirst('job/list',{},'http://127.0.0.1:9223');await new Promise(resolve=>setImmediate(resolve));const s=e.sockets[0];
  s.onmessage({data:JSON.stringify({type:'error',streamId:e.sent[0].streamId,error:{code:'job/unavailable'}})});await assert.rejects(pending,/job\/unavailable/);assert.equal(e.sent.at(-1).type,'cancel');
});
