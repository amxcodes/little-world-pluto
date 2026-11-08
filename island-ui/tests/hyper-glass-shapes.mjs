import assert from 'node:assert/strict';
import {mkdir,writeFile} from 'node:fs/promises';
import {pathToFileURL,fileURLToPath} from 'node:url';
const {chromium}=await import(pathToFileURL(process.env.PLUTO_PLAYWRIGHT_PATH).href);
const browser=await chromium.launch({headless:true,executablePath:process.env.PLUTO_BROWSER_PATH});
const out=new URL('../../target/ui-review/hyper-glass/shapes/',import.meta.url);await mkdir(out,{recursive:true});
const report=[];
try{
 for(const {dark,width,scale} of [true,false].flatMap(dark=>[{dark,width:680,scale:1},{dark,width:380,scale:1.25},{dark,width:680,scale:2}])){
  const context=await browser.newContext({viewport:{width,height:620},deviceScaleFactor:scale}),p=await context.newPage(),errors=[];p.on('pageerror',e=>errors.push(e.message));await p.route(/^https?:/,r=>r.abort());
  await p.addInitScript(()=>{localStorage.setItem('pluto.appearance.mode.v1',JSON.stringify('hyper-glass'));localStorage.setItem('island.agent-preferences',JSON.stringify({defaultProvider:'codex',providerSetupComplete:true}));window.ipc={postMessage(raw){const m=JSON.parse(raw);if(m.kind==='page')window.islandReceive?.({page:m.page});}};});
  await p.goto(new URL('../dist/index.html',import.meta.url).href);await p.waitForFunction(()=>window.islandHyperGlassDiagnostics);
  const receive=async state=>{await p.evaluate(state=>window.islandReceive(state),state);await p.waitForTimeout(350);};
  const shot=async name=>p.screenshot({path:fileURLToPath(new URL(`${name}-${dark?'dark':'light'}-${width}-${scale}.png`,out))});
  await receive({expanded:true,dark,page:'Home',outputVolume:60,outputMuted:false,startup:{enabled:true,canChange:true},radios:{wifi:true,bluetooth:false}});
  await p.evaluate(dark=>{document.documentElement.style.background=dark?'#000':'#f6f6f7';},dark);
  const font=await p.locator('.volume-range .split-range-face strong').first().evaluate(e=>({weight:getComputedStyle(e).fontWeight,family:getComputedStyle(e).fontFamily,parent:getComputedStyle(e.parentElement).fontFamily}));assert.equal(font.weight,'550');assert.equal(font.family,font.parent);
  const checkMute=async()=>assert.equal(await p.locator('.volume-mute').evaluate(e=>getComputedStyle(e).color),await p.locator('.island').evaluate(e=>getComputedStyle(e).color),'mute uses neutral foreground');
  await checkMute();await receive({outputMuted:true});await p.locator('.volume-mute').hover();await p.waitForTimeout(250);await checkMute();await shot('mute');
  const hoverMorph=async(selector,icon)=>{
   await p.mouse.move(0,0);await p.waitForTimeout(300);const el=p.locator(selector).first();
   const sample=()=>el.evaluate(e=>({image:getComputedStyle(e).backgroundImage,color:getComputedStyle(e).backgroundColor,opacity:Number(getComputedStyle(e,'::before').opacity)}));
   const iconBefore=await el.locator(icon).evaluate(e=>getComputedStyle(e).backgroundImage);
   const before=await sample();await el.hover();await p.waitForTimeout(55);const middle=await sample();await p.waitForTimeout(230);const after=await sample();
   assert.equal(before.image,after.image,'hover retains its image layer');assert.ok(middle.opacity>before.opacity&&middle.opacity<after.opacity,'highlight fades through intermediate frames');
   assert.ok(after.opacity>=.59);assert.equal(await el.locator(icon).evaluate(e=>getComputedStyle(e).translate),'0px -1px');assert.equal(await el.locator(icon).evaluate(e=>getComputedStyle(e).backgroundImage),iconBefore,'icon highlight also retains its image layer');
   return {before,middle,after};
  };
  const home=await hoverMorph('.actions[aria-label="Quick actions"] > button','svg');await shot('home-hover');
  await receive({page:'Controls'});const controls=await hoverMorph('.controls-page button.control-tile:not(.control-active)','.control-icon');await shot('control-hover');
  await p.emulateMedia({reducedMotion:'reduce'});assert.equal(await p.locator('.control-icon').first().evaluate(e=>getComputedStyle(e).transitionDuration),'0s');assert.equal(await p.locator('.control-icon').first().evaluate(e=>getComputedStyle(e).translate),'none');await p.emulateMedia({reducedMotion:'no-preference'});
  await receive({radioBusy:true});const disabled=p.locator('.controls-page button.control-tile:not(.control-active)').first();assert.ok(await disabled.isDisabled());await disabled.hover();await p.waitForTimeout(250);assert.equal(await disabled.evaluate(e=>getComputedStyle(e,'::before').opacity),'0');assert.equal(await disabled.locator('.control-icon').evaluate(e=>getComputedStyle(e).translate),'none');await receive({radioBusy:false});
  await receive({page:'Expand'});const tabs=p.getByRole('tablist',{name:'Text expander'});assert.equal(await tabs.locator('button').first().evaluate(e=>getComputedStyle(e).borderRadius),'999px');await tabs.getByRole('tab',{name:/Saved/}).click();await p.waitForTimeout(300);
  const lens=await tabs.locator('.hyper-selection-lens').evaluate(e=>({radius:parseFloat(getComputedStyle(e).borderRadius),height:e.getBoundingClientRect().height}));assert.ok(lens.radius>=lens.height/2);await shot('expand-tabs');
  await p.evaluate(()=>window.islandOpenSettings());await p.waitForTimeout(400);
  const pressInside=async(el,name)=>{
   await el.scrollIntoViewIfNeeded();await p.waitForTimeout(250);const b=await el.boundingBox();await p.mouse.move(b.x+b.width/2,b.y+b.height/2);await p.mouse.down();await p.waitForTimeout(260);
   const bounds=await el.evaluate(e=>{const r=e.getBoundingClientRect(),clips=[];for(let a=e.parentElement;a;a=a.parentElement){const s=getComputedStyle(a),b=a.getBoundingClientRect();if(/auto|scroll|hidden|clip/.test(s.overflowX))clips.push({axis:'x',min:b.left,max:b.right});if(/auto|scroll|hidden|clip/.test(s.overflowY))clips.push({axis:'y',min:b.top,max:b.bottom});}return {left:r.left,right:r.right,top:r.top,bottom:r.bottom,clips};});
   for(const c of bounds.clips){assert.ok((c.axis==='x'?bounds.left:bounds.top)>=c.min-.5,`${name} leading edge stays visible`);assert.ok((c.axis==='x'?bounds.right:bounds.bottom)<=c.max+.5,`${name} trailing edge stays visible`);}
   await shot(name);await p.mouse.move(0,0);await p.mouse.up();await p.waitForTimeout(400);
  };
  const nav=p.getByRole('navigation',{name:'Settings sections'});
  for(const label of ['General','Status','Priority','Meetings'])await pressInside(nav.getByRole('button',{name:label,exact:true}),`settings-${label.toLowerCase()}`);
  await nav.getByRole('button',{name:'Agents',exact:true}).click();await p.locator('.agent-settings-menu').getByRole('button',{name:/Presentation/}).click();await p.waitForTimeout(400);await pressInside(p.getByRole('button',{name:'Back to Agent settings',exact:true}),'settings-back');
  assert.deepEqual(errors,[]);report.push({dark,width,scale,home,controls,errors:0});await context.close();
 }
 await writeFile(new URL('report.json',out),JSON.stringify(report,null,2));console.log(`Passed shape, clipping, typography, mute and hover checks in ${report.length} contexts.`);
}finally{await browser.close();}
