import assert from 'node:assert/strict';
import {mkdir,writeFile} from 'node:fs/promises';
import {pathToFileURL,fileURLToPath} from 'node:url';
const {chromium}=await import(pathToFileURL(process.env.PLUTO_PLAYWRIGHT_PATH).href);
const browser=await chromium.launch({headless:true,executablePath:process.env.PLUTO_BROWSER_PATH});
const out=new URL('../../target/ui-review/settings-sources/',import.meta.url);await mkdir(out,{recursive:true});
const report={themes:0,settingsDetails:0,controls:0,errors:[]};
try{
 for(const glass of [false,true])for(const dark of [true,false]){
  const c=await browser.newContext({viewport:{width:680,height:448}}),p=await c.newPage();p.on('pageerror',e=>report.errors.push(e.message));
  await p.addInitScript(glass=>{localStorage.setItem('pluto.material.v1',JSON.stringify({enabled:glass,motion:false}));localStorage.setItem('island.agent-preferences',JSON.stringify({defaultProvider:'codex',providerSetupComplete:true}));window.messages=[];window.ipc={postMessage(raw){const m=JSON.parse(raw);window.messages.push(m);if(m.kind==='page')window.islandReceive?.({page:m.page});}};},glass);
  await p.goto(new URL('../dist/index.html',import.meta.url).href);await p.waitForFunction(()=>window.islandReceive);
  const receive=async s=>{await p.evaluate(s=>window.islandReceive(s),s);await p.evaluate(()=>new Promise(r=>requestAnimationFrame(r)));};
  const shot=async name=>{await p.waitForTimeout(220);await p.evaluate(dark=>document.documentElement.style.background=dark?'#000':'#f6f6f7',dark);await p.screenshot({path:fileURLToPath(new URL(`${name}-${glass?'glass':'normal'}-${dark?'dark':'light'}.png`,out))});};
  const agent={connected:true,selectedId:'',askSelectedId:'ask',sessions:[{id:'ask',surface:'ask',ready:true,title:'A previous question'}],askResponse:{sessionId:'ask',text:'A previous answer'}};
  await receive({dark,expanded:true,page:'Home',agents:{codex:agent,opencode:{connected:false,error:'Provider did not connect',missing:false}},opencode:{installError:'Archive verification failed. The existing installation was kept.',installedVersion:'2.2.6',latestVersion:'2.2.6',installing:false}});
  await p.evaluate(()=>window.islandOpenSettings());await p.getByRole('navigation',{name:'Settings sections'}).getByRole('button',{name:'Agents',exact:true}).click();
  for(const title of ['Providers','Defaults','MCP tools','Shared skills','Shared system prompt','Presentation']){
   await p.locator('.agent-settings-menu').getByRole('button',{name:new RegExp(title)}).click();
   const surface=await p.locator('.agent-settings.is-detail > .settings-card:not([hidden])').evaluate(el=>({shadow:getComputedStyle(el).boxShadow,border:getComputedStyle(el).borderTopWidth,filter:getComputedStyle(el).backdropFilter,heading:el.querySelector('.settings-card-heading')?.getBoundingClientRect().height||0}));
   assert.deepEqual(surface,{shadow:'none',border:'0px',filter:'none',heading:0},'detail panel has no duplicate divider/rim/title');
   if(title==='Providers'){
    assert.ok(await p.getByText('Archive verification failed. The existing installation was kept.',{exact:true}).isVisible());
    await p.getByRole('button',{name:'Repair',exact:true}).click();assert.ok(await p.evaluate(()=>window.messages.some(m=>m.kind==='agentAction'&&m.provider==='opencode'&&m.action==='install')));
    await shot('providers');
   }
   report.settingsDetails++;await p.getByRole('button',{name:'Back to Agent settings',exact:true}).click();
  }
  await p.getByRole('button',{name:'Back to app',exact:true}).click();await p.locator('.settings-page').waitFor({state:'detached'});
  // Media and Ask use the same neutral and selected row treatment.
  const sessions=[{id:'one',source:'Spotify',playing:true},{id:'two',source:'VLC',playing:true},{id:'three',source:'Chrome',playing:false}];
  await receive({page:'Home',media:{session_id:'one',title:'Studio session',source:'Spotify',playing:true,sessions,duration_ms:120000,position_ms:0}});
  await p.getByRole('button',{name:'Choose media source, 3 sessions',exact:true}).click();
  await p.locator('.media-session-panel').waitFor();
  const rowStyle=async selector=>p.locator(selector).first().evaluate(el=>{const s=getComputedStyle(el);return{radius:s.borderRadius,background:s.backgroundImage,border:s.borderWidth,padding:s.padding,minHeight:s.minHeight,shadow:s.boxShadow};});
  const mediaRow=await rowStyle('.media-session-option:not(.is-selected)');
  assert.equal(mediaRow.radius,'14px');assert.equal(mediaRow.background.includes('radial-gradient'),glass);
  await shot('media');await p.getByRole('option',{name:/VLC/}).click();assert.ok(await p.evaluate(()=>window.messages.some(m=>m.kind==='mediaSelect'&&m.id==='two')));
  await receive({page:'Ask',media:null,speechSettings:{configured:true}});await p.getByRole('button',{name:'Session',exact:true}).click();
  const askRow=await rowStyle('.ask-session-choice:first-child');assert.deepEqual(askRow,mediaRow,'media/Ask row styling matches in this material/theme');
  const back=await p.locator('.ask-session-page footer button').evaluate(el=>getComputedStyle(el).backgroundImage);assert.equal(back.includes('radial-gradient'),glass);await shot('ask-sessions');
  await p.getByRole('button',{name:'A previous question',exact:true}).click();assert.ok(await p.evaluate(()=>window.messages.some(m=>m.kind==='agentAction'&&m.action==='select'&&m.surface==='ask')));
  // A tight native viewport must still request room for inserted privacy rows and messages.
  for(const width of [680,380]){
   await p.setViewportSize({width,height:300});await receive({page:'Controls',activity:'',microphone:false,camera:false,radios:{wifi:true,bluetooth:true}});
   await p.waitForTimeout(220);const before=await p.evaluate(()=>window.messages.filter(m=>m.kind==='layout').at(-1).height);
   await receive({activity:'Microphone · Browser and Pluto\nCamera · Video call',microphone:true,camera:true});
   await p.waitForFunction(before=>window.messages.filter(m=>m.kind==='layout').at(-1).height>before,before);
   const added=await p.evaluate(()=>window.messages.filter(m=>m.kind==='layout').at(-1).height);assert.ok(added>before+35);
   await p.setViewportSize({width,height:added});await p.waitForTimeout(180);
   const power=await p.locator('.power-section').boundingBox();assert.ok(power.y+power.height<=added+.1,'all controls remain inside grown viewport');
   await receive({activity:'',microphone:false,camera:false});await p.waitForFunction(added=>window.messages.filter(m=>m.kind==='layout').at(-1).height<added,added);
   report.controls++;
  }
  report.themes++;await c.close();
 }
 assert.deepEqual(report.errors,[]);await writeFile(new URL('report.json',out),JSON.stringify(report,null,2));console.log(report);
}finally{await browser.close();}
