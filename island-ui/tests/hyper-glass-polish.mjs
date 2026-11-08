import assert from 'node:assert/strict';
import {mkdir,writeFile} from 'node:fs/promises';
import {pathToFileURL,fileURLToPath} from 'node:url';
const {chromium}=await import(pathToFileURL(process.env.PLUTO_PLAYWRIGHT_PATH).href);
const browser=await chromium.launch({headless:true,executablePath:process.env.PLUTO_BROWSER_PATH});
const out=new URL('../../target/ui-review/hyper-glass/polish/',import.meta.url);await mkdir(out,{recursive:true});
const report=[];
const choices=Array.from({length:90},(_,i)=>({value:`model-${i}`,name:`Provider / Model ${i}`,description:'Available model'}));
const configOptions=[{id:'model',name:'Model',type:'select',currentValue:'model-0',options:choices},{id:'provider',name:'Provider',type:'select',currentValue:'provider-a',options:[{value:'provider-a',name:'Provider A'},{value:'provider-b',name:'Provider B'}]}];
try{
 for(const {dark,width,scale} of [true,false].flatMap(dark=>[{dark,width:680,scale:1},{dark,width:380,scale:1.25},{dark,width:680,scale:2}])){
  const context=await browser.newContext({viewport:{width,height:620},deviceScaleFactor:scale}),p=await context.newPage(),errors=[];p.on('pageerror',e=>errors.push(e.message));await p.route(/^https?:/,r=>r.abort());
  await p.addInitScript(()=>{localStorage.setItem('pluto.appearance.mode.v1',JSON.stringify('hyper-glass'));localStorage.setItem('island.agent-preferences',JSON.stringify({defaultProvider:'codex',providerSetupComplete:true}));window.messages=[];window.ipc={postMessage(raw){const m=JSON.parse(raw);window.messages.push(m);if(m.kind==='page')window.islandReceive?.({page:m.page});if(m.kind==='outputVolume')queueMicrotask(()=>window.islandReceive?.({outputVolume:m.value}));}};});
  await p.goto(new URL('../dist/index.html',import.meta.url).href);await p.waitForFunction(()=>window.islandHyperGlassDiagnostics);
  const receive=async state=>{await p.evaluate(state=>window.islandReceive(state),state);await p.waitForTimeout(350);};
  const shot=async name=>p.screenshot({path:fileURLToPath(new URL(`${name}-${dark?'dark':'light'}-${width}-${scale}.png`,out))});
  await receive({expanded:true,dark,page:'Home',outputVolume:50,outputMuted:false,startup:{enabled:true,canChange:true}});
  await p.evaluate(dark=>{document.documentElement.style.background=dark?'#000':'#f6f6f7';const root=document.querySelector('.island');root.style.setProperty('--accent-a','#a8c4ff');root.style.setProperty('--accent-b','#648ef1');root.style.setProperty('--accent','#8eb0ff');root.style.setProperty('--accent-ink','#152038');},dark);
  const rootStyle=await p.locator('.island').evaluate(e=>({bg:getComputedStyle(e).backgroundColor,image:getComputedStyle(e).backgroundImage}));assert.equal(rootStyle.bg,'rgba(0, 0, 0, 0)');assert.equal(rootStyle.image,'none','the WebView does not paint a rectangular shell backdrop');
  assert.ok((await p.locator('.home-primary').evaluate(e=>getComputedStyle(e,'::before').backgroundImage)).split(',').every(v=>v.trim()==='none'),'reading plane has no low-contrast gradient banding');await shot('home');
  const slider=p.locator('.volume-range'),input=slider.locator('input');await input.focus();await p.keyboard.press('ArrowRight');await p.waitForTimeout(50);
  assert.notEqual(await slider.evaluate(e=>getComputedStyle(e).outlineStyle),'none','keyboard focus remains visible');
  const box=await slider.boundingBox();await slider.evaluate(e=>{window.sliderReads=0;const original=e.getBoundingClientRect.bind(e);e.getBoundingClientRect=()=>{window.sliderReads++;return original();};});
  await p.mouse.move(box.x+box.width*.3,box.y+box.height/2);await p.mouse.down();
  assert.equal(await slider.evaluate(e=>getComputedStyle(e).outlineStyle),'none','pointer adjustment never inherits the keyboard ring');
  const maps=(await p.evaluate(()=>window.islandHyperGlassDiagnostics())).maps;
  await slider.evaluate((e,b)=>{for(let i=0;i<120;i++)e.dispatchEvent(new PointerEvent('pointermove',{bubbles:true,pointerId:1,isPrimary:true,buttons:1,clientX:b.x+b.width*(i/119),clientY:b.y+b.height/2}));},box);
  await p.mouse.move(box.x+box.width+4,box.y+box.height/2);await p.mouse.up();await p.waitForTimeout(350);
  assert.equal(await input.inputValue(),'100','release commits the final value');
  assert.ok(await p.evaluate(()=>window.sliderReads)<=5,'drag does not read layout on every pointer event');
  assert.equal((await p.evaluate(()=>window.islandHyperGlassDiagnostics())).maps,maps,'slider drag creates no wide SVG refraction fields');
  assert.equal(await slider.evaluate(e=>getComputedStyle(e).outlineStyle),'none');await shot('slider');
  await p.mouse.move(box.x+box.width*.2,box.y+box.height/2);await p.mouse.down();
  await slider.evaluate((e,b)=>{e.dispatchEvent(new PointerEvent('pointermove',{bubbles:true,pointerId:1,isPrimary:true,buttons:1,clientX:b.x+b.width*.55,clientY:b.y+b.height/2}));window.dispatchEvent(new Event('blur'));},box);
  await p.mouse.up();await p.mouse.move(box.x+box.width*.8,box.y+box.height/2);await p.waitForTimeout(300);assert.equal(await input.inputValue(),'55','focus loss flushes the final sample and stops hover from continuing the drag');
  await p.mouse.move(box.x+box.width*.5,box.y+box.height/2);await p.mouse.down();await p.setViewportSize({width:width+80,height:620});await p.waitForTimeout(80);
  const resized=await slider.boundingBox();await p.mouse.move(resized.x+resized.width*.8,resized.y+resized.height/2);await p.mouse.up();await p.waitForTimeout(300);assert.equal(await input.inputValue(),'80','a mid-drag resize invalidates the cached bounds');await p.setViewportSize({width,height:620});await p.waitForTimeout(100);
  await p.getByRole('button',{name:'Command',exact:true}).click();await p.waitForTimeout(450);
  const checkRows=async selector=>{
   const rows=await p.locator(selector).evaluateAll(es=>es.slice(0,8).map(e=>{const r=e.getBoundingClientRect();return{top:r.top,bottom:r.bottom,shadow:getComputedStyle(e).boxShadow,color:getComputedStyle(e).color};}));
   assert.ok(rows.length>=2);for(let i=0;i<rows.length;i++){assert.equal(rows[i].shadow,'none');if(i)assert.ok(rows[i].top-rows[i-1].bottom>=5.5,'rows have real separation');}
   await p.locator(selector).nth(1).hover();
   const row=await p.locator(selector).nth(1).evaluate(e=>({color:getComputedStyle(e).color,text:getComputedStyle(e.querySelector('strong')).color,small:e.querySelector('small')?getComputedStyle(e.querySelector('small')).color:null,bg:getComputedStyle(e).backgroundColor}));
   assert.equal(row.color,row.text,'hover preserves foreground ink');assert.notEqual(row.small,row.bg,'secondary text remains distinct from the highlight');
  };
  await checkRows('.command-results > button');await shot('command');await p.keyboard.press('Escape');
  const agent={connected:true,selectedId:'test',sessions:[{id:'test',surface:'agent',title:'Styling check',ready:true,active:false,configOptions}]};
  await receive({page:'Agents',agents:{codex:agent,opencode:{connected:true,sessions:[]}}});await p.getByRole('tab',{name:'Options',exact:true}).click();
  await p.locator('.agent-option-row').filter({hasText:'Model'}).click();await p.waitForTimeout(350);await checkRows('.agent-option-browser-list > button');await shot('models');
  await p.getByRole('button',{name:'Back to task options',exact:true}).click();await p.getByRole('button',{name:'Provider Provider A',exact:true}).click();await p.waitForTimeout(250);await checkRows('.agent-option-browser-list > button');await shot('providers');
  await p.evaluate(()=>window.islandOpenSettings());await p.waitForTimeout(300);const toggle=p.getByRole('switch',{name:'Start with Windows',exact:true}),tb=await toggle.boundingBox();await p.mouse.move(tb.x+tb.width/2,tb.y+tb.height/2);await p.mouse.down();await p.waitForTimeout(240);
  const replay=await toggle.locator('i').evaluate(e=>({image:getComputedStyle(e,'::after').backgroundImage,scale:getComputedStyle(e,'::after').scale}));assert.ok(replay.image.includes('linear-gradient'));assert.equal(replay.scale,'none','track replay is a continuous strip, not a nested oval');await shot('toggle');await p.mouse.up();
  assert.deepEqual(errors,[]);report.push({dark,width,scale,sliderBoundsReads:await p.evaluate(()=>window.sliderReads),errors:0});await context.close();
 }
 await writeFile(new URL('report.json',out),JSON.stringify(report,null,2));console.log(`Passed Hyper Glass polish checks in ${report.length} contexts.`);
}finally{await browser.close();}
