"use strict";
const $ = id => document.getElementById(id);
const invoke = (command, args = {}) => {
  if (!window.__TAURI__?.core?.invoke) return Promise.reject({message:"请通过桌面程序打开此界面。"});
  return window.__TAURI__.core.invoke(command, args);
};
const state = {settings:null, busy:false, connected:false, installed:null, autoTimer:null, failures:0, toastTimer:null, records:[], selected:new Set(), recyclePreference:false, restoringRecycle:false};
const titles = {overview:"概览",sessions:"会话中心",trash:"会话回收站",connection:"连接与增强",capabilities:"能力探测",skins:"皮肤中心",settings:"设置"};
const actionNames = {diagnose:"连接检查",inject_test:"测试注入",remove_injection:"卸载增强",probe_capabilities:"只读能力探测"};
function navigate(page) {
  for (const section of document.querySelectorAll(".page")) section.hidden = section.id !== `page-${page}`;
  for (const button of document.querySelectorAll(".nav-item")) {
    const active = button.dataset.page === page;
    button.classList.toggle("active", active);
    if (active) button.setAttribute("aria-current","page"); else button.removeAttribute("aria-current");
  }
  $("crumb").textContent = titles[page];
  if(page==='skins')loadSkins();
  if(page==='sessions')window.sessionCenterOpen?.();
}
const skinState={catalog:[],selected:null,loading:false,running:false,page:0,communityOffset:0,communityTotal:0,communityItems:[],communityLoading:false};
// Keep upstream identifiers and attribution files intact; use neutral UI labels.
const themeText=value=>String(value??'').replace(/codex(?:\+\+|plusplus)?[\s-]*/gi,'').trim();
// Presentation aliases only: never use these labels as session IDs or API arguments.
const sessionLabel=value=>String(value??'').replace(/codex(?:\+\+|plusplus)?/gi,'编程助手');
const themeDisplay=theme=>({...theme,name:themeText(theme.name)||'未命名主题',description:themeText(theme.description),author:themeText(theme.author),authorDisplayName:themeText(theme.authorDisplayName),license:themeText(theme.license),conversionReason:themeText(theme.conversionReason),tags:(theme.tags||[]).map(themeText)});
async function loadSkins(){
  if(skinState.loading||skinState.catalog.length)return;skinState.loading=true;
  try{
    const catalog=await invoke('theme_library');skinState.catalog=catalog.map(themeDisplay);renderSkins();
  }catch(e){$("skin-feedback").textContent=e.message||'无法加载皮肤，请重新打开页面。';}finally{skinState.loading=false;}
}
function renderSkins(){
    const query=$("skin-search").value.trim().toLowerCase();
    const category=$("skin-filter").value;
    const catalog=skinState.catalog.filter(s=>(!category||category==='all'||s.category===category||(category==='imported'&&(s.id.startsWith('imported-')||s.id==='backup-appearance')))&&`${s.name} ${s.author||''} ${s.description||''} ${(s.tags||[]).join(' ')}`.toLowerCase().includes(query));
    const pages=Math.max(1,Math.ceil(catalog.length/12));skinState.page=Math.min(skinState.page,pages-1);
    $("skin-count").textContent=`本地 ${catalog.length} 款 · 第 ${skinState.page+1} / ${pages} 页`;
    $("skin-prev").disabled=skinState.page===0;$("skin-next").disabled=skinState.page>=pages-1;
    $("skin-catalog").replaceChildren();
    for(const skin of catalog.slice(skinState.page*12,skinState.page*12+12)){
      const card=document.createElement('button');card.type='button';card.className='skin-card';card.setAttribute('aria-pressed',String(skinState.selected===skin.id));
      for(const key of ['bg','surface','hover','text','muted','accent','border'])card.style?.setProperty(`--skin-${key}`,skin[key]);
      const preview=document.createElement('div');preview.className='skin-preview';const side=document.createElement('div');side.className='skin-preview-side';side.textContent='H+';
      const content=document.createElement('div');content.className='skin-preview-content';const label=document.createElement('strong');label.textContent='你的工作区';const line=document.createElement('span');line.textContent='让专注更舒服';const bubble=document.createElement('div');bubble.className='skin-preview-bubble';bubble.textContent='开始新的思考';content.append(label,line,bubble);preview.append(side,content);
      const name=document.createElement('strong');name.textContent=skin.name;const description=document.createElement('small');description.textContent=skin.description;card.append(preview,name,description);
      if(skin.preview||skin.wallpaper){const img=document.createElement('img');img.className='skin-wallpaper-preview';img.loading='lazy';img.src=skin.preview||skin.wallpaper;img.alt=`${skin.name} 壁纸预览`;card.prepend(img);}
      if(skin.converted){const credit=document.createElement('small');credit.textContent=`作者：${skin.author||'未提供'} · ${skin.license||'未提供许可'} · 转换主题`;card.append(credit);}
      card.addEventListener('click',()=>{if(state.busy)return;skinState.selected=skin.id;for(const sibling of $("skin-catalog").children)sibling.setAttribute('aria-pressed',String(sibling===card));$("skin-selection").textContent=skin.name;$("skin-description").textContent=skin.description;$("skin-source-info").textContent=skin.source?'作者、素材来源与原许可详见随附主题文件。':'';$("skin-apply").disabled=false;});
      $("skin-catalog").append(card);
    }
}
$("skin-search").addEventListener('input',()=>{skinState.page=0;renderSkins();});
$("skin-filter").addEventListener('change',()=>{skinState.page=0;renderSkins();});
$("skin-prev").addEventListener('click',()=>{if(skinState.page>0){skinState.page--;renderSkins();}});
$("skin-next").addEventListener('click',()=>{skinState.page++;renderSkins();});
async function loadCommunity(offset=0){
  if(skinState.communityLoading)return;skinState.communityLoading=true;$("community-refresh").disabled=true;$("community-prev").disabled=true;$("community-next").disabled=true;
  $("community-info").textContent='正在从 DreamSkin 社区加载…';
  try{
    const result=await invoke('community_page',{offset});result.items=result.items.map(themeDisplay);skinState.communityOffset=offset;skinState.communityTotal=result.total;skinState.communityItems=result.items;
    $("community-list").replaceChildren();
    for(const item of result.items){
      const card=document.createElement('article');card.className='panel community-card';const name=document.createElement('strong');name.textContent=item.name;
      const credit=document.createElement('small');credit.textContent=`${item.authorDisplayName||'未知作者'} · ${item.license||'许可未提供'}`;
      const note=document.createElement('p');note.textContent=item.conversionReason||'安装时核验主题包和素材摘要，仅转换配色与壁纸。';
      const button=document.createElement('button');button.className='button';button.textContent='安装到我的主题';button.disabled=!!item.conversionReason;
      button.addEventListener('click',()=>installCommunity(item,button));card.append(name,credit,note,button);$("community-list").append(card);
    }
    $("community-info").textContent=`社区共 ${result.total} 个条目 · 当前 ${offset+1}–${offset+result.items.length} · 安装后在本地主题中应用`;
  }catch(e){$("community-info").textContent=e.message||'社区加载失败，本地主题仍可使用。';}
  finally{skinState.communityLoading=false;$("community-refresh").disabled=false;$("community-prev").disabled=skinState.communityOffset===0;$("community-next").disabled=skinState.communityOffset+skinState.communityItems.length>=skinState.communityTotal;}
}
async function installCommunity(item,button){
  if(state.busy)return;
  if(!window.confirm(`安装“${item.name}”？\n作者：${item.authorDisplayName||'未知'}\n许可：${item.license||'未提供'}\n请确认素材许可适合你的用途。安装只保存到本机，不会自动应用。`))return;
  busy(true);button.disabled=true;
  try{await invoke('community_install',{versionId:item.id});skinState.catalog=[];skinState.page=0;$("skin-filter").value='all';$("skin-search").value='';await loadSkins();button.textContent='已安装';toast('已安装到本地主题，选择卡片后应用。');}
  catch(e){toast(e.message||'主题无法转换，请查看许可或格式。',true);button.disabled=false;}
  finally{busy(false);}
}
$("community-refresh").addEventListener('click',()=>loadCommunity(0));
$("community-prev").addEventListener('click',()=>loadCommunity(Math.max(0,skinState.communityOffset-48)));
$("community-next").addEventListener('click',()=>loadCommunity(skinState.communityOffset+48));
async function skinAction(action){
  if(state.busy||skinState.running||(action==='apply'&&!skinState.selected))return;
  skinState.running=true;busy(true);
  for(const id of ['skin-apply','skin-reset','skin-status'])$(id).disabled=true;
  try{
    const result=await invoke('skin_action',{action,skinId:action==='apply'?skinState.selected:null,wallpaperClarity:Number($("skin-clarity").value)});
    const selected=skinState.catalog.find(s=>s.id===result.active);
    $("skin-current").textContent=result.installed?(selected?.name||themeText(result.active)||'已应用主题'):'Harness 默认';
    $("skin-feedback").textContent=action==='reset'?'已停止主题自动恢复，并移除配色和壁纸覆盖。':action==='apply'?`已应用到 Harness${result.wallpaper?'，含壁纸':''}。主题已保存，后台主题守护将自动恢复；关闭工具仍保留。${selected?.converted?' 主题仅应用配色和壁纸。':''}`:result.supported?'当前页面具备已识别的主题变量。':'当前页面缺少主题变量，不能应用皮肤。';
    if(action!=='status'){log(action==='apply'?`已应用 Harness 皮肤：${selected?.name||result.active}`:'Harness 皮肤已恢复默认');toast($("skin-feedback").textContent);}
  }catch(e){$("skin-current").textContent='状态未确认';$("skin-feedback").textContent=e.message||'请检查 Harness 连接后重试。';toast($("skin-feedback").textContent,true);if(e.target_ids?.length){fillTargets(e.target_ids.map(id=>({id})));navigate('settings');}}
  finally{skinState.running=false;busy(false);$("skin-apply").disabled=!skinState.selected;$("skin-reset").disabled=false;$("skin-status").disabled=false;}
}
$("skin-apply").addEventListener('click',()=>skinAction('apply'));
$("skin-reset").addEventListener('click',()=>skinAction('reset'));
$("skin-status").addEventListener('click',()=>skinAction('status'));
$("skin-clarity").addEventListener('input',()=>{$("skin-clarity-value").textContent=`${$("skin-clarity").value}%`;});
$("theme-import-form").addEventListener('submit',async event=>{
  event.preventDefault();if(state.busy)return;
  const file=$("theme-config-file").files?.[0],image=$("theme-image-file").files?.[0];
  if(!file||file.size>65536){toast('请选择不超过 64 KiB 的 theme.json。',true);return;}
  if(image&&image.size>5*1024*1024){toast('壁纸不能超过 5 MiB。',true);return;}
  busy(true);$("theme-import-button").disabled=true;
  try{
    let imageBase64=null;
    if(image){const data=new Uint8Array(await image.arrayBuffer());const parts=[];for(let i=0;i<data.length;i+=8192)parts.push(String.fromCharCode(...data.subarray(i,i+8192)));imageBase64=btoa(parts.join(''));}
    const imported=await invoke('import_theme',{config:await file.text(),imageBase64,author:$("theme-author").value.trim(),license:$("theme-license").value.trim()});
    skinState.catalog=[];skinState.selected=null;skinState.page=0;$("skin-filter").value='all';$("skin-search").value='';await loadSkins();$("skin-apply").disabled=true;$("skin-selection").textContent='请选择皮肤';
    $("skin-feedback").textContent=`已导入 ${themeText(imported.name)}。请点击卡片预览，再应用到 Harness。只转换配色和图片。`;
    toast('主题已保存到本机，尚未应用。');
  }catch(e){toast(e.message||'导入失败，请检查配置和图片。',true);}finally{busy(false);$("theme-import-button").disabled=false;}
});
const recordStates={archived:'已移入回收站',pending_archive:'正在归档',pending_restore:'正在恢复',unknown:'结果未确认',restored:'已恢复',failed:'操作未生效',missing:'原生会话不存在'};
function renderTrash(records){
  state.records=records;state.selected.clear();$("restore-selected").disabled=true;$("check-delete").disabled=true;$("delete-report").hidden=true;
  const query=$("trash-search").value.trim().toLowerCase();
  const visible=records.filter(r=>r.state!=='restored'&&`${r.title} ${r.workspaceTitle}`.toLowerCase().includes(query));
  $("trash-list").replaceChildren();
  if(!visible.length){const empty=document.createElement('div');empty.className='empty-state';const h=document.createElement('h2');h.textContent=query?'没有匹配记录':'回收站为空';const p=document.createElement('p');p.textContent='只显示由本工具移入且尚未恢复的会话。';empty.append(h,p);$("trash-list").append(empty);return;}
  for(const record of visible){
    const row=document.createElement('div');row.className='trash-row';
    const check=document.createElement('input');check.type='checkbox';check.disabled=record.state!=='archived';check.setAttribute('aria-label',`选择 ${sessionLabel(record.title)}`);check.addEventListener('change',()=>{check.checked?state.selected.add(record.sessionId):state.selected.delete(record.sessionId);$("restore-selected").disabled=state.busy||!state.selected.size;$("check-delete").disabled=state.busy||!state.selected.size;});
    const info=document.createElement('div');const title=document.createElement('strong');title.textContent=sessionLabel(record.title)||'未命名会话';const meta=document.createElement('small');meta.textContent=`${sessionLabel(record.workspaceTitle)} · ${new Date(record.movedAt).toLocaleString('zh-CN')} · ${recordStates[record.state]||record.state}`;info.append(title,meta);if(record.error){const error=document.createElement('small');error.textContent=record.error;info.append(error);}
    const button=document.createElement('button');button.className='button';button.textContent='恢复';button.disabled=record.state!=='archived';button.addEventListener('click',()=>restore([record.sessionId]));row.append(check,info,button);$("trash-list").append(row);
  }
}
async function refreshTrash(){
  if(state.busy)return;busy(true);$("trash-error").hidden=true;
  try{renderTrash(await invoke('refresh_trash'));$("recycle-status").textContent='当前实例已核实，记录已与原生状态核对';}
  catch(e){$("trash-error").hidden=false;$("trash-error").textContent=e.message||'回收站无法刷新；请检查连接。';state.selected.clear();$("restore-selected").disabled=true;}
  finally{busy(false);}
}
async function restore(ids){
  if(state.busy||!ids.length)return;
  if(!window.confirm(`恢复所选 ${ids.length} 个会话？`))return;
  busy(true);try{const result=await invoke('restore_sessions',{sessionIds:ids});const failed=result.outcomes.filter(x=>!x.ok);toast(failed.length?failed[0].error:`已恢复 ${result.outcomes.length} 个会话`,failed.length>0);log(`恢复完成：${result.outcomes.filter(x=>x.ok).length} 项成功，${failed.length} 项失败`,failed.length>0);}
  catch(e){toast(e.message||'恢复失败',true);}finally{busy(false);await refreshTrash();}
}
$("refresh-trash").addEventListener('click',refreshTrash);
$("trash-search").addEventListener('input',()=>renderTrash(state.records));
$("restore-selected").addEventListener('click',()=>restore([...state.selected]));
$("check-delete").addEventListener('click',async()=>{
  if(state.busy||!state.selected.size)return;
  busy(true);$("delete-report").hidden=true;
  try{const report=await invoke('check_permanent_delete',{sessionIds:[...state.selected]});
    const box=$("delete-report");box.replaceChildren();const heading=document.createElement('strong');heading.textContent='只读检查完成：永久删除未开放';box.append(heading);
    const reason=document.createElement('p');reason.textContent=report.reason;box.append(reason);
    for(const target of report.targets){const item=document.createElement('p');item.textContent=`${sessionLabel(target.title)||target.sessionId}：${target.reason}`;box.append(item);}box.hidden=false;
    log('已完成删除能力只读检查，未执行删除');
  }catch(e){toast(e.message||'删除能力检查失败',true);}finally{busy(false);}
});
$("install-recycle").addEventListener('click',async()=>{if(state.busy)return;busy(true);try{const value=await invoke('install_recycle');state.recyclePreference=true;renderTrash(value.records);injection(value.injection);status(true,'当前 Harness 版本和 profile 已核实。');$("recycle-status").textContent='回收站增强已安装，请重新打开会话菜单';toast('已安装“移入回收站”菜单与侧栏入口。');log('原生回收站增强已安装');}catch(e){toast(e.message||'回收站增强安装失败',true);}finally{busy(false);}});
function toast(message, error = false) {
  clearTimeout(state.toastTimer);
  $("toast").textContent = message; $("toast").classList.toggle("error",error); $("toast").hidden=false;
  state.toastTimer=setTimeout(()=>$("toast").hidden=true,6500);
}
function log(message, error=false) {
  document.querySelector(".activity-empty")?.remove();
  const row = document.createElement("div"); row.className=`activity-item ${error?"error":"success"}`;
  const dot=document.createElement("span");dot.className="dot";row.append(dot);
  const body=document.createElement("div");body.textContent=message;
  const time=document.createElement("time");time.textContent=new Date().toLocaleTimeString("zh-CN",{hour:"2-digit",minute:"2-digit",second:"2-digit"});body.append(time);row.append(body);
  $("activity").prepend(row);while($("activity").children.length>30)$("activity").lastElementChild.remove();
}
function busy(value) {
  state.busy=value;
  for(const button of document.querySelectorAll("[data-action],#launch-harness,#refresh-targets,#settings-form button"))button.disabled=value;
  for(const button of document.querySelectorAll('#install-recycle,#refresh-trash,.trash-row button,.trash-row input'))button.disabled=value;
  $("restore-selected").disabled=value||!state.selected.size;
  $("check-delete").disabled=value||!state.selected.size;
  if(!value)for(const row of document.querySelectorAll('.trash-row')){const index=[...row.parentElement.children].indexOf(row);const query=$("trash-search").value.trim().toLowerCase();const record=state.records.filter(r=>r.state!=='restored'&&`${r.title} ${r.workspaceTitle}`.toLowerCase().includes(query))[index];for(const el of row.querySelectorAll('button,input'))el.disabled=record?.state!=='archived';}
}
function status(connected, message) {
  state.connected=connected;
  $("global-status").className=`status-pill ${connected?"success":"error"}`;
  $("global-status").replaceChildren();const dot=document.createElement("span");dot.className="dot";$("global-status").append(dot,connected?"最近检查已连接":"连接待检查");
  $("metric-connection").textContent=connected?"已连接":"未连接";
  $("connection-title").textContent=connected?"Harness 页面已识别":"连接尚未就绪";
  $("connection-description").textContent=message;
  if(!connected){$("metric-recycle").textContent="待连接核实";state.installed=null;$("metric-injection").textContent="状态未知";$("injection-badge").textContent="状态未知";$("metric-buttons").textContent="需连接后重新核对";$("injection-detail").textContent="连接断开，当前注入状态无法确认。";}
}
function injection(value) {
  state.installed=value.installed===true;$("metric-recycle").textContent=value.version==="0.2.0"&&value.dataOperations?"归档 / 恢复可用":"待安装增强";
  const label=state.installed?"已安装":"未安装";
  $("metric-injection").textContent=label;$("injection-badge").textContent=label;
  const detail=state.installed?(value.version==='0.2.0'?`回收站增强已安装，当前 ${value.recycleButtons||0} 个入口`:(value.testButtons>0?`当前检测到 ${value.testButtons} 个测试按钮` : "已安装；打开会话菜单后检查按钮")) : "页面增强已卸载或尚未安装";
  $("metric-buttons").textContent=detail;$("injection-detail").textContent=detail;
}
function renderResult(action,result) {
  state.failures=0;
  status(true,"已通过本机 CDP 检查。页面刷新或连接变化后请重新核对。");
  if(action==="diagnose") {
    $("browser-version").textContent=result.browser;$("diagnostic-output").textContent=JSON.stringify(result,null,2);injection(result.injection);
    maybeRestoreRecycle(result.injection);
  }
  if(action==="inject_test"||action==="remove_injection")injection(result);
  if(action==="probe_capabilities") {$("probe-output").textContent=JSON.stringify(result,null,2);$("browser-version").textContent=result.browser;$("export-report").disabled=false;$("report-summary").textContent="结构证据已采集。此报告仅含结构证据；原生回收站在安装增强时另行核实当前实例。";}
}
async function maybeRestoreRecycle(value) {
  if(!state.recyclePreference||state.restoringRecycle||state.installed===true||!state.connected)return;
  state.restoringRecycle=true;
  try {
    const installed=await invoke('install_recycle');
    renderTrash(installed.records);injection(installed.injection);
    $("recycle-status").textContent='回收站增强已自动恢复，请重新打开会话菜单';
    log('连接恢复后已自动恢复回收站增强');
  } catch(error) {
    // Avoid retrying the same failed install on every health tick. The persisted
    // preference is kept, so a fresh connection or restart can try again.
    state.recyclePreference=false;
    $("recycle-status").textContent='连接已恢复，但回收站增强待手动恢复';
    log(`回收站增强自动恢复失败：${error.message||'请手动安装增强'}`,true);
  } finally { state.restoringRecycle=false; }
}
async function run(action,quiet=false) {
  if(state.busy)return;
  busy(true);
  try {const result=await invoke("desktop_action",{action});renderResult(action,result);if(!quiet){log(`${actionNames[action]}成功`);toast(action==="inject_test"?"测试增强已安装，请打开会话菜单。":`${actionNames[action]}成功`);}}
  catch(error){const message=error.message||"操作失败，请检查连接。";status(false,message);if(!quiet){log(`${actionNames[action]}：${message}`,true);toast(message,true);}if(error.target_ids?.length){fillTargets(error.target_ids.map(id=>({id})));navigate("settings");}if(quiet&&++state.failures>=3){stopMonitor();log("连续 3 次检查失败，已暂停自动检查。",true);toast("连接连续失败，自动检查已暂停。",true);}}
  finally{busy(false);}
}
function fillTargets(targets) {
  const selected=state.settings?.target_id||"";
  $("target").replaceChildren(new Option("仅有一个匹配页面时自动选择",""));
  for(const target of targets)$("target").add(new Option(`Harness 页面 · ${target.id}`,target.id));
  if(selected&&!targets.some(t=>t.id===selected))$("target").add(new Option(`已保存（尚未发现）· ${selected}`,selected));
  $("target").value=selected;
}
function setSettings(settings){state.settings=settings;$("port").value=settings.port;$("executable").value=settings.executable;$("theme").value=settings.theme;document.documentElement.dataset.theme=settings.theme;fillTargets([]);$("metric-address").textContent=`127.0.0.1:${settings.port}`;$("connection-address").textContent=`127.0.0.1:${settings.port}`;}
function stopMonitor(){clearInterval(state.autoTimer);state.autoTimer=null;$("auto-check").checked=false;}
for(const button of document.querySelectorAll("[data-page]"))button.addEventListener("click",()=>navigate(button.dataset.page));
for(const button of document.querySelectorAll("[data-nav]"))button.addEventListener("click",()=>navigate(button.dataset.nav));
window.addEventListener('keydown',event=>{
  if(!event.ctrlKey||event.altKey||event.metaKey||event.key.toLowerCase()!=='k')return;
  if(event.target?.matches?.('input,textarea,select,[contenteditable="true"]'))return;
  event.preventDefault();navigate('sessions');window.openSessionSwitcher?.();
});
for(const button of document.querySelectorAll("[data-action]"))button.addEventListener("click",async()=>{if(button.dataset.action==='remove_injection')state.recyclePreference=false;await run(button.dataset.action);});
$("auto-check").addEventListener("change",()=>{const requested=$("auto-check").checked;stopMonitor();if(!requested)return;if(!state.connected){toast("请先成功检查一次连接。",true);return;}$("auto-check").checked=true;state.failures=0;state.autoTimer=setInterval(()=>run("diagnose",true),15000);});
$("disconnect").addEventListener("click",()=>{stopMonitor();toast("自动状态检查已停止；现有页面增强不受影响。");});
$("theme-toggle").addEventListener("click",()=>{const theme=document.documentElement.dataset.theme==="dark"?"light":"dark";document.documentElement.dataset.theme=theme;$("theme").value=theme;toast("主题已切换，保存设置后下次启动继续使用。");});
$("refresh-targets").addEventListener("click",async()=>{if(state.busy)return;busy(true);try{const targets=await invoke("discover_targets");fillTargets(targets);toast(`发现 ${targets.length} 个匹配页面。`);}catch(error){toast(error.message||"发现页面失败",true);}finally{busy(false);}});
$("settings-form").addEventListener("submit",async event=>{event.preventDefault();if(state.busy)return;busy(true);try{const previous=state.settings;const settings=await invoke("save_settings",{settings:{port:Number($("port").value),target_id:$("target").value||null,executable:$("executable").value.trim(),theme:$("theme").value}});setSettings(settings);if(previous?.port!==settings.port||previous?.target_id!==settings.target_id){stopMonitor();status(false,"配置已改变，请重新检查连接。");}toast("设置已保存。");log("管理配置已保存");}catch(error){toast(error.message||"保存失败",true);}finally{busy(false);}});
$("launch-harness").addEventListener("click",async()=>{if(state.busy)return;busy(true);try{await invoke("launch_harness");toast("已发送启动请求，请稍后检查连接。若已运行，需正常退出后再启动。");log("已请求启动 Harness");}catch(error){toast(error.message||"启动失败",true);}finally{busy(false);}});
$("export-report").addEventListener("click",async()=>{try{const path=await invoke("export_report");toast(`报告已保存：${path}`);}catch(error){toast(error.message||"报告保存失败",true);}});
$("quit").addEventListener("click",()=>invoke("quit_app").catch(error=>toast(error.message,true)));
$("stay").addEventListener("click",()=>$("cleanup-dialog").close());
$("force-quit").addEventListener("click",()=>invoke("exit_without_cleanup").catch(error=>toast(error.message,true)));
$("open-recommendation").addEventListener("click",async()=>{
  const button=$("open-recommendation");if(button.disabled)return;button.disabled=true;
  try{await invoke("open_recommendation");}catch(error){toast(error.message||"无法打开官网，请稍后重试。",true);}finally{button.disabled=false;}
});
(async()=>{
  try{
    setSettings(await invoke("load_settings"));
    await window.__TAURI__.event.listen("cleanup-failed",event=>{$("cleanup-message").textContent=event.payload;$("cleanup-dialog").showModal();});
    await window.__TAURI__.event.listen('open-trash',()=>{navigate('trash');refreshTrash();});
    await window.__TAURI__.event.listen('trash-changed',()=>{if(!state.busy)refreshTrash();});
    await window.__TAURI__.event.listen('recycle-disconnected',()=>{$("recycle-status").textContent='菜单连接已停止。重新安装增强后继续使用。';});
    await window.__TAURI__.event.listen("management-result",event=>{const {action,result,error}=event.payload;if(state.busy)return;if(error){status(false,error.message);log(`${actionNames[action]}：${error.message}`,true);toast(error.message,true);}else if(result){renderResult(action,result);log(`${actionNames[action]}成功`);}});
    // Load the local preference after event wiring so a slow preference read cannot
    // block cleanup handling or the first connection check.
    invoke('recycle_restore_preference').then(value=>{state.recyclePreference=value===true;}).catch(()=>{state.recyclePreference=false;});
  }catch(error){toast(error.message||"桌面服务初始化失败",true);}
})();
