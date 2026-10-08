const fs=require('fs');
/* Harness for prototype.html interaction logic. Reads the <script> out of the HTML,
   appends one test-only hook (window.__stack), and runs it against a mock DOM.
   Run: node proto-dom-test.js  (from this directory) */
const HERE=__dirname;
const HTMLPATH=require('path').join(HERE,'prototype.html');
const raw=fs.readFileSync(HTMLPATH,'utf8').match(/<script>([\s\S]*?)<\/script>/)[1];
/* inject the test hook inside the IIFE, before its closing })(); */
const js=raw.replace(/\n  render\(\);\n\}\)\(\);/, '\n  render();\n  window.__stack = stack; /* test hook, harness only */\n})();');
if(!/window\.__stack/.test(js)) throw new Error('harness hook injection failed: IIFE tail changed');


function mkEl(id,cls,extra={}){
  const attrs=Object.assign({},extra);
  const el={
    id, cls:new Set(cls.split(' ').filter(Boolean)), tag:'div',
    listeners:{},
    dataset:{},
    style:{},
    children:[],
    classList:{
      toggle(c,on){ on?el.cls.add(c):el.cls.delete(c); },
      add(c){el.cls.add(c);}, remove(c){el.cls.delete(c);},
      contains(c){return el.cls.has(c);}
    },
    setAttribute(k,v){attrs[k]=String(v);},
    getAttribute(k){return k in attrs?attrs[k]:null;},
    removeAttribute(k){delete attrs[k];},
    addEventListener(t,f){(el.listeners[t]=el.listeners[t]||[]).push(f);},
    focus(){global.__focus=this;},
    querySelector(sel){
      if(sel==='.tabbar button') return el._tabs||[];
      if(sel==='button') return el._first||null;
      return null;
    },
    querySelectorAll(sel){
      if(sel==='.tabbar button') return el._tabs||[];
      return [];
    },
    _attrs:attrs
  };
  return el;
}

// screens
const screenIds=['s-home','s-terminal','s-aichat','s-files','s-switcher','s-manage','s-detail','s-settings'];
const screens=screenIds.map(id=>{
  const tabIds={'s-home':'s-home','s-manage':'s-manage','s-settings':'s-settings'};
  const tabs=[];
  if(tabIds[id]){
    for(const t of ['s-home','s-manage','s-settings']){
      const b=mkEl(null,'tabbtn',{onclick:"go('"+t+"')"});
      if(t===id) b.dataset.tab=t;
      tabs.push(b);
    }
  }
  const s=mkEl(id,'screen');
  s._tabs=tabs;
  return s;
});
const navButtons=['s-home','s-terminal','s-aichat','s-files','s-switcher','s-manage','s-detail','s-settings','demo-perm']
  .map(g=>{const b=mkEl(null,'navbtn');b.dataset.go=g;return b;});
const themeBtns=['dark','light'].map(t=>{const b=mkEl(null,'tbtn');b.dataset.themeSet=t;return b;});
const paletteBtns=['default','forest','ocean','sunset','violet'].map(p=>{const b=mkEl(null,'pbtn');b.dataset.paletteSet=p;return b;});

const byId={};
screenIds.forEach((id,i)=>byId[id]=screens[i]);
byId['phone']=mkEl('phone','phone');
byId['ov-capsule']=mkEl('ov-capsule','overlay'); byId['ov-capsule']._first=mkEl('capsule-first','');
byId['ov-perm']=mkEl('ov-perm','overlay'); byId['ov-perm']._first=mkEl('perm-first','');
byId['capsule-title']=mkEl('capsule-title','');

const overlays=[byId['ov-capsule'],byId['ov-perm']];

global.document={
  documentElement:mkEl('html',''),
  activeElement:null,
  getElementById:id=>byId[id]||null,
  querySelectorAll(sel){
    if(sel==='.screen') return screens;
    if(sel==='#nav button') return navButtons;
    if(sel==='#theme-seg button') return themeBtns;
    if(sel==='#palette-seg button') return paletteBtns;
    if(sel==='.overlay.show') return overlays.filter(o=>o.cls.has('show'));
    return [];
  },
  createElement:()=>mkEl(null,''),
  _docListeners:{},
  addEventListener(t,f){(this._docListeners[t]=this._docListeners[t]||[]).push(f);},
  dispatch(t,ev){(this._docListeners[t]||[]).forEach(f=>f(ev));}
};
const store={};
global.localStorage={getItem:k=>k in store?store[k]:null,setItem:(k,v)=>{store[k]=v;}};
global.window={
  matchMedia:()=>({matches:false,addEventListener(){},addListener(){}}),
};
global.setTimeout=(f)=>{f();return 0;};

// run
const win=global.window;
const fn=new Function('document','window','localStorage','setTimeout',js+'\n;return {go:window.go,closeSheet:window.closeSheet,openSheet:window.openSheet,flip:window.flip};');
const api=fn(global.document,win,global.localStorage,global.setTimeout);
console.log('window exports:',['go','closeSheet','openSheet','flip','closeApp'].map(k=>k+'='+typeof win[k]).join(' '));

