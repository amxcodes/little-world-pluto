// Compare the saved previous build with the current one in the same browser.
import assert from 'node:assert/strict';
import {mkdir,writeFile} from 'node:fs/promises';
import {pathToFileURL} from 'node:url';
const {chromium}=await import(pathToFileURL(process.env.PLUTO_PLAYWRIGHT_PATH).href);
const browser=await chromium.launch({headless:true,executablePath:process.env.PLUTO_BROWSER_PATH});
const output=new URL('../../target/hyper-glass/refinement/',import.meta.url);
await mkdir(output,{recursive:true});
const results=[];
try{
 for(let trial=0;trial<3;trial++)for(const which of trial%2?['after','before']:['before','after']){
  const context=await browser.newContext({viewport:{width:800,height:650}}),p=await context.newPage();
  await p.addInitScript(()=>{localStorage.setItem('pluto.appearance.mode.v1',JSON.stringify('hyper-glass'));window.ipc={postMessage(raw){const m=JSON.parse(raw);if(m.kind==='page')window.islandReceive?.({page:m.page});}};});
  await p.goto(which==='before'?new URL('before.html',output).href:new URL('../dist/index.html',import.meta.url).href);
  await p.waitForFunction(()=>window.islandHyperGlassDiagnostics);
  await p.evaluate(()=>window.islandReceive({expanded:true,dark:true,page:'Home',outputVolume:60,outputMuted:false}));
  await p.waitForTimeout(700);
  const cdp=await context.newCDPSession(p);await cdp.send('Performance.enable');
  const metrics=async()=>Object.fromEntries((await cdp.send('Performance.getMetrics')).metrics.map(m=>[m.name,m.value]));
  const before=await metrics();
  await p.evaluate(()=>{window.perfGaps=[];let last=performance.now();window.perfLoop=true;const loop=now=>{window.perfGaps.push(now-last);last=now;if(window.perfLoop)requestAnimationFrame(loop);};requestAnimationFrame(loop);});
  const start=performance.now();
  for(let round=0;round<4;round++){
   await p.evaluate(()=>window.islandReceive({page:'Home'}));await p.waitForTimeout(180);
   const button=p.getByRole('button',{name:'Island settings',exact:true}),box=await button.boundingBox();
   await p.mouse.move(box.x+box.width/2,box.y+box.height/2);await p.mouse.down();
   await p.mouse.move(box.x+box.width/2+10,box.y+box.height/2+5,{steps:8});await p.waitForTimeout(120);
   await p.mouse.move(799,649);await p.mouse.up();
   for(const page of ['Text','Note','Home']){await p.getByRole('navigation',{name:'Island pages'}).getByRole('button',{name:page,exact:true}).click();await p.waitForTimeout(140);}
  }
  await p.waitForFunction(()=>{const d=window.islandHyperGlassDiagnostics();return !d.active&&!d.pendingFrame&&!d.pendingMeasure;});
  const elapsed=performance.now()-start,after=await metrics();
  const gaps=await p.evaluate(()=>{window.perfLoop=false;return window.perfGaps.sort((a,b)=>a-b);});
  const diag=await p.evaluate(()=>window.islandHyperGlassDiagnostics());
  const resting=diag.frames;await p.waitForTimeout(250);assert.equal((await p.evaluate(()=>window.islandHyperGlassDiagnostics())).frames,resting,'owned scheduler is idle');
  const rendererTaskMs=(after.TaskDuration-before.TaskDuration)*1000;
  results.push({which,trial,elapsed,layoutCount:after.LayoutCount-before.LayoutCount,layoutMs:(after.LayoutDuration-before.LayoutDuration)*1000,styleCount:after.RecalcStyleCount-before.RecalcStyleCount,styleMs:(after.RecalcStyleDuration-before.RecalcStyleDuration)*1000,rendererTaskMs,rendererMsPerAnimationFrame:rendererTaskMs/Math.max(1,diag.frames),rendererTaskPercent:rendererTaskMs/elapsed*100,heapMiB:after.JSHeapUsedSize/1048576,frameP95:gaps[Math.floor(gaps.length*.95)],...diag});
  await context.close();
 }
 await writeFile(new URL('performance.json',output),JSON.stringify(results,null,2));
 const summary=['before','after'].map(which=>{
  const cases=results.filter(r=>r.which===which),keys=['elapsed','frames','layoutCount','layoutMs','styleCount','styleMs','rendererTaskMs','rendererMsPerAnimationFrame','rendererTaskPercent','frameP95','maxBakeMs','maxUpdateMs','heapMiB'];
  return {which,...Object.fromEntries(keys.map(k=>[k,cases.reduce((s,r)=>s+r[k],0)/cases.length]))};
 });
 console.log(JSON.stringify(summary,null,2));
}finally{await browser.close();}
