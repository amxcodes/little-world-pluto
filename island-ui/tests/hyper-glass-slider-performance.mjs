import assert from 'node:assert/strict';
import {mkdir,writeFile} from 'node:fs/promises';
import {pathToFileURL} from 'node:url';
const {chromium}=await import(pathToFileURL(process.env.PLUTO_PLAYWRIGHT_PATH).href);
const browser=await chromium.launch({headless:true,executablePath:process.env.PLUTO_BROWSER_PATH});
const out=new URL('../../target/hyper-glass/polish/',import.meta.url);await mkdir(out,{recursive:true});
const results=[];
try{
 for(let trial=0;trial<3;trial++)for(const which of trial%2?['after','before']:['before','after']){
  const context=await browser.newContext({viewport:{width:800,height:650}}),p=await context.newPage();
  await p.addInitScript(()=>{localStorage.setItem('pluto.appearance.mode.v1',JSON.stringify('hyper-glass'));window.messages=[];window.ipc={postMessage(raw){const m=JSON.parse(raw);window.messages.push(m);if(m.kind==='outputVolume')queueMicrotask(()=>window.islandReceive?.({outputVolume:m.value}));}};});
  await p.goto(which==='before'?new URL('before.html',out).href:new URL('../dist/index.html',import.meta.url).href);await p.waitForFunction(()=>window.islandHyperGlassDiagnostics);
  await p.evaluate(()=>{document.documentElement.style.background='#000';window.islandReceive({expanded:true,dark:true,page:'Home',outputVolume:50,outputMuted:false});});await p.waitForTimeout(600);
  const cdp=await context.newCDPSession(p);await cdp.send('Performance.enable');
  const metrics=async()=>Object.fromEntries((await cdp.send('Performance.getMetrics')).metrics.map(m=>[m.name,m.value]));
  const slider=p.locator('.volume-range'),box=await slider.boundingBox();
  await slider.evaluate(e=>{window.rangeReads=0;const read=e.getBoundingClientRect.bind(e);e.getBoundingClientRect=()=>{window.rangeReads++;return read();};});
  await p.mouse.move(box.x+box.width/2,box.y+box.height/2);const before=await metrics();await p.mouse.down();
  const gaps=await slider.evaluate(async(e,b)=>{
   const gaps=[];let last=performance.now();
   // Four samples per 60Hz frame represent a high-polling-rate pointer. Keep
   // the native range semantics and the same workload in both versions.
   for(let frame=0;frame<80;frame++){
    for(let sample=0;sample<4;sample++){
     const fraction=.5+.49*Math.sin((frame*4+sample)/32);
     e.dispatchEvent(new PointerEvent('pointermove',{bubbles:true,pointerId:1,isPrimary:true,buttons:1,clientX:b.x+b.width*fraction,clientY:b.y+b.height/2}));
    }
    await new Promise(r=>requestAnimationFrame(r));const now=performance.now();gaps.push(now-last);last=now;
   }return gaps.sort((a,b)=>a-b);
  },box);
  await p.mouse.move(box.x+box.width+3,box.y+box.height/2);await p.mouse.up();
  await p.waitForFunction(()=>{const d=window.islandHyperGlassDiagnostics();return !d.active&&!d.pendingFrame;});await p.waitForTimeout(80);
  const after=await metrics(),diag=await p.evaluate(()=>window.islandHyperGlassDiagnostics());
  assert.equal(await slider.locator('input').inputValue(),'100');
  results.push({which,trial,boundsReads:await p.evaluate(()=>window.rangeReads),layoutCount:after.LayoutCount-before.LayoutCount,layoutMs:(after.LayoutDuration-before.LayoutDuration)*1000,styleMs:(after.RecalcStyleDuration-before.RecalcStyleDuration)*1000,rendererTaskMs:(after.TaskDuration-before.TaskDuration)*1000,frameP95:gaps[Math.floor(gaps.length*.95)],maps:diag.maps,maxUpdateMs:diag.maxUpdateMs});
  const idleFrames=diag.frames;await p.waitForTimeout(200);assert.equal((await p.evaluate(()=>window.islandHyperGlassDiagnostics())).frames,idleFrames);await context.close();
 }
 await writeFile(new URL('slider-performance.json',out),JSON.stringify(results,null,2));
 const summary=['before','after'].map(which=>{const cases=results.filter(r=>r.which===which);return{which,...Object.fromEntries(['boundsReads','layoutCount','layoutMs','styleMs','rendererTaskMs','frameP95','maps','maxUpdateMs'].map(k=>[k,cases.reduce((sum,r)=>sum+r[k],0)/cases.length]))};});
 await writeFile(new URL('slider-performance-summary.json',out),JSON.stringify(summary,null,2));console.log(JSON.stringify(summary,null,2));
}finally{await browser.close();}