function state(label){
  const cur=screens.filter(s=>s.cls.has('cur')).map(s=>s.id);
  const left=screens.filter(s=>s.cls.has('left')).map(s=>s.id);
  const navOn=navButtons.filter(b=>b.cls.has('on')).map(b=>b.dataset.go);
  const ariaCur=[];
  screens.forEach(s=>s._tabs.forEach(b=>{if(b.getAttribute('aria-current')==='page')ariaCur.push(s.id+':'+b.dataset.tab);}));
  const hid=screens.filter(s=>s.getAttribute('aria-hidden')==='true').map(s=>s.id).length;
  console.log(`${label.padEnd(26)} cur=[${cur}] left=[${left}] navOn=[${navOn}] ariaCurrent=[${ariaCur}] ariaHiddenCount=${hid}`);
  return {cur,left,navOn,ariaCur,hid};
}
let fails=0;
function expect(cond,msg){ if(!cond){console.log('  FAIL:',msg);fails++;} else console.log('  ok:',msg); }

console.log('theme attr on <html>:',document.documentElement.getAttribute('data-theme'));
console.log('palette on #phone:',byId['phone'].getAttribute('data-palette'));
console.log('theme segmented pressed:',themeBtns.map(b=>b.dataset.themeSet+'='+b.getAttribute('aria-pressed')).join(' '));
console.log('palette pressed:',paletteBtns.map(b=>b.dataset.paletteSet+'='+b.getAttribute('aria-pressed')).join(' '));
console.log();
const s0=state('initial');
expect(s0.cur.length===1&&s0.cur[0]==='s-home','exactly one current screen: s-home');
expect(s0.navOn.length===1&&s0.navOn[0]==='s-home','nav highlights s-home');
expect(s0.ariaCur.length===1&&s0.ariaCur[0]==='s-home:s-home','tabbar aria-current only on home tab');
expect(s0.hid===7,'7 screens aria-hidden');
console.log();
api.go('s-terminal'); const s1=state("go('s-terminal')");
expect(s1.cur[0]==='s-terminal','terminal is current');
expect(s1.left[0]==='s-home','home slides left');
expect(s1.ariaCur.length===0,'terminal has no tabbar aria-current (no tabbar)');
console.log();
api.go('s-manage'); const s2=state("go('s-manage')");
expect(s2.cur[0]==='s-manage','manage current');
expect(s2.ariaCur.length===1&&s2.ariaCur[0]==='s-manage:s-manage','only manage tab marked');
console.log();
api.go('s-detail'); const s3=state("go('s-detail')");
expect(s3.ariaCur.length===0,'detail page: no active tab');
api.go('s-manage'); const s4=state("back to s-manage");
expect(s4.cur[0]==='s-manage','returns to manage, instance reused');
console.log();
api.go('s-home'); const s5=state("go('s-home') resets stack");
expect(s5.cur[0]==='s-home'&&s5.left.length===0,'home current, nothing left');
expect(win.__stack.length===1,'stack collapsed to 1');
console.log();
api.go('s-aichat'); api.openSheet('ov-perm'); const s6=state("open perm sheet over aichat");
expect(byId['ov-perm'].cls.has('show'),'perm sheet shown');
expect(global.__focus===byId['ov-perm']._first,'focus moved to first button in sheet');
api.closeSheet('ov-perm');
expect(!byId['ov-perm'].cls.has('show'),'perm sheet hidden after close');
console.log();
const before=byId['ov-capsule'].cls.has('show'); api.openSheet('ov-capsule','终端');
expect(byId['capsule-title'].textContent==='终端','capsule title set via textContent');
console.log();
const tg=mkEl(null,'toggle'); tg.setAttribute('aria-checked','true');
api.flip(tg);
expect(tg.getAttribute('aria-checked')==='false','flip toggles aria-checked true->false');
api.flip(tg);
expect(tg.getAttribute('aria-checked')==='true','flip toggles back false->true');
console.log();
console.log();
console.log('== theme / palette switches ==');
themeBtns[1].listeners.click[0]();   // light
expect(document.documentElement.getAttribute('data-theme')==='light','theme -> light');
expect(themeBtns[1].getAttribute('aria-pressed')==='true'&&themeBtns[0].getAttribute('aria-pressed')==='false','segmented state follows');
expect(JSON.parse(store['bedcode-proto-v0.2']).theme==='light','theme persisted to localStorage');
themeBtns[0].listeners.click[0]();
expect(document.documentElement.getAttribute('data-theme')==='dark','theme -> dark');
paletteBtns[2].listeners.click[0]();  // ocean
expect(byId['phone'].getAttribute('data-palette')==='ocean','palette -> ocean on #phone');
expect(paletteBtns.filter(b=>b.getAttribute('aria-pressed')==='true').length===1,'exactly one swatch pressed');
expect(JSON.parse(store['bedcode-proto-v0.2']).palette==='ocean','palette persisted');
console.log();
console.log('== Escape closes sheets ==');
api.openSheet('ov-capsule');
api.openSheet('ov-perm');
expect(document.querySelectorAll('.overlay.show').length===2,'two sheets open');
document.dispatch('keydown',{key:'Escape'});
expect(document.querySelectorAll('.overlay.show').length===0,'Escape closed all sheets');
document.dispatch('keydown',{key:'a'});
expect(document.querySelectorAll('.overlay.show').length===0,'non-Escape key is a no-op');
console.log();
console.log(fails===0?'ALL DOM TESTS PASSED':(fails+' DOM TEST FAILURES'));
process.exit(fails?1:0);
