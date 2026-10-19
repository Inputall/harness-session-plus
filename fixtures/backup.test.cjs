const test=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const vm=require('node:vm');
const html=fs.readFileSync('desktop/ui/index.html','utf8');
const source=fs.readFileSync('desktop/ui/backup.js','utf8');
function setup(respond,confirm=true){
  const calls=[],elements={};
  for(const [,id] of html.matchAll(/\bid="(backup-[^"]+)"/g))elements[id]={disabled:false,checked:false,files:[],events:{},children:[],textContent:'',hidden:false,addEventListener(n,f){this.events[n]=f;},replaceChildren(){this.children=[];},append(n){this.children.push(n);}};
  const invoke=async(command,args)=>{calls.push({command,args});return command==='backup_status'?{pending:false,message:''}:respond(command,args);};
  class Reader {readAsDataURL(file){this.result='data:application/zip;base64,'+file.encoded;this.onload();}}
  vm.runInNewContext(source,{invoke,FileReader:Reader,Date,window:{confirm:()=>confirm},document:{getElementById:id=>elements[id],createElement:()=>({textContent:''})}});
  return {calls,el:id=>elements['backup-'+id],fire:async(id,event='click')=>elements['backup-'+id].events[event](),settle:()=>new Promise(resolve=>setImmediate(resolve))};
}
const preview={version:'0.8.0',createdAt:1,sha256:'digest',files:[{path:'settings.json',size:20,change:'覆盖'}],warning:'覆盖清单内文件',records:'保留当前记录'};
test('restoring requires successful preview, acknowledgment and explicit confirmation',async()=>{
 const env=setup((command,args)=>{if(command==='backup_preview')return preview;if(command==='backup_stage'){assert.equal(args.expectedSha256,'digest');assert.equal(args.archiveBase64,'zip');return {message:'已安排'};}});
 await env.settle();assert(env.el('stage').disabled);
 env.el('file').files=[{size:10,encoded:'zip'}];await env.fire('preview');
 assert(env.el('stage').disabled);assert.equal(env.el('files').children.length,1);
 env.el('ack').checked=true;await env.fire('ack','change');assert(!env.el('stage').disabled);
 await env.fire('stage');assert.equal(env.calls.filter(c=>c.command==='backup_stage').length,1);
 assert(env.el('stage').disabled);assert(!env.el('cancel').disabled);
});
test('file changes invalidate prior confirmation and oversized files never reach host',async()=>{
 const env=setup(()=>preview);await env.settle();env.el('file').files=[{size:10,encoded:'zip'}];await env.fire('preview');env.el('ack').checked=true;
 await env.fire('file','change');assert(env.el('stage').disabled);assert(!env.el('ack').checked);
 env.el('file').files=[{size:65*1024*1024}];await env.fire('preview');
 assert.match(env.el('result').textContent,/64 MiB/);assert.equal(env.calls.filter(c=>c.command==='backup_preview').length,1);
});
test('rejected confirmation does not schedule restore; backup options are opt-in',async()=>{
 const env=setup(command=>command==='backup_preview'?preview:{path:'local.zip',sha256:'digest'},false);await env.settle();
 await env.fire('create');const args=env.calls.find(c=>c.command==='backup_create').args;assert(!args.includeRecords);assert(!args.includePaths);
 env.el('file').files=[{size:10,encoded:'zip'}];await env.fire('preview');env.el('ack').checked=true;await env.fire('stage');
 assert(!env.calls.some(c=>c.command==='backup_stage'));
});
test('host errors keep restore disabled and report plain text',async()=>{
 const env=setup(()=>{throw {message:'<invalid archive>'};});await env.settle();env.el('file').files=[{size:10,encoded:'zip'}];await env.fire('preview');
 assert.equal(env.el('result').textContent,'<invalid archive>');assert(env.el('stage').disabled);assert(!env.el('preview').disabled);
});
