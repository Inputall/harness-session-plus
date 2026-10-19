"use strict";
(() => {
  const el = id => document.getElementById(`backup-${id}`);
  let busy=false, generation=0, preview=null, pending=false;
  const ids=['create','diagnostics','folder','preview','stage','cancel'];
  function controls(){
    for(const id of ids) el(id).disabled=busy || (id==='stage'&&(!preview||!el('ack').checked||pending)) || (id==='cancel'&&!pending);
    el('file').disabled=busy;el('ack').disabled=busy;
  }
  function clear(){generation++;preview=null;el('ack').checked=false;el('details').hidden=true;controls();}
  function output(value){el('result').textContent=value;}
  async function status(){const s=await invoke('backup_status');pending=s.pending;if(s.message)output(s.message);if(pending)output('已有待恢复任务。请从托盘完全退出后重新打开工具，或取消待恢复任务。');controls();}
  async function run(action){if(busy)return;busy=true;controls();try{await action();}catch(error){output(error?.message||String(error));}finally{busy=false;controls();}}
  function read(file){return new Promise((resolve,reject)=>{const reader=new FileReader();reader.onerror=()=>reject(new Error('读取备份失败'));reader.onload=()=>resolve(String(reader.result).split(',')[1]);reader.readAsDataURL(file);});}
  el('file').addEventListener('change',clear);
  el('ack').addEventListener('change',controls);
  el('create').addEventListener('click',()=>run(async()=>{
    const result=await invoke('backup_create',{includeRecords:el('records').checked,includePaths:el('paths').checked});
    output(`备份已保存：${result.path}\nSHA256：${result.sha256}`);
  }));
  el('diagnostics').addEventListener('click',()=>run(async()=>{
    const result=await invoke('backup_diagnostics');output(`脱敏诊断已保存：${result.path}\n仅含工具状态摘要，不含正文、标题、路径、profile、令牌或原始日志。`);
  }));
  el('folder').addEventListener('click',()=>run(()=>invoke('backup_open_folder')));
  el('preview').addEventListener('click',()=>run(async()=>{
    clear();const token=generation,file=el('file').files?.[0];
    if(!file)throw new Error('请先选择备份 ZIP');
    if(file.size>64*1024*1024)throw new Error('备份超过 64 MiB');
    output('正在读取并校验备份…');const encoded=await read(file);
    const result=await invoke('backup_preview',{archiveBase64:encoded});
    if(token!==generation)return;
    preview={encoded,hash:result.sha256};el('details').hidden=false;
    el('summary').textContent=`版本 ${result.version} · ${new Date(result.createdAt).toLocaleString()} · ${result.files.length} 个文件 · SHA256 ${result.sha256}`;
    el('files').replaceChildren();
    for(const item of result.files){const li=document.createElement('li');li.textContent=`${item.change}：${item.path}（${item.size} 字节）`;el('files').append(li);}
    el('warning').textContent=`${result.warning} ${result.records} 额外操作：关闭本工具的自动回收站增强偏好。未列出的文件保留。`;
    output('备份校验通过。请核对文件清单，再勾选确认。校验码用于检查完整性，不证明备份来源可信。');
  }));
  el('stage').addEventListener('click',()=>run(async()=>{
    if(!preview||!el('ack').checked||pending)return;
    if(!window.confirm('将覆盖清单内的工具数据。下次启动前会自动备份，失败将回滚。是否安排恢复？'))return;
    const result=await invoke('backup_stage',{archiveBase64:preview.encoded,expectedSha256:preview.hash});pending=true;clear();output(result.message);
  }));
  el('cancel').addEventListener('click',()=>run(async()=>{await invoke('backup_cancel');pending=false;output('待恢复任务已取消，当前工具数据未变更。');}));
  status().catch(error=>output(error?.message||String(error)));
})();
