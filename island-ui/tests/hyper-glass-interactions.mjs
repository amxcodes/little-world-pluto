// Exercise actual rendered pixels, not just successful SVG filter attachment.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {mkdir,writeFile} from 'node:fs/promises';
import {pathToFileURL,fileURLToPath} from 'node:url';
const runtime=pathToFileURL(process.env.PLUTO_PLAYWRIGHT_PATH);
const {chromium}=await import(runtime.href);
const sharp=createRequire(runtime)('sharp');
const browser=await chromium.launch({headless:true,executablePath:process.env.PLUTO_BROWSER_PATH});
const out=new URL('../../target/ui-review/hyper-glass/interactions/',import.meta.url);
await mkdir(out,{recursive:true});
const results=[];
try{
 for(const dark of [true,false])for(const scale of [1,1.25,2]){
  const context=await browser.newContext({viewport:{width:800,height:650},deviceScaleFactor:scale});
  const p=await context.newPage(),errors=[];p.on('pageerror',e=>errors.push(e.message));
  await p.route(/^https?:/,r=>r.abort());
  await p.addInitScript(()=>{
   localStorage.setItem('pluto.appearance.mode.v1',JSON.stringify('hyper-glass'));
   localStorage.setItem('island.agent-preferences',JSON.stringify({defaultProvider:'codex',providerSetupComplete:true}));
   window.messages=[];window.ipc={postMessage(raw){const m=JSON.parse(raw);window.messages.push(m);if(m.kind==='page')window.islandReceive?.({page:m.page});}};
  });
  await p.goto(new URL('../dist/index.html',import.meta.url).href);
  await p.waitForFunction(()=>window.islandHyperGlassDiagnostics);
  const receive=async state=>{await p.evaluate(state=>window.islandReceive(state),state);await p.waitForTimeout(300);};
  await receive({expanded:true,dark,page:'Home',outputVolume:60,outputMuted:false,agents:{codex:{connected:true,sessions:[]}},startup:{enabled:true,canChange:true}});
  await p.evaluate(dark=>document.documentElement.style.background=dark?'#000':'#f6f6f7',dark);
  const idle=()=>p.waitForFunction(()=>{const d=window.islandHyperGlassDiagnostics();return !d.active&&!d.pendingFrame&&!d.pendingMeasure;});
  await idle();
  const countInk=async(buffer,rect)=>{
   const x=Math.max(0,Math.floor(rect.x*scale)),y=Math.max(0,Math.floor(rect.y*scale));
   const {data,info}=await sharp(buffer).extract({left:x,top:y,width:Math.ceil(rect.width*scale),height:Math.ceil(rect.height*scale)}).removeAlpha().raw().toBuffer({resolveWithObject:true});
   let count=0;for(let i=0;i<data.length;i+=info.channels){const lum=(data[i]+data[i+1]+data[i+2])/3;if(dark?lum>145:lum<165)count++;}return count;
  };
  const hold=async(control,name)=>{
   await control.scrollIntoViewIfNeeded();await idle();await p.waitForTimeout(100);
   const svg=control.locator('svg').first(),box=await control.boundingBox(),icon=await svg.count()?await svg.boundingBox():null;
   const before=await p.screenshot();
   await p.mouse.move(box.x+box.width/2,box.y+box.height/2);await p.mouse.down();await p.waitForTimeout(250);
   const afterBox=await control.boundingBox();
   const physical=await control.getAttribute('data-hyper-physical')!==null;
   if(!physical)for(const axis of ['x','y','width','height'])assert.ok(Math.abs(box[axis]-afterBox[axis])<.15,`${name}: layout slot remains stable during press`);
   else{
    assert.ok(afterBox.width>box.width&&afterBox.width<box.width*1.3,`${name}: the control visibly swells`);
    assert.ok(Math.abs(box.x+box.width/2-afterBox.x-afterBox.width/2)<.2&&Math.abs(box.y+box.height/2-afterBox.y-afterBox.height/2)<.2,`${name}: press stays centred`);
   }
   const optical=await p.locator('[data-hyper-optic]').evaluateAll(es=>es.map(e=>({tag:e.tagName,filter:getComputedStyle(e).backdropFilter,position:getComputedStyle(e).position})));
   if(name==='volume-slider'){
    assert.equal(optical.length,0,'slider avoids a wide SVG backdrop pass');assert.ok(await control.locator('.split-range-face').getAttribute('data-hyper-physical')!==null,'slider keeps its physical response');
   }else assert.ok(optical.length&&optical.every(e=>e.filter.includes('url(')),`${name}: actual backdrop filter is active`);
   const held=await p.screenshot({path:fileURLToPath(new URL(`${name}-${dark?'dark':'light'}-${scale}.png`,out))});
   if(name==='home-tab'){
    const lens=await p.locator('.bar nav .hyper-raised-lens').boundingBox(),track=await p.locator('.bar nav').boundingBox();
    assert.ok(lens.x>=track.x-.5&&lens.y>=track.y-.5&&lens.x+lens.width<=track.x+track.width+.5&&lens.y+lens.height<=track.y+track.height+.5,'raised lens has room inside the scrolling navigation');
   }
   if(icon){
    const afterIcon=await svg.boundingBox();
    const beforeInk=await countInk(before,icon),afterInk=await countInk(held,afterIcon);
    if(beforeInk<=10*scale)await writeFile(new URL(`failed-rest-${name}-${dark}-${scale}.png`,out),before);
    assert.ok(beforeInk>10*scale,`${name}: foreground icon is painted at rest (${beforeInk})`);
    // The lifted lens changes local contrast and antialiasing. Require the
    // glyph to remain painted, alongside its independent attachment check.
    assert.ok(afterInk>10*scale&&afterInk>=beforeInk*.65,`${name}: foreground icon stays painted while held (${beforeInk}/${afterInk})`);
    const normalized=(rect,parent)=>[(rect.x+rect.width/2-parent.x)/parent.width,(rect.y+rect.height/2-parent.y)/parent.height];
    assert.ok(normalized(icon,box).every((v,i)=>Math.abs(v-normalized(afterIcon,afterBox)[i])<.012),`${name}: icon stays attached to its control`);
   }
   await p.mouse.move(799,649);await p.mouse.up();await idle();
   assert.equal(await p.locator('[data-hyper-held],[data-hyper-optic]').count(),0,`${name}: release clears interaction`);
   results.push({dark,scale,name,stable:true});
  };
  const nav=p.getByRole('navigation',{name:'Island pages'});
  const neighborBoxes=await nav.locator('button svg').evaluateAll(es=>es.map(e=>e.getBoundingClientRect().toJSON()));
  const navBefore=await p.screenshot();
  for(const box of neighborBoxes)assert.ok(await countInk(navBefore,box)>10*scale,'every navigation glyph is painted');
  await hold(nav.getByRole('button',{name:'Home',exact:true}),'home-tab');
  const navAfter=await p.screenshot();
  for(const box of neighborBoxes)assert.ok(await countInk(navAfter,box)>10*scale,'neighbor glyphs survive the lens release');
  assert.equal(await p.locator('.bar nav .hyper-raised-lens:not([hidden])').count(),0,'raised lens is removed from painting at rest');
  await hold(p.locator('.volume-range'),'volume-slider');
  for(const value of [0,5,20,50,80,95,100]){
   await receive({outputVolume:value});await p.waitForTimeout(80);
   const geometry=await p.locator('.volume-range').evaluate(e=>{const face=e.querySelector('.split-range-face'),fill=e.querySelector('.split-range-filled'),remaining=e.querySelector('.split-range-remaining');return{slot:e.getBoundingClientRect().toJSON(),face:face.getBoundingClientRect().toJSON(),fill:fill.getBoundingClientRect().toJSON(),remaining:remaining.getBoundingClientRect().toJSON(),inset:parseFloat(e.style.getPropertyValue('--split-edge-inset'))};});
   assert.ok(Math.abs(geometry.face.height-(geometry.slot.height-2*geometry.inset))<.2,`volume ${value}: backing follows the edge morph`);
   assert.ok(geometry.fill.y>=geometry.face.y&&geometry.remaining.y>=geometry.face.y,'split content stays inside the backing');
  }
  assert.equal(await p.locator('.bar').evaluate(e=>getComputedStyle(e).borderBottomWidth),'0px','navigation has no divider in the lens path');
  await hold(p.getByRole('button',{name:'Island settings',exact:true}),'settings-icon');
  await receive({page:'Agents'});
  await hold(p.getByRole('button',{name:'Attach files',exact:true}),'attachment');
  await p.getByRole('tab',{name:'Options',exact:true}).click();await p.waitForTimeout(500);await idle();
  await hold(p.getByRole('tab',{name:'Options',exact:true}),'agent-tab');
  await receive({page:'Convert'});await p.getByRole('button',{name:'Fit size',exact:true}).click();
  const conversionToggle=p.getByRole('switch',{name:'Allow smaller dimensions',exact:true});await conversionToggle.scrollIntoViewIfNeeded();await idle();
  const cb=await conversionToggle.boundingBox();await p.mouse.move(cb.x+cb.width/2,cb.y+cb.height/2);await p.mouse.down();await p.waitForTimeout(220);
  assert.ok(await conversionToggle.locator('i > b').evaluate(e=>new DOMMatrixReadOnly(getComputedStyle(e).transform).m11>1.5),'conversion switch lifts its thumb');
  assert.ok(Math.abs((await conversionToggle.boundingBox()).width-cb.width)<.1,'conversion switch does not inflate its entire settings row');
  await p.mouse.move(799,649);await p.mouse.up();await idle();
  const previousChecked=await conversionToggle.getAttribute('aria-checked');
  await conversionToggle.focus();await p.keyboard.down('Space');await p.waitForTimeout(140);await p.keyboard.up('Space');await idle();
  assert.notEqual(await conversionToggle.getAttribute('aria-checked'),previousChecked,'keyboard activation commits one toggle');results.push({dark,scale,name:'conversion-toggle',stable:true});
  await p.evaluate(()=>window.islandOpenSettings());await p.waitForTimeout(300);
  const toggle=p.getByRole('switch',{name:'Start with Windows',exact:true});await toggle.scrollIntoViewIfNeeded();
  const b=await toggle.boundingBox();await p.mouse.move(b.x+b.width/2,b.y+b.height/2);await p.mouse.down();await p.waitForTimeout(200);
  const lens=await toggle.locator('i').evaluate(e=>({position:getComputedStyle(e).position,scale:new DOMMatrixReadOnly(getComputedStyle(e).transform).m11,lift:Number(e.parentElement.style.getPropertyValue('--hyper-lift'))}));
  assert.equal(lens.position,'relative','toggle knob keeps its containing block');assert.ok(lens.lift>.9&&parseFloat(lens.scale)>1.5,'toggle lifts into a clear, expanded lens');
  await p.screenshot({path:fileURLToPath(new URL(`toggle-${dark?'dark':'light'}-${scale}.png`,out))});
  await p.mouse.move(799,649);await p.mouse.up();
  await p.mouse.move(b.x+b.width/2,b.y+b.height/2);await p.mouse.down();
  await p.evaluate(()=>window.dispatchEvent(new Event('blur')));
  assert.equal(await p.locator('[data-hyper-held],[data-hyper-optic]').count(),0,'focus loss cancels a held toggle');await p.mouse.up();
  await p.emulateMedia({reducedMotion:'reduce'});await holdWithoutOptics();
  async function holdWithoutOptics(){await p.mouse.move(b.x+b.width/2,b.y+b.height/2);await p.mouse.down();await p.waitForTimeout(100);assert.equal(await p.locator('[data-hyper-optic]').count(),0);await p.mouse.move(799,649);await p.mouse.up();}
  assert.deepEqual(errors,[]);await context.close();
 }
 await writeFile(new URL('report.json',out),JSON.stringify(results,null,2));console.log(`Passed ${results.length} held-control pixel/geometry cases across six theme/scale contexts.`);
}finally{await browser.close();}
