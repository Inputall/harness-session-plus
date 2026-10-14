(request => {
  'use strict';
  const key='__HARNESS_SESSION_PLUS_SKIN__', id='hsp-skin-style';
  const tokens=['--dsw-alias-bg-base','--dsw-specific-sidebar-fill','--dsw-alias-label-primary','--dsw-alias-state-business-primary'];
  const existing=window[key], node=document.getElementById(id);
  if((existing&&!['1.0.0','1.1.0','1.2.0'].includes(existing.version))||(node&&(!existing||node!==existing.node)))throw Error('Unknown skin installation');
  const supported=!!document.body&&tokens.every(t=>getComputedStyle(document.body).getPropertyValue(t).trim());
  if(request.action==='status')return {supported,active:existing?.skinId||null,installed:!!existing&&existing.node.isConnected,revision:existing?.revision||null};
  if(request.action==='reset'){
    existing?.node.remove();delete window[key];return {supported,active:null,installed:false};
  }
  if(request.action!=='apply'||!supported)throw Error('Theme tokens unavailable');
  const skin=request.skin;
  if(!skin||!/^[a-z0-9-]{1,100}$/.test(skin.id))throw Error('Invalid skin');
  const validColor=color=>{
    if(/^#(?:[0-9a-f]{6}|[0-9a-f]{8})$/i.test(color))return true;
    const m=/^rgba\((\d+),(\d+),(\d+),(0(?:\.\d+)?|1(?:\.0+)?)\)$/.exec(color);
    return !!m&&m.slice(1,4).every(v=>Number(v)<=255);
  };
  for(const color of [skin.bg,skin.surface,skin.text,skin.muted,skin.accent,skin.border,skin.hover])if(!validColor(color))throw Error('Invalid color');
  if(skin.wallpaper&&(!/^data:image\/(png|jpeg|webp);base64,[A-Za-z0-9+/]+={0,2}$/.test(skin.wallpaper)||skin.wallpaper.length>7000000))throw Error('Invalid wallpaper');
  const clarity=skin.wallpaperClarity??70;
  if(!Number.isInteger(clarity)||clarity<0||clarity>100)throw Error('Invalid wallpaper clarity');
  const translucent=(color,alpha)=>{
    let rgb,original=1;
    if(color.startsWith('#')){rgb=[1,3,5].map(i=>parseInt(color.slice(i,i+2),16));if(color.length===9)original=parseInt(color.slice(7,9),16)/255;}
    else {const m=/^rgba\((\d+),(\d+),(\d+),([^\)]+)\)$/.exec(color);rgb=m.slice(1,4).map(Number);original=Number(m[4]);}
    return `rgba(${rgb.join(',')},${Math.min(original,alpha).toFixed(3)})`;
  };
  const values={
    '--dsw-alias-bg-base':skin.bg,'--dsw-alias-bg-layer-1':skin.surface,'--dsw-alias-bg-layer-2':skin.hover,
    '--dsw-alias-bg-layer-3':skin.hover,'--dsw-alias-bg-module-platform':skin.surface,
    '--dsw-specific-sidebar-fill':skin.surface,'--dsw-specific-sidebar-nav-item-active':skin.hover,
    '--dsw-specific-sidebar-nav-item-active-accent':skin.hover,'--dsw-specific-sidebar-nav-item-hover':skin.hover,
    '--dsw-specific-input-major':skin.surface,'--dsw-specific-bubble':skin.hover,
    '--dsw-alias-label-primary':skin.text,'--dsw-alias-label-primary-dimmed':skin.text,
    '--dsw-alias-label-primary-bluish':skin.text,'--dsw-alias-label-secondary':skin.muted,
    '--dsw-alias-label-tertiary':skin.muted,'--dsw-alias-border-l1':skin.border,'--dsw-alias-border-l2':skin.border,
    '--dsw-alias-state-business-primary':skin.accent,'--dsw-alias-link':skin.accent,
    '--dsw-alias-interactive-bg-hover':skin.hover,'--dsw-alias-interactive-bg-active':skin.hover,
    '--dsw-menu-surface-fill':skin.surface,'--dsw-specific-menu':skin.surface,
    '--dsw-alias-menu-group-header-fill':skin.surface,'--dsw-focus-ring-color':skin.accent
  };
  if(skin.wallpaper){
    values['--dsw-alias-bg-base']='transparent';
    const panelAlpha=0.95-clarity*0.007;
    for(const token of ['--dsw-alias-bg-layer-1','--dsw-alias-bg-module-platform','--dsw-specific-sidebar-fill'])values[token]=translucent(skin.surface,panelAlpha);
    for(const token of ['--dsw-alias-bg-layer-2','--dsw-alias-bg-layer-3'])values[token]=translucent(skin.hover,Math.min(0.96,panelAlpha+0.12));
  }
  const css='body,body[data-ds-dark-theme]{'+Object.entries(values).map(([k,v])=>`${k}:${v}!important`).join(';')+'}'+(skin.wallpaper?`body{background-color:${skin.bg}!important;background-image:url("${skin.wallpaper}")!important;background-size:cover!important;background-position:center!important;background-attachment:fixed!important}`:'');
  const style=node||document.createElement('style');style.id=id;style.textContent=css;
  if(!node)document.head.append(style);
  window[key]={version:'1.2.0',skinId:skin.id,node:style,revision:skin.persistenceRevision||null};
  return {supported:true,active:skin.id,installed:style.isConnected,wallpaper:!!skin.wallpaper,wallpaperClarity:clarity};
})
