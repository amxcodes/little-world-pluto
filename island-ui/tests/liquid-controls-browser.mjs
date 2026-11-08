// Visual coverage for the action families missed by the original glass selector list.
// IPC, providers and audio are mocked. This never records audio or invokes OS actions.
import assert from 'node:assert/strict';
import {mkdir,writeFile} from 'node:fs/promises';
import {pathToFileURL,fileURLToPath} from 'node:url';
const {chromium}=await import(pathToFileURL(process.env.PLUTO_PLAYWRIGHT_PATH).href);
const browser=await chromium.launch({headless:true,executablePath:process.env.PLUTO_BROWSER_PATH});
const out=new URL('../../target/ui-review/liquid-controls/',import.meta.url);await mkdir(out,{recursive:true});
const report={contexts:0,actionChecks:0,screenshots:[],errors:[]};
const cases=[...([380,680].flatMap(width=>[false,true].flatMap(glass=>[false,true].map(dark=>({width,glass,dark,scale:1.25}))))),...[1,2].map(scale=>({width:680,glass:true,dark:true,scale}))];
try {
 for(const {width,glass,dark,scale} of cases){
  const c=await browser.newContext({viewport:{width,height:560},deviceScaleFactor:scale}),p=await c.newPage();
  p.on('pageerror',e=>report.errors.push(e.message));await p.route(/^https?:/,route=>route.abort());
  await p.addInitScript(glass=>{
   localStorage.setItem('pluto.material.v1',JSON.stringify({enabled:glass,motion:false}));
   localStorage.setItem('island.agent-preferences',JSON.stringify({defaultProvider:'codex',providerSetupComplete:true}));
   window.messages=[];window.ipc={postMessage(raw){const m=JSON.parse(raw);window.messages.push(m);if(m.kind==='page')window.islandReceive?.({page:m.page});}};
   const track={stop(){},addEventListener(){}};
   Object.defineProperty(navigator,'mediaDevices',{value:{getUserMedia:async()=>({getTracks:()=>[track],getAudioTracks:()=>[track]})}});
   window.MediaRecorder=class{state='inactive';static isTypeSupported(){return true;}start(){this.state='recording';queueMicrotask(()=>this.onstart?.());}stop(){this.state='inactive';queueMicrotask(()=>{this.ondataavailable?.({data:new Blob(['mock audio'])});this.onstop?.();});}};
   window.AudioContext=undefined;
  },glass);
  await p.goto(new URL('../dist/index.html',import.meta.url).href);await p.waitForFunction(()=>window.islandReceive);
  const receive=async s=>{await p.evaluate(s=>window.islandReceive(s),s);await p.evaluate(()=>new Promise(r=>requestAnimationFrame(r)));};
  const shot=async name=>{if(!glass||scale!==1.25)return;await p.waitForTimeout(200);const file=`${name}-${width}-${dark?'dark':'light'}.png`;await p.screenshot({path:fileURLToPath(new URL(file,out))});report.screenshots.push(file);};
  const action=async selector=>{
   const controls=p.locator(selector);assert.ok(await controls.count(),`Missing audit target: ${selector}`);
   for(const el of await controls.all()){
    const s=await el.evaluate(e=>{const s=getComputedStyle(e);return{lens:s.backgroundImage.includes('radial-gradient'),shadow:s.boxShadow,background:s.backgroundImage};});
    assert.equal(s.lens,glass,`${selector}: expected ${glass?'glass lens':'normal material'}; ${JSON.stringify(s)}`);
    if(glass)assert.notEqual(s.shadow,'none',`${selector}: shared lens rim`);report.actionChecks++;
   }
  };
  const agent={connected:true,sessions:[],selectedId:'',capabilities:{sessionCapabilities:{additionalDirectories:true}},askResponse:{text:'A concise answer.'}};
  await receive({expanded:true,dark,page:'Agents',agents:{codex:agent,opencode:{connected:true,sessions:[]}},speechSettings:{configured:true},textExpander:{enabled:true,entries:[]}});
  await p.evaluate(dark=>document.documentElement.style.background=dark?'#000':'#f6f6f7',dark);
  await action('.agent-attach,.agent-dictate,.agent-submit');
  await p.locator('.agent-composer textarea').fill('A local draft');await action('.agent-submit');
  await p.getByRole('button',{name:'Dictate into task',exact:true}).click();await p.getByText('Listening',{exact:true}).waitFor();
  await action('.agent-voice-top > button,.agent-voice-bottom > button');
  const circle=await p.getByRole('button',{name:'Close dictation',exact:true}).evaluate(e=>{const r=e.getBoundingClientRect(),s=getComputedStyle(e),icon=e.querySelector('svg').getBoundingClientRect();return{width:r.width,height:r.height,radius:s.borderRadius,dx:icon.x+icon.width/2-r.x-r.width/2,dy:icon.y+icon.height/2-r.y-r.height/2,hit:getComputedStyle(e,'::after').left,extended:document.elementFromPoint(r.x-3,r.y+r.height/2)===e};});
  assert.ok(Math.abs(circle.width-32)<.01);assert.ok(Math.abs(circle.height-32)<.01);assert.equal(circle.radius,'50%');assert.ok(Math.abs(circle.dx)<1&&Math.abs(circle.dy)<1);assert.equal(circle.hit,'-4px');assert.ok(circle.extended,'40px hit target must belong to close button');
  await shot('dictation');await p.getByRole('button',{name:'Close dictation',exact:true}).click();await p.locator('.agent-composer').waitFor();
  assert.equal(await p.evaluate(()=>window.messages.some(m=>m.kind==='speechTranscribe')),false,'closing discards mocked audio');
  await p.getByRole('tab',{name:'Options',exact:true}).click();await action('.agent-prepare,.agent-extra-folders > button');
  await p.getByRole('tab',{name:'Task',exact:true}).click();
  const running={...agent,selectedId:'demo',sessions:[{id:'demo',surface:'agent',title:'Local preview',active:true,ready:true,configOptions:[{id:'model',name:'Model',type:'select',currentValue:'gpt-6-sol',options:[{value:'gpt-6-sol',name:'6 Sol'}]}]}],permissions:[{id:'approval',ownerSessionId:'demo',sessionId:'demo',action:'Example approval',options:[{optionId:'once',kind:'allow_once',name:'Allow once'},{optionId:'always',kind:'allow_always',name:'Always allow'},{optionId:'deny',kind:'reject_once',name:'Decline'}]}]};
  await receive({agents:{codex:running,opencode:{connected:true,sessions:[]}}});await action('.agent-stop,.agent-approval-actions button');
  const stop=await p.locator('.agent-stop').boundingBox();assert.ok(Math.abs(stop.width-stop.height)<.01,'task stop remains circular');
  if(glass){const colors=await p.locator('.agent-approval-actions').evaluate(e=>({allow:getComputedStyle(e.querySelector('.allow')).color,deny:getComputedStyle(e.querySelector('.deny')).color,persistent:getComputedStyle(e.querySelector('.allow-persistent')).borderColor,neutral:getComputedStyle(e.querySelector('.deny')).borderColor}));assert.notEqual(colors.allow,colors.deny);assert.notEqual(colors.persistent,colors.neutral);}
  if(await p.locator('.agent-visualize-toggle').count()){await p.locator('.agent-visualize-toggle').click();await action('.agent-visualize-toggle[aria-pressed="true"]');}
  await receive({page:'Expand'});await action('.expand-save');await p.getByRole('textbox',{name:'Shortcut',exact:true}).fill('!audit');await p.getByRole('textbox',{name:'Text to insert',exact:true}).fill('Local visual audit.');await action('.expand-save');
  const primary=await p.locator('.expand-save').evaluate(e=>({color:getComputedStyle(e).color,ink:getComputedStyle(e.closest('.island')).getPropertyValue('--accent-ink').trim()}));
  assert.equal(primary.color,await p.evaluate(ink=>{const e=document.createElement('span');e.style.color=ink;document.body.append(e);const color=getComputedStyle(e).color;e.remove();return color;},primary.ink));
  if(glass)assert.equal(await p.locator('.expand-save').evaluate(e=>getComputedStyle(e).borderRadius),'999px');
  await p.locator('.expand-save').hover();await action('.expand-save');await p.mouse.move(1,1);await shot('expand');
  await p.getByRole('button',{name:'Save shortcut',exact:true}).click();assert.ok(await p.evaluate(()=>window.messages.some(m=>m.kind==='textExpanderAdd'&&m.trigger==='!audit')));
  await p.getByRole('tab',{name:/Saved/}).click();await action('.expand-empty > button');
  await receive({page:'Text'});await action('.tool-grid > button');
  await receive({page:'Note',noteFolders:['Notes'],noteFolderSelected:0});await action('.folder-chip,.add-folder,.folder-manage,.note-tool,.note-quick-reminder > button,.reminder-pill');await shot('notes');
  await p.getByRole('tab',{name:'Reminders',exact:true}).click();await action('.utility-primary');
  await receive({page:'Weather'});await action('.wx-search > button');
  await receive({page:'Ask'});await p.getByRole('button',{name:'Context',exact:true}).click();await action('.ask-context-page footer button');await p.getByRole('button',{name:'Back to Ask',exact:true}).click();
  await receive({page:'Home'});const quick=p.getByRole('region',{name:'Quick actions',exact:true});
  await quick.getByRole('button',{name:'Record',exact:true}).click();await action('.record-quality-options > button');await p.locator('.record-quality-options > button').first().click();await action('.record-quality-options > button[aria-pressed="true"]');
  await p.getByRole('navigation',{name:'Island pages'}).getByRole('button',{name:'Home',exact:true}).click();await quick.getByRole('button',{name:'Dictate',exact:true}).click();await action('.dictate-record');
  await p.getByRole('button',{name:'Start dictation',exact:true}).click();await p.locator('.dictate-record-split').waitFor();
  const split=await p.locator('.dictate-record-split').evaluate(e=>({lens:getComputedStyle(e).backgroundImage.includes('radial-gradient'),children:[...e.querySelectorAll('button')].map(b=>({image:getComputedStyle(b).backgroundImage,left:getComputedStyle(b).borderTopLeftRadius,right:getComputedStyle(b).borderTopRightRadius}))}));
  assert.equal(split.lens,glass);if(glass){assert.ok(split.children.every(b=>b.image==='none'),'joined control has one lens');assert.equal(split.children[0].right,'0px');assert.equal(split.children[1].left,'0px');}
  await p.getByRole('button',{name:'Cancel dictation',exact:true}).click();assert.equal(await p.evaluate(()=>window.messages.some(m=>m.kind==='speechTranscribe')),false);
  await p.evaluate(()=>window.islandOpenSettings());await p.getByRole('navigation',{name:'Settings sections'}).getByRole('button',{name:'Agents',exact:true}).click();
  await p.locator('.agent-settings-menu').getByRole('button',{name:/Defaults/}).click();await action('.agent-default-folder > button');await p.getByRole('button',{name:'Back to Agent settings',exact:true}).click();
  await p.getByRole('navigation',{name:'Settings sections'}).getByRole('button',{name:'Dictation',exact:true}).click();await action('.settings-custom-endpoint,.settings-save');
  await p.getByRole('navigation',{name:'Settings sections'}).getByRole('button',{name:'Meetings',exact:true}).click();await action('.meeting-manager-actions button');
  if(glass){await p.emulateMedia({reducedMotion:'reduce'});const s=await p.locator('.meeting-manager-actions button').first().evaluate(e=>getComputedStyle(e).transitionDuration);assert.equal(s,'0s');}
  report.contexts++;await c.close();
 }
 assert.deepEqual(report.errors,[]);await writeFile(new URL('report.json',out),JSON.stringify(report,null,2));console.log(JSON.stringify(report));
}finally{await browser.close();}
