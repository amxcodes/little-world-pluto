import assert from 'node:assert/strict';
import {mkdir,writeFile} from 'node:fs/promises';
import {pathToFileURL,fileURLToPath} from 'node:url';
const {chromium}=await import(pathToFileURL(process.env.PLUTO_PLAYWRIGHT_PATH).href);
const browser=await chromium.launch({headless:true,executablePath:process.env.PLUTO_BROWSER_PATH});
const out=new URL('../../target/ui-review/navigation-models/',import.meta.url);await mkdir(out,{recursive:true});const report=[];
try{
 for(const mode of ['glass','hyper-glass'])for(const [width,scale] of [[380,1.25],[680,1],[680,2]]){
  const context=await browser.newContext({viewport:{width,height:620},deviceScaleFactor:scale}),p=await context.newPage(),errors=[];p.on('pageerror',e=>errors.push(e.message));await p.route(/^https?:/,r=>r.abort());
  await p.addInitScript(mode=>{localStorage.setItem('pluto.appearance.mode.v1',JSON.stringify(mode));localStorage.setItem('pluto.material.v1',JSON.stringify({enabled:true,motion:true}));localStorage.setItem('island.agent-preferences',JSON.stringify({defaultProvider:'opencode',providerSetupComplete:true}));window.messages=[];window.ipc={postMessage(raw){const m=JSON.parse(raw);window.messages.push(m);if(m.kind==='page')window.islandReceive?.({page:m.page});}};},mode);
  await p.goto(new URL('../dist/index.html',import.meta.url).href);await p.waitForFunction(()=>window.islandReceive);
  const receive=async state=>{await p.evaluate(state=>window.islandReceive(state),state);await p.waitForTimeout(300);};
  const shot=async name=>p.screenshot({path:fileURLToPath(new URL(`${name}-${mode}-${width}-${scale}.png`,out))});
  await receive({expanded:true,dark:false,page:'Home',outputVolume:62});await p.evaluate(()=>document.documentElement.style.background='#f6f6f7');
  const checkRims=async()=>{
   const shadows=await p.locator('.bar .tab-indicator,.bar .hyper-selection-lens,.bar .hyper-raised-lens').evaluateAll(es=>es.map(e=>getComputedStyle(e).boxShadow));assert.ok(shadows.length);
   for(const shadow of shadows){assert.notEqual(shadow,'none','glass still has a rim');assert.ok(shadow.split(/,(?![^()]*\))/).every(s=>s.includes('inset')),'navigation lens casts no shadow into its clipped scrollport');}
  };
  await checkRims();await shot('navigation-rest');
  const nav=p.getByRole('navigation',{name:'Island pages'});await nav.getByRole('button',{name:'Text',exact:true}).hover();await p.mouse.down();await p.waitForTimeout(220);await checkRims();await shot('navigation-held');await p.mouse.up();await p.waitForTimeout(300);await checkRims();
  const ids=['deepseek/deepseek-v4-pro','deepseek/deepseek-v3.2','qwen/qwen3-coder-next','z-ai/glm-5.3','anthropic/claude-sonnet-4-6','google/gemini-3.1-pro-preview','unknown-model','deepseek-r1-distill-qwen-8b'];
  const agentFor=model=>({connected:true,selectedId:'demo',sessions:[{id:'demo',surface:'agent',title:'Eligibility check',ready:true,active:false,configOptions:[{id:'model',name:'Model',type:'select',currentValue:model,options:ids.map(value=>({value,name:value}))}]}]});
  await receive({page:'Agents',agents:{opencode:agentFor(ids[0]),codex:{connected:true,sessions:[]}}});
  const toggle=p.getByRole('button',{name:'Toggle 3D visualization mode',exact:true});
  assert.equal(await toggle.getAttribute('aria-pressed'),'false','a newly qualified model remains opt-in');await toggle.click();assert.equal(await toggle.getAttribute('aria-pressed'),'true');
  for(const [index,id] of ids.entries()){
   await receive({agents:{opencode:agentFor(id),codex:{connected:true,sessions:[]}}});
   assert.equal(await toggle.count(),index<6?1:0,id);
   if(index>0&&index<6)assert.equal(await toggle.getAttribute('aria-pressed'),'false','changing models clears visualization opt-in');
  }
  await receive({agents:{opencode:agentFor(ids[0]),codex:{connected:true,sessions:[]}}});await shot('deepseek-eligible');
  assert.deepEqual(errors,[]);report.push({mode,width,scale,modelChoices:ids.length,errors:0});await context.close();
 }
 await writeFile(new URL('report.json',out),JSON.stringify(report,null,2));console.log(`Passed light navigation rim and live model gating checks in ${report.length} contexts.`);
}finally{await browser.close();}
