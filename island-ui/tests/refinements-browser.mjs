import assert from 'node:assert/strict';
import { mkdir, writeFile } from 'node:fs/promises';
import { fileURLToPath, pathToFileURL } from 'node:url';
const { chromium } = await import(pathToFileURL(process.env.PLUTO_PLAYWRIGHT_PATH).href);
const browser = await chromium.launch({ headless:true, executablePath:process.env.PLUTO_BROWSER_PATH });
const output = new URL('../../target/ui-review/refinements/', import.meta.url);
await mkdir(output,{recursive:true});
const report = {sliderCases:0,artCases:0,compactCases:0,provider:{},ask:{},errors:[]};
const options = [{id:'model',name:'Model',type:'select',currentValue:'gpt-6-sol',options:[{value:'gpt-6-sol',name:'GPT-6 Sol'},{value:'gpt-6-astra',name:'GPT-6 Astra'}]},{id:'approval',name:'Approvals',type:'select',currentValue:'on-request',options:[{value:'on-request',name:'On request'}]}];
const agent = {connected:true,sessions:[],selectedId:'',capabilities:{sessionCapabilities:{list:{}}}};
try {
 const context = await browser.newContext({viewport:{width:680,height:560}});
 const page = await context.newPage();
 page.on('pageerror',error=>report.errors.push(error.message));
 await page.route(/^https?:/,route=>route.abort());
 await page.addInitScript(()=>{
  localStorage.setItem('island.agent-preferences',JSON.stringify({providerSetupComplete:true,defaultProvider:'codex',provider:{codex:{directory:'C:\\project',config:{model:'gpt-6-sol'}},opencode:{config:{model:'other-model'}}}}));
  window.messages=[];
  window.ipc={postMessage(raw){const m=JSON.parse(raw);window.messages.push({...m,at:performance.now()});}};
  window.meterEnergy=0;
  const track={stop(){},addEventListener(){}};
  Object.defineProperty(navigator,'mediaDevices',{value:{getUserMedia:async()=>({getTracks:()=>[track],getAudioTracks:()=>[track]})}});
  window.MediaRecorder=class {
   state='inactive';static isTypeSupported(){return true;}
   start(){this.state='recording';queueMicrotask(()=>this.onstart?.());}
   stop(){this.state='inactive';queueMicrotask(()=>{this.ondataavailable?.({data:new Blob(['sample audio'],{type:'audio/webm'})});this.onstop?.();});}
  };
  window.AudioContext=class {
   state='running';resume(){return Promise.resolve();}close(){this.state='closed';return Promise.resolve();}
   createMediaStreamSource(){return{connect(){}};}
   createAnalyser(){return{frequencyBinCount:256,fftSize:512,getByteFrequencyData(a){a.fill(window.meterEnergy);},getFloatTimeDomainData(a){a.fill(0);}};}
  };
 });
 await page.goto(new URL('../dist/index.html',import.meta.url).href);
 await page.waitForFunction(()=>window.islandReceive);
 const receive=async state=>{await page.evaluate(s=>window.islandReceive(s),state);};
 const shot=async name=>{await page.waitForTimeout(250);await page.evaluate(()=>document.documentElement.style.backgroundColor=document.querySelector('.island').classList.contains('dark')?'#000':'#f6f6f7');return page.screenshot({path:fileURLToPath(new URL(name+'.png',output))});};
 await receive({expanded:true,compactFaded:false,dark:true,page:'Home',outputVolume:100});
 for(const dark of [true,false]) for(const volume of [0,1,50,99,100]) {
  await receive({dark,outputVolume:volume});
  await page.waitForFunction(v=>document.querySelector('.volume-range input')?.value===String(v),volume);
  await page.waitForTimeout(280);
  const sizes=await page.locator('.volume-range').evaluate(el=>({width:el.clientWidth,filled:el.querySelector('.split-range-filled').getBoundingClientRect().width,remaining:el.querySelector('.split-range-remaining').getBoundingClientRect().width}));
  if(volume===0)assert.equal(sizes.filled,0,'empty fill collapses');
  if(volume===100)assert.equal(sizes.remaining,0,'full fill leaves no end bubble');
  if(volume===1)assert.ok(sizes.filled<11,'no padding-sized bubble at low end');
  if(volume===99)assert.ok(sizes.remaining<11,'no padding-sized bubble at high end');
  report.sliderCases++;
  if(volume===100)await shot('volume-'+(dark?'dark':'light'));
 }
 // Only artwork changes across these layouts; exercise both missing and valid art.
 const artwork='data:image/svg+xml,'+encodeURIComponent('<svg xmlns="http://www.w3.org/2000/svg" width="128" height="128"><rect width="128" height="128" fill="#687ba1"/><circle cx="64" cy="64" r="34" fill="#bdcadc"/><circle cx="64" cy="64" r="12" fill="#687ba1"/></svg>');
 for(const dark of [true,false]) for(const style of ['segments','halo','mosaic']) for(const art of [null,artwork]) {
  await receive({page:'Home',dark,mediaCardStyle:style,media:{title:'A quiet afternoon',artist:'Studio session',album:'Evening light',artwork:art,artworkRevision:art?1:0,playing:false,duration_ms:180000,position_ms:42000,can_seek:true,can_play:true}});
  await page.waitForFunction(({style,art})=>document.querySelector('.home-primary')?.classList.contains('media-card-'+style+'-mode')&&!!document.querySelector('.media-art img')===!!art,{style,art});
  const face=await page.locator('.media-art').evaluate(el=>({background:getComputedStyle(el).backgroundColor,border:getComputedStyle(el).borderWidth,shadow:getComputedStyle(el).boxShadow,scroll:el.scrollWidth,width:el.clientWidth}));
  assert.equal(face.background,'rgba(0, 0, 0, 0)');assert.equal(face.border,'0px');assert.equal(face.shadow,'none');
  if(!art)assert.equal(await page.getByLabel('No album artwork').count(),1);
  if(!art)await shot('media-'+style+'-'+(dark?'dark':'light')+'-empty');
  if(art&&style==='segments')await shot('media-segments-'+(dark?'dark':'light')+'-art');
  report.artCases++;
 }
 await receive({page:'Controls',media:null,activity:'Microphone · Browser, Pluto\nCamera · Video call',microphone:true,camera:true});
 assert.equal(await page.locator('.privacy-activity-item').count(),2);
 for(const dark of [true,false]){await receive({dark});await shot('controls-'+(dark?'dark':'light'));}
 // Selected provider starts preparing on Task, before opening its Options tab.
 await receive({page:'Agents',agents:{codex:agent,opencode:agent}});
 await page.locator('.agent-workspace').waitFor();
 assert.match(await page.locator('.agent-config-chip').first().innerText(),/gpt-6-sol/);
 await page.waitForFunction(()=>window.messages.some(m=>m.kind==='agentAction'&&m.action==='prepare'));
 const preparations=()=>page.evaluate(()=>window.messages.filter(m=>m.kind==='agentAction'&&m.action==='prepare').length);
 assert.equal(await preparations(),1);
 const prepared={...agent,lastCreatedId:'draft',selectedId:'draft',sessions:[{id:'draft',surface:'agent',title:'New task',directory:'C:\\project',ready:true,active:false,configOptions:options}]};
 await receive({agents:{codex:prepared,opencode:agent}});
 await page.getByRole('tab',{name:'Options',exact:true}).click();
 await page.locator('.agent-option-row').first().waitFor();
 await page.getByRole('tab',{name:'Task',exact:true}).click();
 await page.getByRole('button',{name:'New task',exact:true}).click();
 await receive({agents:{codex:{...prepared,selectedId:''},opencode:agent}});
 const opened=performance.now();
 await page.getByRole('tab',{name:'Options',exact:true}).click();
 await page.locator('.agent-option-row').first().waitFor();
 report.provider.cachedOptionsMs=Math.round(performance.now()-opened);
 assert.equal(await preparations(),1,'new draft uses cached catalog without another session/new');
 assert.equal(await page.getByText('Loading provider choices…',{exact:true}).count(),0);
 await page.locator('.agent-option-row').first().click();
 await page.getByRole('option',{name:'GPT-6 Astra',exact:false}).click();
 await page.getByRole('tab',{name:'Task',exact:true}).click();
 assert.match(await page.locator('.agent-config-chip').first().innerText(),/Astra/);
 await receive({page:'Home'});await page.locator('.home-layout').waitFor();
 await receive({page:'Agents'});await page.locator('.agent-config-chip').first().waitFor();
 assert.match(await page.locator('.agent-config-chip').first().innerText(),/Astra/);
 assert.equal(await preparations(),1,'tab/remount does not recreate the provider draft');
 report.provider.prepareRequests=await preparations();
 // Native preferred-height notifications settle once, retaining their final target.
 await page.evaluate(()=>window.messages=[]);
 await page.locator('.agent-workspace').evaluate(async el=>{for(let i=0;i<8;i++){el.dataset.layoutHeight=String(330+i*10);await new Promise(r=>setTimeout(r,5));}});
 await page.waitForFunction(()=>window.messages.some(m=>m.kind==='layout'));
 await page.waitForTimeout(120);
 const layouts=await page.evaluate(()=>window.messages.filter(m=>m.kind==='layout'));
 assert.equal(layouts.length,1,'one settled target for a burst of preferred-height changes');
 report.layoutTargets=layouts.length;
 // Ask's actual hold-capture panel must be mounted, visible and responsive to audio.
 await receive({page:'Ask',speechSettings:{configured:true},agents:{codex:agent,opencode:agent}});
 await page.locator('.ask-hold').waitFor();
 await page.keyboard.press('Escape');
 await page.locator('.ask-hold').focus();
 const cdp=await context.newCDPSession(page);await cdp.send('Performance.enable');
 const metric=async()=>{const {metrics}=await cdp.send('Performance.getMetrics');return Object.fromEntries(metrics.map(m=>[m.name,m.value]));};
 const idleStart=await metric();await page.waitForTimeout(400);const idleEnd=await metric();
 await page.keyboard.down('Space');
 await page.locator('.ask-capture-surface[data-capture-state="recording"]').waitFor();
 await page.waitForFunction(()=>getComputedStyle(document.querySelector('.ask-capture-slot')).opacity==='1');
 const idleLevels=await page.locator('.ask-voice-wave i').evaluateAll(es=>es.map(e=>getComputedStyle(e).transform));
 await page.evaluate(()=>window.meterEnergy=55);
 await page.waitForTimeout(250);
 const activeEnd=await metric();
 report.performance={idleTaskMs:Math.round((idleEnd.TaskDuration-idleStart.TaskDuration)*1000),captureTaskMs:Math.round((activeEnd.TaskDuration-idleEnd.TaskDuration)*1000),captureHeapMb:Number((activeEnd.JSHeapUsedSize/1048576).toFixed(1))};
 const activeLevels=await page.locator('.ask-voice-wave i').evaluateAll(es=>es.map(e=>getComputedStyle(e).transform));
 assert.notDeepEqual(activeLevels,idleLevels,'indicator follows measured audio');
 for(const dark of [true,false]){await receive({dark});await shot('ask-listening-'+(dark?'dark':'light'));}
 await page.keyboard.up('Space');
 await page.locator('.ask-capture-surface[data-capture-state="processing"]').waitFor();
 await page.waitForFunction(()=>window.messages.some(m=>m.kind==='speechTranscribe'));
 report.ask.captureVisible=await page.locator('.ask-capture-surface').isVisible();
 await receive({shellSettings:{reducedMotion:true}});
 assert.equal(await page.locator('.ask-voice-processing i').first().evaluate(el=>getComputedStyle(el).animationName),'none');
 await page.evaluate(()=>{const m=window.messages.findLast(m=>m.kind==='speechTranscribe');window.islandSpeechResult({requestId:m.requestId,text:'Explain the weather'});});
 await page.waitForFunction(()=>!document.querySelector('.ask-capture-target .ask-capture-surface'));
 assert.equal(await page.locator('.ask-capture-slot .ask-capture-status').innerText(),'Thinking…');
 assert.equal(await preparations(),0,'Ask never prepares modeling options');
 report.ask.submitted=await page.evaluate(()=>window.messages.some(m=>m.kind==='agentAction'&&m.action==='start'&&m.text==='Explain the weather'));
 assert.ok(report.ask.submitted);
 await context.close();
 // Compact transitions: rain is balanced, privacy never overlaps, reduced motion settles immediately.
 for(const dark of [true,false]) for(const privacy of [0,2]) for(const reduced of [false,true]){
  const c=await browser.newContext({viewport:{width:205+(privacy?53:0),height:28}});const p=await c.newPage();p.on('pageerror',e=>report.errors.push(e.message));
  await p.addInitScript(()=>{window.ipc={postMessage(){}};});
  await p.goto(new URL('../dist/index.html',import.meta.url).href);await p.waitForFunction(()=>window.islandReceive);
  await p.evaluate(({dark,privacy,reduced})=>{const n=new Date();n.setMinutes(0,0,0);const time=n.getFullYear()+'-'+String(n.getMonth()+1).padStart(2,'0')+'-'+String(n.getDate()).padStart(2,'0')+'T'+String(n.getHours()).padStart(2,'0')+':00';window.islandReceive({expanded:false,dark,compactFaded:false,media:null,microphone:!!privacy,camera:!!privacy,shellSettings:{reducedMotion:reduced},weatherData:{hourly:{time:[time],weather_code:[61],precipitation_probability:[100],precipitation:[1]}}});},{dark,privacy,reduced});
  await p.locator('.compact-swap-current .compact-weather-chance').waitFor();
  await p.waitForTimeout(300);
  const bounds=await p.locator('.compact-swap-current').evaluate(el=>{const r=e=>{const b=e.getBoundingClientRect();return{left:b.left,right:b.right};};return{content:r(el.querySelector('.compact-content')),chance:r(el.querySelector('.compact-weather-chance')),mark:r(el.querySelector('.compact-weather-mark')),privacy:document.querySelector('.compact-privacy button')?r(document.querySelector('.compact-privacy button')):null};});
  assert.ok((bounds.privacy?bounds.privacy.left-8:bounds.content.right)-bounds.chance.right<14,'rain probability fills the trailing space');
  assert.ok(bounds.chance.left>bounds.mark.right+25,'rain has separate leading and trailing content');
  if(bounds.privacy)assert.ok(bounds.chance.right+6<=bounds.privacy.left,'weather respects privacy lane');
  const color=await p.locator('.compact-weather-copy strong').last().evaluate(el=>getComputedStyle(el).color);
  assert.equal(color,dark?'rgb(245, 245, 247)':'rgb(23, 23, 25)','compact text uses the active theme');
  if(reduced)assert.equal(await p.locator('.compact-swap-current').evaluate(el=>getComputedStyle(el).animationName),'none');
  await p.evaluate(()=>document.documentElement.style.backgroundColor=document.querySelector('.island').classList.contains('dark')?'#000':'#f6f6f7');
  await p.screenshot({path:fileURLToPath(new URL('compact-'+(dark?'dark':'light')+'-'+privacy+'-'+reduced+'.png',output))});
  await p.evaluate(()=>window.islandReceive({media:{playing:true,title:'Evening light',artist:'Studio'}}));
  await p.locator('.compact-swap-current .compact-media-name').waitFor();
  await p.waitForFunction(()=>!document.querySelector('.compact-swap-old'),{},{timeout:3000}).catch(async error=>{console.log(JSON.stringify({dark,privacy,reduced,dom:await p.locator('.compact-transition').innerHTML(),hidden:await p.evaluate(()=>document.hidden)}));throw error;});
  await p.evaluate(()=>window.islandReceive({expanded:true,page:'Home'}));await p.setViewportSize({width:680,height:440});
  await p.locator('.home-layout').waitFor();
  await p.evaluate(()=>window.islandReceive({expanded:false,media:null}));await p.setViewportSize({width:205+(privacy?53:0),height:28});
  await p.locator('.compact-swap-current .compact-weather-chance').waitFor();
  report.compactCases++;await c.close();
 }
 assert.deepEqual(report.errors,[]);
 await writeFile(new URL('report.json',output),JSON.stringify(report,null,2));console.log(JSON.stringify(report,null,2));
}finally{await browser.close();}
