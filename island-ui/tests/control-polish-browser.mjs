import assert from 'node:assert/strict';
import {mkdir,writeFile} from 'node:fs/promises';
import {pathToFileURL,fileURLToPath} from 'node:url';
const {chromium}=await import(pathToFileURL(process.env.PLUTO_PLAYWRIGHT_PATH).href);
const browser=await chromium.launch({headless:true,executablePath:process.env.PLUTO_BROWSER_PATH});
const out=new URL('../../target/ui-review/control-polish/',import.meta.url);await mkdir(out,{recursive:true});const report=[];
try{
 for(const mode of ['standard','glass','hyper-glass'])for(const dark of [false,true])for(const width of [380,680]){
  const context=await browser.newContext({viewport:{width,height:560},deviceScaleFactor:1.25}),p=await context.newPage(),errors=[];p.on('pageerror',e=>errors.push(e.message));await p.route(/^https?:/,r=>r.abort());
  await p.addInitScript(mode=>{
   localStorage.setItem('pluto.appearance.mode.v1',JSON.stringify(mode));localStorage.setItem('pluto.material.v1',JSON.stringify({enabled:mode==='glass',motion:true}));localStorage.setItem('island.agent-preferences',JSON.stringify({defaultProvider:'opencode',providerSetupComplete:true}));
   window.messages=[];window.ipc={postMessage(raw){const m=JSON.parse(raw);window.messages.push(m);if(m.kind==='page')window.islandReceive?.({page:m.page});if(m.kind==='pinToggle')window.islandReceive?.({pinned:!document.querySelector('.pin-toggle')?.classList.contains('pinned')});}};
   const track={stop(){},addEventListener(){}};Object.defineProperty(navigator,'mediaDevices',{value:{getUserMedia:async()=>({getTracks:()=>[track],getAudioTracks:()=>[track]})}});window.AudioContext=undefined;
   window.MediaRecorder=class{state='inactive';static isTypeSupported(){return true;}start(){this.state='recording';queueMicrotask(()=>this.onstart?.());}stop(){this.state='inactive';queueMicrotask(()=>{this.ondataavailable?.({data:new Blob(['mock'])});this.onstop?.();});}};
  },mode);
  await p.goto(new URL('../dist/index.html',import.meta.url).href);await p.waitForFunction(()=>window.islandReceive);
  const receive=async s=>{await p.evaluate(s=>window.islandReceive(s),s);await p.waitForTimeout(200);};
  const shot=async name=>p.screenshot({path:fileURLToPath(new URL(`${name}-${mode}-${dark?'dark':'light'}-${width}.png`,out))});
  const check=async(el,recording=false)=>{
   await p.mouse.move(0,0);await p.waitForTimeout(200);
   const base=await el.evaluate(e=>({color:getComputedStyle(e).color,image:getComputedStyle(e).backgroundImage,filter:getComputedStyle(e).filter}));
   await el.hover();await p.waitForTimeout(220);const hover=await el.evaluate(e=>({color:getComputedStyle(e).color,image:getComputedStyle(e).backgroundImage,filter:getComputedStyle(e).filter}));
   assert.equal(hover.color,base.color,'hover retains foreground');assert.equal(hover.image,base.image,'hover retains the material layer');assert.equal(hover.filter,'none','hover does not brighten the whole control');
   if(!recording)assert.equal(hover.color,await p.locator('.island').evaluate(e=>getComputedStyle(e).color),'secondary controls have neutral ink');
  };
  await receive({expanded:true,dark,page:'Home',pinned:false,speechSettings:{configured:true},outputVolume:60,opencode:{agents:[{id:'Build',mode:'primary'}],models:[{key:'deepseek/deepseek-v4-pro',name:'DeepSeek V4 Pro',provider:'DeepSeek'}]},agents:{opencode:{connected:true,sessions:[]},codex:{connected:true,sessions:[]}},startup:{enabled:true,canChange:true}});await p.evaluate(dark=>document.documentElement.style.background=dark?'#000':'#f6f6f7',dark);
  await check(p.locator('.pin-toggle'));await p.locator('.pin-toggle').click();await check(p.locator('.pin-toggle'));assert.equal(await p.locator('.pin-toggle').getAttribute('aria-pressed'),'true');
  await p.getByRole('navigation',{name:'Island pages'}).getByRole('button',{name:'Agents',exact:true}).click();await p.getByRole('tab',{name:'Profiles',exact:true}).click();await p.getByRole('button',{name:'New profile',exact:true}).click();await check(p.locator('.opencode-profile-back'));
  const tabs=p.getByRole('tablist',{name:'Profile settings'});for(const b of await tabs.getByRole('tab').all()){await check(b);await b.click();}await tabs.getByRole('tab',{name:'Details',exact:true}).click();
  for(const label of ['Agent role','Preferred model']){
   const trigger=p.getByRole('combobox',{name:label,exact:true});await check(trigger);await trigger.click();const menu=p.getByRole('listbox',{name:label,exact:true});for(const b of await menu.getByRole('option').all())await check(b);await shot(label==='Agent role'?'role-menu':'model-menu');await p.keyboard.press('Escape');
  }
  await p.getByRole('navigation',{name:'Island pages'}).getByRole('button',{name:'Home',exact:true}).click();await p.locator('.actions[aria-label="Quick actions"]').getByRole('button',{name:'Dictate',exact:true}).click();await check(p.getByRole('button',{name:'Start dictation',exact:true}));await shot('dictate-idle');await p.getByRole('button',{name:'Start dictation',exact:true}).click();await p.getByRole('button',{name:'Stop dictation and transcribe',exact:true}).waitFor();await check(p.getByRole('button',{name:'Stop dictation and transcribe',exact:true}),true);await check(p.getByRole('button',{name:'Cancel dictation',exact:true}));await shot('dictate-recording');await p.getByRole('button',{name:'Cancel dictation',exact:true}).click();
  await p.evaluate(()=>window.islandOpenSettings());await p.waitForTimeout(250);const segments=p.locator('.settings-segments').first();const weights=await segments.getByRole('button').evaluateAll(es=>es.map(e=>getComputedStyle(e).fontWeight));assert.ok(weights.length>=2);assert.ok(weights.every(w=>w==='500'),'active and inactive settings choices use a stable medium weight');await shot('settings-weight');
  await p.emulateMedia({reducedMotion:'reduce'});assert.equal(await p.locator('.pin-toggle').evaluate(e=>getComputedStyle(e).transitionDuration),'0s');
  assert.deepEqual(errors,[]);report.push({mode,dark,width,errors:0});await context.close();
 }
 await writeFile(new URL('report.json',out),JSON.stringify(report,null,2));console.log(`Passed profile, pin, dictation and settings-weight checks in ${report.length} mode/theme/width contexts.`);
}finally{await browser.close();}
