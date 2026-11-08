import assert from 'node:assert/strict';
import {mkdir,writeFile} from 'node:fs/promises';
import {fileURLToPath,pathToFileURL} from 'node:url';
const {chromium}=await import(pathToFileURL(process.env.PLUTO_PLAYWRIGHT_PATH).href);
const browser=await chromium.launch({headless:true,executablePath:process.env.PLUTO_BROWSER_PATH});
const out=new URL('../../target/ui-review/hyper-glass/',import.meta.url);await mkdir(out,{recursive:true});
const report={contexts:0,pages:0,settings:0,buttons:0,errors:[],performance:[]};
const agent={connected:true,sessions:[],selectedId:'',capabilities:{sessionCapabilities:{additionalDirectories:true}},askResponse:{text:'A concise answer.'}};
const percent=(values,p)=>[...values].sort((a,b)=>a-b)[Math.floor((values.length-1)*p)]||0;
try{
 const cases=[...[380,680].flatMap(width=>[true,false].map(dark=>({width,dark,scale:1.25}))),...[1,1.5,2].map(scale=>({width:680,dark:true,scale}))];
 for(const {width,dark,scale} of cases){
  const c=await browser.newContext({viewport:{width,height:560},deviceScaleFactor:scale});const p=await c.newPage();p.on('pageerror',e=>report.errors.push(e.message));
  await p.route(/^https?:/,r=>r.abort());
  await p.addInitScript(()=>{
   localStorage.setItem('pluto.material.v1',JSON.stringify({enabled:true,motion:true,tint:31}));localStorage.setItem('pluto.appearance.mode.v1',JSON.stringify('hyper-glass'));
   localStorage.setItem('island.agent-preferences',JSON.stringify({defaultProvider:'codex',providerSetupComplete:true}));
   window.messages=[];window.ipc={postMessage(raw){const m=JSON.parse(raw);window.messages.push(m);if(m.kind==='page')window.islandReceive?.({page:m.page});}};
   const track={stop(){},addEventListener(){}};Object.defineProperty(navigator,'mediaDevices',{value:{getUserMedia:async()=>({getTracks:()=>[track],getAudioTracks:()=>[track]})}});
   window.AudioContext=undefined;window.MediaRecorder=class{state='inactive';static isTypeSupported(){return true;}start(){this.state='recording';queueMicrotask(()=>this.onstart?.());}stop(){this.state='inactive';queueMicrotask(()=>{this.ondataavailable?.({data:new Blob(['mock'])});this.onstop?.();});}};
  });
  await p.goto(new URL('../dist/index.html',import.meta.url).href);await p.waitForFunction(()=>window.islandHyperGlassDiagnostics);
  const receive=async data=>{await p.evaluate(s=>window.islandReceive(s),data);await p.waitForTimeout(80);};
  await receive({expanded:true,dark,page:'Home',agents:{codex:agent,opencode:{connected:true,sessions:[]}},speechSettings:{configured:true},startup:{enabled:true,canChange:true},textExpander:{enabled:true,entries:[]},noteFolders:['Notes'],noteFolderSelected:0,outputVolume:60,outputMuted:false});
  await p.evaluate(dark=>document.documentElement.style.background=dark?'#000':'#f6f6f7',dark);
  const shot=async name=>{if(scale!==1.25)return;await p.waitForTimeout(220);await p.screenshot({path:fileURLToPath(new URL(`${name}-${width}-${dark?'dark':'light'}.png`,out))});};
  const actions=async()=>{
   const list=await p.locator('.island button').evaluateAll(es=>es.filter(e=>e.getBoundingClientRect().height&&getComputedStyle(e).visibility!=='hidden').map(e=>({label:e.getAttribute('aria-label')||e.textContent.trim(),w:e.getBoundingClientRect().width,h:e.getBoundingClientRect().height,bg:getComputedStyle(e).backgroundImage,color:getComputedStyle(e).color,shadow:getComputedStyle(e).boxShadow})));
   assert.ok(list.length);for(const b of list){assert.ok(b.w>0&&b.h>0);assert.ok(b.color);report.buttons++;}
  };
  for(const page of ['Home','Clipboard','Shelf','Text','Agents','Ask','Note','Weather','Controls','Expand','Convert','Meetings']){
   await receive({page});await actions();report.pages++;
   if(['Home','Agents','Weather'].includes(page)){
    const fields=await p.locator('.home-launcher input,.agent-composer textarea,.wx-search input').evaluateAll(es=>es.map(e=>getComputedStyle(e).backgroundColor));
    assert.ok(fields.every(color=>color==='rgba(0, 0, 0, 0)'),'editable content does not paint a second surface');
   }
   if(['Home','Controls','Meetings'].includes(page))await shot(page.toLowerCase());
   if(page==='Agents'){
    await p.locator('.agent-composer textarea').fill('Keep my draft');
    const attach=await p.getByRole('button',{name:'Attach files',exact:true}).boundingBox();await p.mouse.move(attach.x+attach.width/2,attach.y+attach.height/2);await p.mouse.down();
    await p.waitForFunction(()=>document.querySelector('[data-hyper-optic]'));
    const decoded=await p.evaluate(async()=>{const image=new Image();image.src=document.querySelector('.hyper-defs feImage').getAttribute('href');await image.decode();return{w:image.naturalWidth,h:image.naturalHeight,filter:getComputedStyle(document.querySelector('[data-hyper-optic]')).backdropFilter};});assert.ok(decoded.w>0&&decoded.h>0&&decoded.filter.includes('url('),'optical bitmap decodes and is applied');await p.mouse.up();
    await p.getByRole('button',{name:'Dictate into task',exact:true}).click();await p.getByText('Listening',{exact:true}).waitFor();
    const r=await p.getByRole('button',{name:'Close dictation',exact:true}).boundingBox();assert.ok(Math.abs(r.width-r.height)<.1);await shot('dictation');await p.getByRole('button',{name:'Close dictation',exact:true}).click();
    await p.getByRole('tab',{name:'Options',exact:true}).click();await actions();await p.getByRole('tab',{name:'Task',exact:true}).click();
   }
   if(page==='Note')await shot('notes');
   if(page==='Expand'){await p.getByRole('textbox',{name:'Shortcut',exact:true}).fill('!hyper');await p.getByRole('textbox',{name:'Text to insert',exact:true}).fill('Hyper Glass');await shot('expand');await p.getByRole('button',{name:'Save shortcut',exact:true}).click();assert.ok(await p.evaluate(()=>window.messages.some(m=>m.kind==='textExpanderAdd')));}
  }
  await p.evaluate(()=>window.islandOpenSettings());
  for(const section of ['General','Status','Priority','Motion','Tools','Quick actions','Agents','Dictation','Voice','Ask','Conversion','Meetings']){
   await p.getByRole('navigation',{name:'Settings sections'}).getByRole('button',{name:section,exact:true}).click();await actions();report.settings++;
   if(section==='General'){
    await shot('settings');const toggle=p.getByRole('switch',{name:'Start with Windows',exact:true});
    const knob=await toggle.locator('i').boundingBox();assert.ok(Math.abs(knob.width-knob.height)<.1);await toggle.click();assert.ok(await p.evaluate(()=>window.messages.some(m=>m.kind==='startupSet')));
    const appearance=p.locator('.settings-appearance-segments');const destination=appearance.getByRole('button',{name:dark?'Light':'Dark',exact:true});await destination.scrollIntoViewIfNeeded();
    const start=await appearance.getByRole('button',{name:dark?'Dark':'Light',exact:true}).boundingBox(),end=await destination.boundingBox();
    await p.mouse.move(start.x+start.width/2,start.y+start.height/2);await p.mouse.down();await p.mouse.move(end.x+end.width/2,end.y+end.height/2,{steps:5});await p.mouse.up();
    await p.waitForFunction(()=>window.messages.some(m=>m.kind==='themeToggle'));
    await p.getByRole('switch',{name:'Hyper Glass motion',exact:true}).scrollIntoViewIfNeeded();
    const hyperSwitch=p.getByRole('switch',{name:'Hyper Glass motion',exact:true}),box=await hyperSwitch.boundingBox();
    await p.mouse.move(box.x+box.width-5,box.y+box.height/2);await p.mouse.down();await p.waitForTimeout(60);await p.mouse.move(box.x+4,box.y+box.height/2,{steps:5});await p.mouse.up();
    await p.waitForFunction(()=>document.querySelector('[aria-label="Hyper Glass motion"]').getAttribute('aria-checked')==='false');await hyperSwitch.click();
   }
   if(section==='Agents')for(const label of ['Providers','Defaults','MCP tools','Shared skills','Shared system prompt','Presentation']){await p.locator('.agent-settings-menu').getByRole('button',{name:new RegExp(label)}).click();await actions();await p.getByRole('button',{name:'Back to Agent settings',exact:true}).click();}
  }
  // Every mode has a distinct native message. Preferences and runtime are independent.
  await p.getByRole('navigation',{name:'Settings sections'}).getByRole('button',{name:'General',exact:true}).click();
  const picker=p.getByRole('group',{name:'Appearance mode',exact:true});
  const legacy=await p.evaluate(()=>localStorage.getItem('pluto.material.v1'));
  for(let i=0;i<3;i++)for(const [name,mode] of [['Standard','standard'],['Liquid Glass','glass'],['Hyper Glass','hyper-glass']]){
   await picker.getByRole('button',{name,exact:true}).click();await p.waitForFunction(mode=>document.querySelector('.island').dataset.material===mode,mode);
   if(mode==='hyper-glass')await p.waitForFunction(()=>window.islandHyperGlassDiagnostics);else{
    assert.equal(await p.evaluate(()=>typeof window.islandHyperGlassDiagnostics),'undefined');assert.equal(await p.locator('.hyper-defs,.hyper-selection-lens,.hyper-raised-lens,[data-hyper-track],[data-hyper-held],[data-hyper-surface],[data-hyper-anchor],[data-hyper-host],[data-hyper-physical]').count(),0);
   }
   assert.equal(await p.evaluate(()=>localStorage.getItem('pluto.material.v1')),legacy);
  }
  await p.getByRole('button',{name:'Back to app',exact:true}).click();await receive({page:'Agents'});assert.equal(await p.locator('.agent-composer textarea').inputValue(),'Keep my draft');
  // A single scheduler must become fully idle. Measure bounded callback work.
  const frames=await p.evaluate(async()=>{const gaps=[];let last=performance.now();for(let i=0;i<30;i++){await new Promise(r=>requestAnimationFrame(r));const now=performance.now();gaps.push(now-last);last=now;}return gaps;});
  for(const page of ['Home','Text','Note','Agents','Controls','Home']){await p.getByRole('navigation',{name:'Island pages'}).getByRole('button',{name:page,exact:true}).click();await p.waitForTimeout(110);}
  await p.waitForFunction(()=>window.islandHyperGlassDiagnostics().active===0&&!window.islandHyperGlassDiagnostics().pendingFrame);
  const before=await p.evaluate(()=>window.islandHyperGlassDiagnostics());await p.waitForTimeout(220);const after=await p.evaluate(()=>window.islandHyperGlassDiagnostics());assert.equal(after.frames,before.frames,'idle scheduler sleeps');assert.ok(after.cacheBytes<=16*1024*1024);assert.ok(after.cacheEntries<=32);
  report.performance.push({width,dark,scale,rafP95:percent(frames,.95),...after});
  await p.emulateMedia({reducedMotion:'reduce'});await p.getByRole('navigation',{name:'Island pages'}).getByRole('button',{name:'Agents',exact:true}).click();await p.waitForTimeout(100);assert.equal(await p.evaluate(()=>window.islandHyperGlassDiagnostics().active),0);
  await p.evaluate(()=>window.islandOpenSettings());await p.getByRole('navigation',{name:'Settings sections'}).getByRole('button',{name:'General',exact:true}).click();await p.getByRole('switch',{name:'Hyper Glass motion',exact:true}).click();
  assert.ok(await p.evaluate(()=>window.messages.some(m=>m.kind==='hyperGlassMaterial'&&m.enabled&&m.motion===false)));
  await p.reload();await p.waitForFunction(()=>window.islandHyperGlassDiagnostics);assert.equal(await p.evaluate(()=>JSON.parse(localStorage.getItem('pluto.hyper-glass.v1')).motion),false);
  report.contexts++;await c.close();
 }
 assert.deepEqual(report.errors,[]);await writeFile(new URL('report.json',out),JSON.stringify(report,null,2));console.log(JSON.stringify(report));
}finally{await browser.close();}
