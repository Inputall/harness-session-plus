const test=require('node:test');const assert=require('node:assert/strict');const fs=require('node:fs');const vm=require('node:vm');
const source=fs.readFileSync(require('node:path').join(__dirname,'../injected/skin.js'),'utf8');
const catalog=JSON.parse(fs.readFileSync(require('node:path').join(__dirname,'../desktop/ui/skins.json'),'utf8'));
function env(supported=true){const nodes=new Map();const window={};const document={body:{},head:{append(n){nodes.set(n.id,n);n.isConnected=true;}},getElementById:id=>nodes.get(id),createElement(){return {isConnected:false,remove(){nodes.delete(this.id);this.isConnected=false;}};}};const context=vm.createContext({window,document,getComputedStyle:()=>({getPropertyValue:()=>supported?'#123456':''})});return {window,nodes,run:r=>vm.runInContext(`(${source})(${JSON.stringify(r)})`,context)};}
test('all palettes replace a single owned style; restore removes only that overlay',()=>{const e=env();e.nodes.set('native-theme',{textContent:'native'});for(const skin of catalog){const result=e.run({action:'apply',skin});assert.equal(result.active,skin.id);assert.equal(result.installed,true);assert.equal(e.nodes.size,2);assert.match(e.nodes.get('hsp-skin-style').textContent,/--dsw-alias-bg-layer-1/);assert(!e.nodes.get('hsp-skin-style').textContent.includes('url('));}e.run({action:'reset'});assert.equal(e.nodes.size,1);assert.equal(e.nodes.get('native-theme').textContent,'native');assert.equal(e.run({action:'status'}).installed,false);});
test('unsupported page, unknown installation and CSS injection are rejected before mutation',()=>{const e=env(false);assert.equal(e.run({action:'status'}).supported,false);assert.throws(()=>e.run({action:'apply',skin:catalog[0]}));assert.equal(e.nodes.size,0);const foreign=env();foreign.nodes.set('hsp-skin-style',{textContent:'foreign'});assert.throws(()=>foreign.run({action:'reset'}));assert.equal(foreign.nodes.get('hsp-skin-style').textContent,'foreign');const invalid=env();assert.throws(()=>invalid.run({action:'apply',skin:{...catalog[0],accent:'#fff;display:none'}}));assert.equal(invalid.nodes.size,0);});
test('converted wallpaper and alpha panels are applied together and fully removed on reset',()=>{const e=env();const skin={...catalog[0],id:'caishen-lite',surface:'rgba(255,251,244,0.9)',wallpaper:'data:image/png;base64,aGVsbG8='};e.run({action:'apply',skin});const css=e.nodes.get('hsp-skin-style').textContent;assert.match(css,/background-image:url\("data:image\/png;base64,/);assert.match(css,/--dsw-alias-bg-base:transparent/);assert.match(css,/rgba\(255,251,244,0.9\)/);e.run({action:'apply',skin:catalog[1]});assert(!e.nodes.get('hsp-skin-style').textContent.includes('background-image'));e.run({action:'reset'});assert.equal(e.nodes.size,0);assert.throws(()=>e.run({action:'apply',skin:{...skin,wallpaper:'https://example.com/a.png'}}));assert.equal(e.nodes.size,0);});

test('wallpaper clarity reduces panel coverage while preserving menus and input contrast',()=>{
 const e=env(),skin={...catalog[0],wallpaper:'data:image/png;base64,aGVsbG8='};
 for(const [clarity,alpha] of [[0,'0.950'],[70,'0.460'],[100,'0.250']]){
  const result=e.run({action:'apply',skin:{...skin,wallpaperClarity:clarity}});
  assert.equal(result.wallpaperClarity,clarity);
  const css=e.nodes.get('hsp-skin-style').textContent;
  assert(css.includes(`--dsw-specific-sidebar-fill:rgba(25,34,53,${alpha})`));
  assert(css.includes(`--dsw-specific-input-major:${skin.surface}`));
  assert(css.includes(`--dsw-specific-menu:${skin.surface}`));
 }
 const before=e.nodes.get('hsp-skin-style').textContent;
 for(const value of [-1,101,70.5,'70'])assert.throws(()=>e.run({action:'apply',skin:{...skin,wallpaperClarity:value}}));
 assert.equal(e.nodes.get('hsp-skin-style').textContent,before);
 e.run({action:'reset'});assert.equal(e.nodes.size,0);
});
