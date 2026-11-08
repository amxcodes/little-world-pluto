// Local browser regression: saved scene replay, conversation routing and voice/audio UI.
import assert from 'node:assert/strict';
import http from 'node:http';
import { readFile, mkdir, writeFile } from 'node:fs/promises';
import { fileURLToPath, pathToFileURL } from 'node:url';
const { chromium }=await import(pathToFileURL(process.env.PLUTO_PLAYWRIGHT_PATH).href);
const dist=new URL('../dist/',import.meta.url),out=new URL('../../target/ui-review/followup/',import.meta.url);
await mkdir(out,{recursive:true});
const server=http.createServer(async(req,res)=>{try{const file=req.url==='/figure-worker.js'?'figure-worker.js':'index.html';res.setHeader('Content-Type',file.endsWith('.js')?'text/javascript':'text/html');res.end(await readFile(new URL(file,dist)));}catch{res.statusCode=404;res.end();}});
await new Promise(r=>server.listen(0,'127.0.0.1',r));const url=`http://127.0.0.1:${server.address().port}`;
const browser=await chromium.launch({headless:true,executablePath:process.env.PLUTO_BROWSER_PATH,args:['--enable-unsafe-swiftshader']});
const report={errors:[],providers:[],savedParts:0,mute:false,thinking:false,delete:false};
try{
 const presentation=JSON.parse(await readFile(new URL('../../app/test-fixtures/session114-presentation.json',import.meta.url),'utf8'));
 for(const provider of ['codex','opencode']){
  const context=await browser.newContext({viewport:{width:800,height:790}}),page=await context.newPage();page.on('pageerror',e=>report.errors.push(e.message));
  await page.route(/^https?:/,route=>route.request().url().startsWith(url)?route.continue():route.abort());
  await page.addInitScript(({provider,hyper})=>{
   if(hyper)localStorage.setItem('pluto.appearance.mode.v1','"hyper-glass"');
   localStorage.setItem('island.agent-preferences',JSON.stringify({defaultProvider:provider,providerSetupComplete:true}));
   window.islandDetachedViewer=true;window.messages=[];window.ipc={postMessage(raw){window.messages.push(JSON.parse(raw));}};
   const track={stop(){},addEventListener(){}};Object.defineProperty(navigator,'mediaDevices',{value:{getUserMedia:async()=>({getTracks:()=>[track],getAudioTracks:()=>[track]})}});
   window.MediaRecorder=class{state='inactive';static isTypeSupported(){return true;}start(){this.state='recording';queueMicrotask(()=>this.onstart?.());}stop(){this.state='inactive';queueMicrotask(()=>{this.ondataavailable?.({data:new Blob(['sample'])});this.onstop?.();});}};
   window.AudioContext=class{state='running';resume(){return Promise.resolve();}close(){return Promise.resolve();}createMediaStreamSource(){return{connect(){}};}createAnalyser(){return{frequencyBinCount:256,getByteFrequencyData(a){a.fill(40);},getFloatTimeDomainData(a){a.fill(0);}};}};
  },{provider,hyper:process.env.PLUTO_TEST_HYPER==='1'});
  await page.goto(url);await page.waitForFunction(()=>window.islandReceive);
  const receive=async s=>{await page.evaluate(s=>window.islandReceive(s),s);await page.evaluate(()=>new Promise(r=>requestAnimationFrame(r)));};
  const shot=async name=>{await page.waitForTimeout(300);await page.evaluate(()=>document.documentElement.style.backgroundColor=document.querySelector('.island').classList.contains('dark')?'#000':'#f6f6f7');await page.screenshot({path:fileURLToPath(new URL(provider+'-'+name+'.png',out))});};
  const task={id:'f16',surface:'agent',title:'F-16 jet visualization',directory:'C:\\project',ready:true,active:false,configOptions:[]};
  const question={id:'question',surface:'ask',title:'Ask conversation',ready:true,active:false};
  const wake={id:'wake',surface:'wake',title:'Wake conversation',ready:true,active:false};
  const base={connected:true,selectedId:'f16',askSelectedId:'question',wakeSelectedId:'wake',operationRevision:0,sessions:[task,question,wake],response:{sessionId:'f16',text:'Saved aircraft.',presentation},askResponse:{sessionId:'question',text:'ASK_ONLY_RESULT'},wakeResponse:{sessionId:'wake',text:'WAKE_ONLY_RESULT'}};
  const local=[{id:'114',provider,surface:'agent',runtimeId:'f16',title:'F-16 jet visualization'},{id:'ask-local',provider,surface:'ask',runtimeId:'question',title:'Ask conversation'},{id:'wake-local',provider,surface:'wake',runtimeId:'wake',title:'Wake conversation'}];
  await receive({page:'Agents',expanded:true,dark:true,speechSettings:{configured:true},agents:{[provider]:base},agentSessions:local,outputVolume:64,outputMuted:false});
  await page.locator('.agent-procedural-canvas').waitFor();
  await page.waitForFunction(()=>!document.querySelector('.agent-scene-build-status'));
  await page.waitForFunction(()=>document.querySelector('.agent-procedural-canvas')?.width>10);
  const canvas=page.locator('.agent-procedural-canvas');report.savedParts=presentation.elements[0].parts.length;
  assert.match(await canvas.getAttribute('aria-label'),new RegExp(report.savedParts+' labeled parts'));
  assert.equal(await page.locator('.code-block').count(),0,'saved 3D result is not shown as JSON');
  await shot('saved-f16');
  assert.ok(await canvas.evaluate(el=>el.toDataURL().length)>2000,'WebGL draws the saved assembly');
  // Local history delete never replaces the icon with overflowing words.
  await page.getByRole('tab',{name:'Sessions',exact:true}).click();
  assert.equal(await page.locator('.agent-session-row').count(),1,'Agents excludes Ask and wake conversations');
  const remove=page.getByRole('button',{name:'Remove F-16 jet visualization',exact:true});await remove.click();
  const confirm=page.getByRole('button',{name:'Confirm removal of F-16 jet visualization',exact:true});
  assert.equal(await confirm.innerText(),'');assert.ok((await confirm.getAttribute('class')).includes('is-delete-armed'));
  assert.equal(await page.evaluate(()=>window.messages.filter(m=>m.kind==='agentSessionDelete').length),0);
  await shot('delete-confirm');await confirm.click();assert.equal(await page.evaluate(()=>window.messages.filter(m=>m.kind==='agentSessionDelete').length),1);report.delete=true;
  // Real mute operation uses the endpoint's mute flag and preserves its scalar level.
  await receive({page:'Home',outputVolume:64,outputMuted:false});await page.getByRole('button',{name:'Mute output',exact:true}).click();
  await page.waitForFunction(()=>document.querySelector('.volume-range input')?.value==='0');
  assert.equal(await page.evaluate(()=>window.messages.findLast(m=>m.kind==='outputMute').muted),true);
  await page.waitForTimeout(80);const moving=await page.locator('.volume-range .split-range-filled').evaluate(el=>el.getBoundingClientRect().width);assert.ok(moving>0,'mute animates toward zero');
  await page.waitForTimeout(300);assert.equal(await page.locator('.volume-range .split-range-filled').evaluate(el=>el.getBoundingClientRect().width),0);
  await page.getByRole('button',{name:'Unmute output',exact:true}).click();await page.waitForFunction(()=>document.querySelector('.volume-range input')?.value==='64');
  assert.equal(await page.evaluate(()=>window.messages.findLast(m=>m.kind==='outputMute').muted),false);report.mute=true;
  // Surface selection and replies stay separate on each provider.
  await receive({page:'Ask',agents:{[provider]:base}});await page.getByText('ASK_ONLY_RESULT',{exact:true}).waitFor();
  assert.equal(await page.getByText('WAKE_ONLY_RESULT',{exact:true}).count(),0);
  await page.getByRole('button',{name:'Session',exact:true}).click();
  assert.equal(await page.getByRole('button',{name:'Wake conversation',exact:true}).count(),0);
  await page.getByRole('button',{name:'Ask conversation',exact:true}).click();
  assert.ok(await page.evaluate(()=>window.messages.some(m=>m.kind==='agentAction'&&m.action==='select'&&m.surface==='ask'&&m.sessionId==='question')));
  await page.locator('.ask-hold').focus();await page.keyboard.down('Space');await page.locator('.ask-capture-surface[data-capture-state="recording"]').waitFor();await page.waitForTimeout(250);await page.keyboard.up('Space');
  await page.waitForFunction(()=>window.messages.some(m=>m.kind==='speechTranscribe'));
  await page.evaluate(()=>{const m=window.messages.findLast(m=>m.kind==='speechTranscribe');window.islandSpeechResult({requestId:m.requestId,text:'Explain this'});});
  await page.waitForFunction(()=>window.messages.some(m=>m.kind==='agentAction'&&m.action==='prompt'&&m.text==='Explain this'));
  assert.equal(await page.evaluate(()=>window.messages.findLast(m=>m.kind==='agentAction'&&m.action==='prompt').sessionId),'question');
  const thinking={...base,operationRevision:1,operationKind:'prompt',operationOk:true,sessions:[task,{...question,active:true},wake],askResponse:{sessionId:'question',text:'Partial answer',waiting:true}};
  await receive({agents:{[provider]:thinking}});await page.getByText('Thinking…',{exact:true}).waitFor();
  assert.equal(await page.locator('.is-ask').getAttribute('data-capturing'),'true');
  assert.equal(await page.locator('.voice-answer-text').getAttribute('aria-hidden'),'true');await shot('thinking');
  const answer='A completed answer explaining the mechanism and its consequences.\n'.repeat(65);
  const complete={...base,operationRevision:2,operationKind:'prompt',operationOk:true,notice:'Task finished',askResponse:{sessionId:'question',text:answer,waiting:false}};
  for(const dark of [true,false]){
   await receive({dark,agents:{[provider]:complete}});await page.waitForFunction(()=>document.querySelector('.is-ask')?.dataset.capturing==='false');
   await page.locator('.voice-answer-text').evaluate(el=>el.scrollTop=40);
   const bounds=await page.locator('.voice-answer-clip').evaluate(el=>{const b=el.getBoundingClientRect(),t=el.querySelector('.voice-answer-text'),r=t.getBoundingClientRect();return{top:b.top,bottom:b.bottom,textTop:r.top,textBottom:r.bottom,scroll:t.scrollTop};});
   assert.ok(bounds.textTop>=bounds.top&&bounds.textBottom<=bounds.bottom+.1&&bounds.scroll>0,'long answer scrollbar stays inside its rounded shell');
   await shot('answer-'+(dark?'dark':'light'));
  }report.thinking=true;
  await receive({page:'Agents',wakeWord:{surface:true,status:'ready'},agents:{[provider]:base}});
  await page.getByText('WAKE_ONLY_RESULT',{exact:true}).waitFor();assert.equal(await page.getByText('ASK_ONLY_RESULT',{exact:true}).count(),0);
  await page.getByRole('button',{name:'Session',exact:true}).click();assert.equal(await page.getByRole('button',{name:'Ask conversation',exact:true}).count(),0);
  await page.getByRole('button',{name:'Wake conversation',exact:true}).click();assert.ok(await page.evaluate(()=>window.messages.some(m=>m.kind==='agentAction'&&m.action==='select'&&m.surface==='wake'&&m.sessionId==='wake')));
  report.providers.push(provider);await context.close();
 }
 assert.deepEqual(report.errors,[]);await writeFile(new URL('report.json',out),JSON.stringify(report,null,2));console.log(JSON.stringify(report,null,2));
}finally{await browser.close();await new Promise(r=>server.close(r));}
