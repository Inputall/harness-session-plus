(() => {
  'use strict';
  const key='__HARNESS_SESSION_PLUS__';
  if(['0.1.0','0.1.1','0.2.0'].includes(window[key]?.version))window[key].uninstall();
  if(window[key])throw Error('Unknown enhancement');
  let installed=true,observer,timer,heartbeat=Date.now(),sidebar;
  const buttons=new Map(),queue=[],pending=new Map();
  function binding(native){
    const fiberKey=Object.keys(native).find(k=>k.startsWith('__reactFiber$'));
    let fiber=native[fiberKey];const ids=new Set();
    for(let n=0;fiber&&n<60;n++,fiber=fiber.return){const id=fiber.memoizedProps?.sessionId;if(typeof id==='string'&&id.length>0)ids.add(id);}
    if(ids.size!==1)return null;
    const id=[...ids][0];
    const rows=[...document.querySelectorAll('[data-row-key]')].filter(r=>r.getAttribute('data-row-key')===`session:${id}`);
    if(rows.length!==1)return null;
    return {id,title:rows[0].querySelector('[class*="_title"]')?.textContent||'此会话'};
  }
  const available=()=>installed&&Date.now()-heartbeat<5000;
  function send(action,sessionId,button){
    if(!available()){window.alert('管理工具已断开。请在 Harness Plus 中重新安装回收站增强。');return;}
    if(queue.length+pending.size>=16)return;
    const requestId=crypto.randomUUID();
    queue.push({action,sessionId,requestId});pending.set(requestId,button);if(button)button.disabled=true;
  }
  function scan(){
    timer=undefined;if(!installed)return;
    for(const [button,handler]of buttons)if(!button.isConnected){button.removeEventListener('click',handler);buttons.delete(button);}
    for(const menu of document.querySelectorAll('[role="menu"]')){
      if(menu.querySelector('[data-hsp-recycle]'))continue;
      const native=[...menu.querySelectorAll('[role="menuitem"],button')].find(el=>el.textContent?.trim()==='归档会话'||[...el.querySelectorAll('span')].some(s=>s.textContent?.trim()==='归档会话'));
      if(!native)continue;
      const button=document.createElement('button');button.type='button';button.setAttribute('role','menuitem');button.dataset.hspRecycle='true';button.className='hsp-recycle-action';
      const bound=binding(native);button.textContent='移入回收站';button.disabled=!bound;button.title=bound?'通过原生归档保留数据，可在 Harness Plus 恢复':'无法确认此菜单对应的会话 ID';
      const handler=e=>{e.preventDefault();e.stopPropagation();const current=binding(native);if(!bound||!current||current.id!==bound.id){window.alert('菜单对应的会话已改变，请重新打开菜单。');return;}if(!window.confirm(`将“${current.title}”移入回收站？\n会话数据将保留，可以在 Harness Plus 中恢复。运行中的会话不会被停止。`))return;if(binding(native)?.id!==current.id)return;send('archive',current.id,button);};
      button.addEventListener('click',handler);buttons.set(button,handler);(native.parentElement||menu).append(button);
    }
    if(!sidebar?.isConnected){
      const row=document.querySelector('[data-row-key^="workspace:"]');
      if(row?.parentElement){sidebar=document.createElement('button');sidebar.type='button';sidebar.dataset.hspRecycle='true';sidebar.className='hsp-recycle-action hsp-recycle-entry';sidebar.textContent='♧ 会话回收站';const handler=()=>send('open',null,sidebar);sidebar.addEventListener('click',handler);buttons.set(sidebar,handler);row.parentElement.prepend(sidebar);}
    }
  }
  const style=document.createElement('style');style.dataset.hspRecycle='true';style.textContent='.hsp-recycle-action{display:flex;align-items:center;width:100%;padding:8px 12px;border:0;border-radius:6px;background:transparent;color:inherit;font:inherit;text-align:left;cursor:pointer}.hsp-recycle-action:hover{background:color-mix(in srgb,currentColor 9%,transparent)}.hsp-recycle-action:disabled{opacity:.45;cursor:default}.hsp-recycle-entry{margin-bottom:8px;color:#7279ff}';document.documentElement.append(style);
  const api={version:'0.2.0',binding,
    drain(){heartbeat=Date.now();return queue.splice(0,16);},
    complete(requestId,error){const button=pending.get(requestId);pending.delete(requestId);if(button?.isConnected)button.disabled=false;if(error)window.alert(error);},
    getStatus(){return {installed,version:api.version,testButtons:0,recycleButtons:[...buttons.keys()].filter(b=>b.isConnected).length,dataOperations:available()};},
    uninstall(){installed=false;observer.disconnect();clearTimeout(timer);queue.length=0;pending.clear();for(const [b,h]of buttons){b.removeEventListener('click',h);b.remove();}buttons.clear();style.remove();delete window[key];return {installed:false,dataOperations:false};}
  };
  Object.defineProperty(window,key,{value:api,configurable:true});
  observer=new MutationObserver(records=>{if(records.some(r=>!r.target.closest?.('[data-hsp-recycle]'))&&timer===undefined)timer=setTimeout(scan,80);});observer.observe(document.documentElement,{childList:true,subtree:true});scan();return api.getStatus();
})()
