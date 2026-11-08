import { displacementField, fieldBitmap, springStep, pressShape, fitPress, rubberband } from './hyper-glass-math.js';

const groupSelector = '.bar > nav,.settings-segments,.settings-lane-tabs,.agent-tabs,.note-mode-switch,.meeting-view-tabs,.meeting-source-options,.signature-format,.record-quality-options,.appearance-mode-picker,.opencode-profile-sections';
const popupSelector = '.media-session-panel,.device-menu,.calendar-popover,.agent-mention-menu,.meeting-export-menu > div,.opencode-profile-select-menu';
const rowSelector='.command-results > button,.agent-option-browser-list > button,.agent-option-row,.agent-session-main,.agent-library-tile,.media-session-option,.ask-session-choice,.agent-turn-row,.device-menu > button,.meeting-session,.meeting-export-menu > div > button,.calendar-day,.opencode-profile-row,.opencode-profile-select-menu > button';
const hostSelector = '.home-primary,.home-strip,.shelf-panel,.settings-card,.settings-disclosure,.control-tile,.battery-tile,.clip-card,.media-session-panel,.device-menu,.calendar-popover,.meeting-export-menu > div,.convert-source,.convert-inspector,.convert-options,.agent-session-card,.agent-composer,.agent-voice-composer,.opencode-composer,.power-section,.wx-current,.wx-daily,.meeting-session-detail,.meetings-page .panel';
const selected = button => button.matches('.active,.selected,.is-selected,[aria-selected="true"],[aria-pressed="true"],[aria-current="page"]');
const switchSelector='.settings-switch,.convert-switch-row';
const thumb=owner=>owner.querySelector(owner.matches('.convert-switch-row')?'i > b':'i');
let nextRuntime = 0;

export function installHyperGlass(root, options) {
  const id = ++nextRuntime;
  const groups = new Map(), active = new Set(), touched = new Set(), surfaces = new Set();
  const motions=new Map(), moving=new Set(), pendingSurfaces=new Set(), hosts=new Set();
  const maps = new Map();
  let frame = 0, measuring = 0, last = 0, idle = 0, stopped = false, pressed = null, pointerId = null;
  let scrollTimer = 0, opticTarget = null, idleJob = null, warmIdle = 0;
  let gesture = null, suppress = null, suppressTimer = 0, press = null;
  const stats = { frames: 0, updates: 0, maps: 0, bakeMs: 0, maxBakeMs: 0, maxUpdateMs: 0, cacheBytes: 0, quality: options.quality, missed: 0, measurements:0, surfaceBatches:0, coldPresses:0, warmBakes:0, cacheHits:0 };
  const originalIndicator = root.querySelector('.tab-indicator')?.getAttribute('style');
  const originalIndicatorHeight = root.querySelector('.tab-indicator')?.style.height || '';
  const originalIndicatorRadius = root.querySelector('.tab-indicator')?.style.borderRadius || '';
  const svg = document.createElementNS('http://www.w3.org/2000/svg','svg');
  svg.setAttribute('aria-hidden','true'); svg.dataset.hyperOwned=''; svg.classList.add('hyper-defs');
  const defs=document.createElementNS(svg.namespaceURI,'defs');svg.append(defs);root.append(svg);
  const media = matchMedia('(prefers-reduced-motion: reduce)');
  const contrast = matchMedia('(prefers-reduced-transparency: reduce), (prefers-contrast: more), (forced-colors: active)');
  const reduced = () => !options.motion || media.matches || root.dataset.reducedMotion === 'true' || document.hidden;
  const diagnose = () => ({...stats,active:active.size+moving.size,pendingFrame:!!frame,pendingMeasure:!!measuring,cacheEntries:maps.size,groups:groups.size,motions:motions.size});
  const previousDiagnostics = window.islandHyperGlassDiagnostics;
  window.islandHyperGlassDiagnostics=diagnose;
  root.dataset.hyperRuntime='ready';root.dataset.hyperQuality=options.quality === 'efficient' ? 'efficient' : 'full';
  const node = (name,attrs) => {const n=document.createElementNS(svg.namespaceURI,name);for(const [k,v] of Object.entries(attrs))n.setAttribute(k,String(v));return n;};
  const clearOptic = () => {
    if(opticTarget){opticTarget.removeAttribute('data-hyper-optic');opticTarget.style.removeProperty('--hyper-optic-filter');if(!groups.has(opticTarget))touched.delete(opticTarget);opticTarget=null;}
  };
  const cancelBake = () => {if(idle){if(window.cancelIdleCallback)cancelIdleCallback(idle);else clearTimeout(idle);idle=0;}idleJob=null;};
  const bake = (target,attach=true) => {
    if(!target.isConnected || !root.contains(target) || options.depth<=0 || contrast.matches || root.dataset.hyperScrolling || root.dataset.hyperQuality==='efficient')return;
    const width=Math.max(1,target.offsetWidth||parseFloat(target.style.width)||1),height=Math.max(1,target.offsetHeight||parseFloat(target.style.height)||1);
    if(width>800||height>96)return;
    const computed=getComputedStyle(target),raw=parseFloat(computed.borderTopLeftRadius)||16;
    const radius=Math.min(width/2,height/2,raw);
    const key=`${width}:${height}:${Math.round(radius)}:${options.depth}`;
    let map=maps.get(key);
    if(!map){
      if(attach)stats.coldPresses++;else stats.warmBakes++;
      const started=performance.now(), w=Math.min(160,width),h=Math.min(48,height);
      const field=displacementField(width,height,radius,w,h,options.depth/100);
      const bytes=fieldBitmap(field.pixels,w,h);
      const binary=String.fromCharCode(...bytes);
      // Match the reference's Chromium path: pixel-sized primitives, with
      // overscan for the curved rim. Percentages resolve against the SVG
      // viewport (our defs viewport is zero), not the pressed control.
      const filter=node('filter',{id:`hyper-${id}-${stats.maps++}`,filterUnits:'objectBoundingBox',primitiveUnits:'userSpaceOnUse',x:'-.25',y:'-.25',width:'1.5',height:'1.5','color-interpolation-filters':'sRGB'});
      filter.dataset.hyperOwned='';
      const area={x:0,y:0,width,height,preserveAspectRatio:'none'};
      const bitmap=`data:image/bmp;base64,${btoa(binary)}`;
      filter.append(node('feImage',{href:bitmap,...area,result:'bevel'}),node('feDisplacementMap',{in:'SourceGraphic',in2:'bevel',scale:field.scale,xChannelSelector:'R',yChannelSelector:'G',result:'bent'}));
      // Clip the output to the rounded shape, including antialiased corners.
      const mask=`data:image/svg+xml;base64,${btoa(`<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}"><rect width="${width}" height="${height}" rx="${radius}" fill="white"/></svg>`)}`;
      filter.append(node('feImage',{href:mask,...area,result:'shape'}),node('feComposite',{in:'bent',in2:'shape',operator:'in'}));
      defs.append(filter);map={filter,bytes:w*h*4};maps.set(key,map);stats.cacheBytes+=map.bytes;
      // Small active controls only: at most 32 bounded maps, no page-sized bitmaps.
      while(maps.size>32 || stats.cacheBytes>16*1024*1024){const [old,entry]=maps.entries().next().value;entry.filter.remove();maps.delete(old);stats.cacheBytes-=entry.bytes;}
      const cost=performance.now()-started;stats.bakeMs+=cost;stats.maxBakeMs=Math.max(stats.maxBakeMs,cost);
      const image=new Image();image.src=bitmap;image.decode().catch(()=>{});
    }else{stats.cacheHits++;maps.delete(key);maps.set(key,map);}
    if(!attach)return;
    clearOptic();opticTarget=target;touched.add(target);
    target.style.setProperty('--hyper-optic-filter',`url(#${map.filter.id})`);target.dataset.hyperOptic='';
  };
  const requestOptic = target => {
    if(target&&(opticTarget===target||idleJob===target))return;
    cancelBake();if(reduced()||!target)return;
    idleJob=target;
    const run=()=>{idle=0;const job=idleJob;idleJob=null;if(!stopped&&job)bake(job);};
    idle=window.requestIdleCallback ? requestIdleCallback(run,{timeout:150}) : setTimeout(run,24);
  };
  const warm = event => {
    if(pressed||warmIdle||reduced()||options.quality==='efficient')return;
    const owner=event.target.closest('.split-range')||event.target.closest('button,summary');
    if(!owner||owner.disabled||owner.getAttribute('aria-disabled')==='true'||owner.dataset.disabled==='true')return;
    // A list shares its reading plane; a range already has an animated glass
    // face. Neither needs a wide SVG pass on each pointer/scroll interaction.
    if(owner.matches(`.split-range,${rowSelector}`))return;
    const group=groups.get(owner.closest(groupSelector));
    const target=group?.lens||(owner.matches(switchSelector)?thumb(owner):owner);
    if(!target)return;
    const run=()=>{warmIdle=0;if(!stopped&&!pressed)bake(target,false);};
    warmIdle=window.requestIdleCallback?requestIdleCallback(run,{timeout:180}):setTimeout(run,32);
  };
  const paint = record => {
    if(!record.lens.isConnected)return;
    const [x,y,w,h,lift]=record.values;
    const raised=Math.max(0,Math.min(1,lift));
    record.lens.hidden=raised<.002;
    record.select.style.transform=`translate3d(${x}px,${y}px,0) scale(${w/record.target[2]},${h/record.target[3]})`;
    record.select.style.opacity=String(1-raised);
    // Resting selection and raised lens are separate, as in LensTrack. The
    // full-sized lens scales about its centre; the real glyph row stays put.
    const W=record.target[2]+12,H=record.target[3]+8;
    const stretch=record.group.hasAttribute('data-hyper-drag')?Math.min(.08,Math.abs(record.velocity[0])/6000):0;
    record.lens.style.transform=`translate3d(${x+(w-W)/2}px,${y+(h-H)/2}px,0) scale(${(w+(W-w)*raised)/W*(1+stretch)},${(h+(H-h)*raised)/H*(1-stretch*.6)})`;
    record.lens.style.opacity=String(Math.min(1,raised*4));
  };
  const tick = now => {
    frame=0;const started=performance.now(),dt=Math.min(.04,Math.max(.001,(now-last)/1000));last=now;
    stats.frames++;if(dt>.026)stats.missed++;
    // Adaptive optics never suppress motion. Avoid using unrelated slow page
    // frames as a reason to permanently remove a held lens's refraction.
    for(const record of active){
      if(!record.group.isConnected){active.delete(record);continue;}
      let settled=true;
      for(let i=0;i<5;i++){
        [record.values[i],record.velocity[i]]=springStep(record.values[i],record.velocity[i],record.target[i],dt,i===4?24:28,i===4?.78:1);
        const epsilon=i===4?.002:.08;
        if(Math.abs(record.values[i]-record.target[i])>epsilon||Math.abs(record.velocity[i])>(i===4?.02:.3))settled=false;
      }
      if(settled||reduced()){
        record.values=[...record.target];record.velocity.fill(0);
        if(record.target[4]&&!pressed&&!reduced())record.target[4]=0;
        else{active.delete(record);record.group.removeAttribute('data-hyper-travelling');}
      }
      paint(record);
    }
    for(const motion of moving){
      if(!motion.element.isConnected){restoreMotion(motion);continue;}
      if(motion.releaseAt&&now>=motion.releaseAt){motion.releaseAt=0;releaseMotion(motion);}
      let settled=true;
      for(let i=0;i<5;i++){
        [motion.values[i],motion.velocity[i]]=springStep(motion.values[i],motion.velocity[i],motion.target[i],dt,motion.releasing?19:29,motion.releasing?.58:.86);
        if(Math.abs(motion.values[i]-motion.target[i])>(i<2?.03:.001)||Math.abs(motion.velocity[i])>(i<2?.15:.01))settled=false;
      }
      if(settled||reduced()){
        motion.values=[...motion.target];motion.velocity.fill(0);
        if(!motion.releaseAt)moving.delete(motion);
      }
      paintMotion(motion);
      if(!moving.has(motion)&&motion.releasing)restoreMotion(motion);
    }
    stats.updates++;stats.maxUpdateMs=Math.max(stats.maxUpdateMs,performance.now()-started);
    if(active.size||moving.size)frame=requestAnimationFrame(tick);else if(!pressed)clearOptic();
  };
  const wake = () => {if(!frame&&(active.size||moving.size)){last=performance.now();frame=requestAnimationFrame(tick);}};
  const measureGroup = group => {
    stats.measurements++;
    const buttons=[...group.children].filter(e=>e.tagName==='BUTTON'&&!e.disabled);
    const chosen=buttons.find(selected);
    if(!chosen||!group.offsetWidth){const r=groups.get(group);if(r){r.lens.hidden=r.select.hidden=true;active.delete(r);}return;}
    let record=groups.get(group);
    if(!record){
      const existing=group.matches('.bar > nav')?group.querySelector('.tab-indicator'):null;
      const select=existing||document.createElement('span');
      if(!existing){select.className='hyper-selection-lens';select.dataset.hyperOwned='';select.setAttribute('aria-hidden','true');group.append(select);}
      const lens=document.createElement('span');lens.className='hyper-raised-lens';lens.dataset.hyperOwned='';lens.setAttribute('aria-hidden','true');group.append(lens);
      group.dataset.hyperTrack='';touched.add(group);
      record={group,lens,select,values:[0,0,0,0,0],velocity:[0,0,0,0,0],target:[],placed:false,radius:16,positions:new Map(),neighbors:new Map()};groups.set(group,record);resize.observe(group);
    }
    record.select.hidden=false;
    const target=[chosen.offsetLeft,chosen.offsetTop,chosen.offsetWidth,chosen.offsetHeight,record.target[4]||0];
    const positions=buttons.map(button=>({button,x:button.offsetLeft,previous:record.positions.get(button),shift:record.neighbors.has(button)?new DOMMatrixReadOnly(getComputedStyle(button).transform).m41:0}));
    record.radius=Math.min(target[3]/2,parseFloat(getComputedStyle(chosen).borderTopLeftRadius)||16);
    if(record.placed&&record.target.every((v,i)=>Math.abs(v-target[i])<.1))return;
    record.target=target;
    for(const {button,x,previous,shift} of positions){
      record.positions.set(button,x);
      if(previous===undefined||reduced()||Math.abs(previous-x+shift)<.5)continue;
      const old=record.neighbors.get(button);if(old){old.cancel();animations.delete(old);}
      const animation=button.animate([{transform:`translate3d(${previous-x+shift}px,0,0)`},{transform:'translate3d(0,0,0)'}],{duration:280,easing:'cubic-bezier(.2,.8,.2,1)'});
      record.neighbors.set(button,animation);animations.add(animation);
      animation.onfinish=()=>{animations.delete(animation);if(record.neighbors.get(button)===animation)record.neighbors.delete(button);};
    }
    // Fixed geometry is set once per measurement; animation frames only
    // transform the layers. No width/height writes in the spring loop.
    record.select.style.width=`${target[2]}px`;record.select.style.height=`${target[3]}px`;record.select.style.borderRadius=`${record.radius}px`;
    record.lens.style.width=`${target[2]+12}px`;record.lens.style.height=`${target[3]+8}px`;record.lens.style.borderRadius=`${Math.min((target[3]+8)/2,record.radius+4)}px`;
    if(!record.placed||reduced()){record.values=[...target];record.velocity.fill(0);record.placed=true;paint(record);active.delete(record);}
    else{record.target[4]=1;active.add(record);group.dataset.hyperTravelling='';paint(record);requestOptic(record.lens);wake();}
  };
  const pendingGroups=new Set();
  const schedule = group => {
    if(group)pendingGroups.add(group);
    if(!measuring)measuring=requestAnimationFrame(()=>{
      measuring=0;
      // Read all new containing blocks before setting any style-affecting
      // attributes. The old per-button read/write loop forced style recalc.
      const prepare=[...pendingSurfaces].filter(e=>root.contains(e)&&!surfaces.has(e)&&!hosts.has(e)).map(element=>({element,anchor:getComputedStyle(element).position==='static',host:element.matches(hostSelector)&&!element.matches('button,summary')}));pendingSurfaces.clear();
      if(prepare.length)stats.surfaceBatches++;
      for(const {element,anchor,host} of prepare){if(anchor)element.dataset.hyperAnchor='';if(host){element.dataset.hyperHost='';hosts.add(element);}else{element.dataset.hyperSurface='';surfaces.add(element);}}
      for(const el of surfaces)if(!root.contains(el))surfaces.delete(el);
      for(const el of hosts)if(!root.contains(el))hosts.delete(el);
      for(const [el,r] of groups)if(!root.contains(el)){resize.unobserve(el);active.delete(r);r.lens.remove();if(r.select.dataset.hyperOwned!==undefined)r.select.remove();el.removeAttribute('data-hyper-track');el.removeAttribute('data-hyper-travelling');touched.delete(el);groups.delete(el);}
      for(const el of pendingGroups)if(root.contains(el))measureGroup(el);pendingGroups.clear();
    });
  };
  const resize=new ResizeObserver(entries=>{for(const {target} of entries)schedule(target);});
  const popup = element => {
    if(reduced()||!element.matches(popupSelector))return;
    // transform/opacity only; no animated blur, pixel snapshots or cloned content.
    const animation=element.animate([{opacity:0,transform:'translateY(-4px) scale(.97)'},{opacity:1,transform:'none'}],{duration:210,easing:'cubic-bezier(.2,.8,.2,1)'});
    animations.add(animation);animation.onfinish=()=>animations.delete(animation);
  };
  const animations=new Set();
  const prepareSurface = element => {
    if(element.matches(groupSelector)||element.parentElement?.matches(groupSelector)||element.matches(`input,.split-range,${switchSelector}`))return;
    pendingSurfaces.add(element);schedule();
  };
  const inspect = subtree => {
    if(subtree.matches('button,summary,.settings-switch i,.convert-switch-row > i > b,.split-range-divider'))prepareSurface(subtree);
    for(const element of subtree.querySelectorAll('button,summary,.settings-switch i,.convert-switch-row > i > b,.split-range-divider'))prepareSurface(element);
    if(subtree.matches(hostSelector)){pendingSurfaces.add(subtree);schedule();}
    for(const element of subtree.querySelectorAll(hostSelector)){pendingSurfaces.add(element);schedule();}
    if(subtree.matches(groupSelector))schedule(subtree);
    for(const group of subtree.querySelectorAll(groupSelector))schedule(group);
    popup(subtree);for(const el of subtree.querySelectorAll(popupSelector))popup(el);
  };
  const observer=new MutationObserver(mutations=>{
    for(const mutation of mutations){
      if(mutation.type==='attributes'){
        if(mutation.target===root){if(reduced())settle();continue;}
        const group=mutation.target.closest(groupSelector);if(group)schedule(group);
        if(mutation.target.matches(switchSelector))syncToggle(mutation.target);
      }else{
        for(const added of mutation.addedNodes)if(added.nodeType===1&&!added.hasAttribute('data-hyper-owned'))inspect(added);
        if([...mutation.removedNodes].some(n=>n.nodeType===1&&!n.hasAttribute('data-hyper-owned')))schedule();
      }
    }
  });
  observer.observe(root,{subtree:true,childList:true,attributes:true,attributeFilter:['aria-selected','aria-pressed','aria-checked','class','data-reduced-motion']});
  function makeMotion(element,owner,kind){
    let motion=motions.get(element);if(motion)return motion;
    const travel=owner.matches('.convert-switch-row')?13:16;
    const initial=kind==='toggle'&&owner.getAttribute('aria-checked')==='true'?travel:0;
    motion={element,owner,kind,travel,values:[initial,0,1,1,0],target:[initial,0,1,1,0],velocity:[0,0,0,0,0],releasing:false,releaseAt:0,original:Object.fromEntries(['scale','translate','transform'].map(name=>[name,element.style.getPropertyValue(name)]))};
    motions.set(element,motion);element.dataset.hyperPhysical='';return motion;
  }
  function paintMotion(motion){
    const [x,y,sx,sy,lift]=motion.values;
    // One compositable transform rather than separate scale/translate style
    // updates. The state and release curve are unchanged.
    motion.element.style.transform=`translate3d(${x}px,${y}px,0) scale(${sx},${sy})`;
    if(motion.kind==='toggle'){
      motion.owner.style.setProperty('--hyper-lift',String(Math.max(0,Math.min(1,lift))));
      motion.owner.style.setProperty('--hyper-knob-x',`${(motion.travel===13?8.5:10.5)+x}px`);
    }
  }
  function restoreMotion(motion){
    moving.delete(motion);motions.delete(motion.element);
    for(const [name,value] of Object.entries(motion.original)){if(value)motion.element.style.setProperty(name,value);else motion.element.style.removeProperty(name);}
    motion.element.removeAttribute('data-hyper-physical');
    if(motion.kind==='toggle'){motion.owner.style.removeProperty('--hyper-lift');motion.owner.style.removeProperty('--hyper-knob-x');}
  }
  function releaseMotion(motion){
    motion.releasing=true;motion.target=[motion.kind==='toggle'&&motion.owner.getAttribute('aria-checked')==='true'?motion.travel:0,0,1,1,0];moving.add(motion);wake();
  }
  function syncToggle(owner){
    if(reduced()||!owner.isConnected)return;
    const knob=thumb(owner);if(!knob)return;
    const motion=motions.get(knob);
    if(motion){motion.target[0]=owner.getAttribute('aria-checked')==='true'?motion.travel:0;moving.add(motion);wake();}
  }
  const light = event => {
    if(!press)return;
    const {box,owner}=press;
    owner.style.setProperty('--hyper-press-x',`${Math.max(0,Math.min(100,(event.clientX-box.left)/box.width*100))}%`);
    owner.style.setProperty('--hyper-press-y',`${Math.max(0,Math.min(100,(event.clientY-box.top)/box.height*100))}%`);
  };
  const pressClip = (element,box) => {
    let clip={left:0,top:0,right:innerWidth,bottom:innerHeight};
    for(let parent=element.parentElement;parent;parent=parent.parentElement){
      const style=getComputedStyle(parent),x=/(auto|scroll|hidden|clip)/.test(style.overflowX),y=/(auto|scroll|hidden|clip)/.test(style.overflowY);
      if(x||y){const r=parent.getBoundingClientRect();if(x){clip.left=Math.max(clip.left,r.left+2);clip.right=Math.min(clip.right,r.right-2);}if(y){clip.top=Math.max(clip.top,r.top+2);clip.bottom=Math.min(clip.bottom,r.bottom-2);}}
      if(parent===root)break;
    }
    // A partly scrolled-out row should stay scrolled out, rather than jump
    // back into the viewport when pressed.
    return box.left>=clip.left-2&&box.top>=clip.top-2&&box.right<=clip.right+2&&box.bottom<=clip.bottom+2?clip:null;
  };
  const stopPress = () => {
    if(gesture){
      const owner=gesture.owner;
      if(owner.hasPointerCapture?.(pointerId))owner.releasePointerCapture(pointerId);
      owner.removeAttribute('data-hyper-drag');
      if(gesture.record){gesture.record.target[4]=0;active.add(gesture.record);gesture.record.velocity.fill(0);wake();if(gesture.moved)schedule(gesture.record.group);}
      gesture=null;
    }
    if(press?.motion){
      const motion=press.motion;
      if(motion.kind==='toggle'&&!reduced()&&!stopped){motion.releaseAt=performance.now()+140;moving.add(motion);wake();}
      else releaseMotion(motion);
    }
    if(pressed){pressed.removeAttribute('data-hyper-held');pressed.style.removeProperty('--hyper-press-x');pressed.style.removeProperty('--hyper-press-y');if(!groups.has(pressed))touched.delete(pressed);pressed=null;}
    press=null;
    pointerId=null;cancelBake();clearOptic();
  };
  const down = event => {
    if(!event.isPrimary||event.button!==0||reduced())return;
    const element=event.target.closest('.split-range')||event.target.closest('button,input[type="range"],summary');
    if(!element||!root.contains(element)||element.disabled||element.getAttribute('aria-disabled')==='true'||element.dataset.disabled==='true')return;
    stopPress();pressed=element;pointerId=event.pointerId;pressed.dataset.hyperHeld='';touched.add(pressed);
    const box=element.getBoundingClientRect();
    press={owner:element,box,clip:element.matches('button,summary')?pressClip(element,box):null,x:event.clientX,y:event.clientY,mouse:event.pointerType==='mouse',motion:null};light(event);
    const group=element.closest(groupSelector),record=groups.get(group);
    if(record){
      gesture={owner:group,record,start:event.clientX,offset:record.values[0],moved:false};
      record.target[4]=1;active.add(record);paint(record);requestOptic(record.lens);wake();
    }else if(element.matches(switchSelector)){
      gesture={owner:element,knob:thumb(element),start:event.clientX,checked:element.getAttribute('aria-checked')==='true',moved:false,travel:element.matches('.convert-switch-row')?13:16};
      press.motion=makeMotion(gesture.knob,element,'toggle');
      press.motion.releasing=false;press.motion.releaseAt=0;press.motion.target=[gesture.checked?gesture.travel:0,0,1.65,1.42,1];moving.add(press.motion);wake();
    }else if(!record&&element.matches('button,summary,.split-range')){
      const range=element.matches('.split-range');
      const row=element.matches(rowSelector);
      press.motion=makeMotion(range?element.querySelector('.split-range-face'):element,element,range?'range':row?'row':'button');
      press.motion.releasing=false;press.motion.releaseAt=0;
      press.motion.target=range?[0,0,1.015,1.08,1]:row?[0,0,.99,.99,1]:fitPress(pressShape(box.width,box.height,0,0,press.mouse),box,press.clip);moving.add(press.motion);wake();
    }
    if(!record&&!element.matches(`.split-range,${rowSelector}`))requestOptic(element.matches(switchSelector)?thumb(element):element);
  };
  const move = event => {
    if(event.pointerId!==pointerId)return;
    light(event);
    if(press?.motion?.kind==='button'){
      press.motion.target=fitPress(pressShape(press.box.width,press.box.height,event.clientX-press.x,event.clientY-press.y,press.mouse),press.box,press.clip);moving.add(press.motion);wake();
    }else if(press?.motion?.kind==='row'){
      press.motion.target=[rubberband(event.clientX-press.x,2),rubberband(event.clientY-press.y,1),.99,.99,1];moving.add(press.motion);wake();
    }else if(press?.motion?.kind==='range'){
      const past=event.clientX<press.box.left?event.clientX-press.box.left:event.clientX>press.box.right?event.clientX-press.box.right:0;
      const over=rubberband(past,12,.5);
      press.motion.target=[over/2,0,1.015+Math.abs(over)/press.box.width,1.08-Math.abs(over)/press.box.width/2,1];moving.add(press.motion);wake();
    }
    if(!gesture)return;
    const delta=event.clientX-gesture.start;
    if(!gesture.moved&&Math.abs(delta)<6)return;
    if(!gesture.moved){gesture.moved=true;gesture.owner.setPointerCapture(pointerId);gesture.owner.dataset.hyperDrag='';}
    if(gesture.record){
      const r=gesture.record;active.delete(r);
      r.values[0]=Math.max(0,Math.min(r.group.scrollWidth-r.values[2],gesture.offset+delta));paint(r);
      requestOptic(r.lens);
    }else if(gesture.knob){
      gesture.preview=Math.max(0,Math.min(gesture.travel,(gesture.checked?gesture.travel:0)+delta));
      if(press?.motion){press.motion.target[0]=gesture.preview;moving.add(press.motion);wake();}
    }
  };
  const click = event => {if(suppress&&event.detail!==0&&suppress.contains(event.target)){event.preventDefault();event.stopPropagation();suppress=null;}};
  const up = event => {
    if(event&&pointerId!=null&&event.pointerId!==pointerId)return;
    let choice=null;const dragged=!!gesture?.moved;
    if(gesture?.moved){
      suppress=gesture.owner;clearTimeout(suppressTimer);
      if(gesture.record){
        const group=gesture.record.group,box=group.getBoundingClientRect(),local=event.clientX-box.left+group.scrollLeft;
        choice=[...group.children].find(b=>b.tagName==='BUTTON'&&!b.disabled&&local>=b.offsetLeft&&local<=b.offsetLeft+b.offsetWidth);
      }else if((gesture.preview>=gesture.travel*(gesture.checked?.3:.7))!==gesture.checked)choice=gesture.owner;
    }
    stopPress();
    // Commit after the browser's compatibility click has been suppressed. A
    // preference change can dispose this runtime, so committing inline could
    // remove the guard and toggle twice from one drag.
    if(dragged)suppressTimer=setTimeout(()=>{suppress=null;if(!stopped)choice?.click();},0);
  };
  const scrolling = () => {
    root.dataset.hyperScrolling='';clearOptic();cancelBake();clearTimeout(scrollTimer);
    scrollTimer=setTimeout(()=>{root.removeAttribute('data-hyper-scrolling');scrollTimer=0;},140);
  };
  function settle(){
    stopPress();cancelAnimationFrame(frame);frame=0;
    for(const record of active){record.values=[...record.target];record.velocity.fill(0);paint(record);record.group.removeAttribute('data-hyper-travelling');}active.clear();
    for(const animation of animations)animation.finish();animations.clear();
    for(const motion of [...motions.values()])restoreMotion(motion);
  }
  const visibility=()=>{if(document.hidden)settle();};
  const accessibility=()=>{settle();root.dataset.hyperQuality=contrast.matches||options.quality==='efficient'?'efficient':'full';};
  const keyDown=event=>{
    if(event.repeat||![' ','Enter'].includes(event.key)||!event.target.matches('button,summary'))return;
    const box=event.target.getBoundingClientRect();down({target:event.target,isPrimary:true,button:0,pointerId:-1,pointerType:'mouse',clientX:box.x+box.width/2,clientY:box.y+box.height/2});
  };
  const keyUp=event=>{if(pointerId===-1&&[' ','Enter'].includes(event.key))up({pointerId:-1});};
  root.addEventListener('pointerdown',down,{passive:true});
  root.addEventListener('pointerover',warm,{passive:true});root.addEventListener('focusin',warm);
  root.addEventListener('keydown',keyDown);window.addEventListener('keyup',keyUp);
  root.addEventListener('click',click,true);window.addEventListener('pointermove',move,{passive:true});
  window.addEventListener('pointerup',up);window.addEventListener('pointercancel',stopPress);window.addEventListener('blur',stopPress);
  root.addEventListener('scroll',scrolling,{capture:true,passive:true});document.addEventListener('visibilitychange',visibility);
  media.addEventListener('change',accessibility);contrast.addEventListener('change',accessibility);
  inspect(root);accessibility();
  return () => {
    stopped=true;settle();cancelBake();cancelAnimationFrame(measuring);clearTimeout(scrollTimer);clearTimeout(suppressTimer);
    if(warmIdle){if(window.cancelIdleCallback)cancelIdleCallback(warmIdle);else clearTimeout(warmIdle);warmIdle=0;}
    observer.disconnect();resize.disconnect();
    root.removeEventListener('pointerdown',down);root.removeEventListener('click',click,true);window.removeEventListener('pointermove',move);
    root.removeEventListener('pointerover',warm);root.removeEventListener('focusin',warm);
    root.removeEventListener('keydown',keyDown);window.removeEventListener('keyup',keyUp);
    window.removeEventListener('pointerup',up);window.removeEventListener('pointercancel',stopPress);window.removeEventListener('blur',stopPress);
    root.removeEventListener('scroll',scrolling,true);document.removeEventListener('visibilitychange',visibility);
    media.removeEventListener('change',accessibility);contrast.removeEventListener('change',accessibility);
    for(const record of groups.values()){record.lens.remove();if(record.select.hasAttribute('data-hyper-owned'))record.select.remove();}
    const indicator=root.querySelector('.tab-indicator');if(indicator){
      if(root.dataset.material==='hyper-glass'){if(originalIndicator==null)indicator.removeAttribute('style');else indicator.setAttribute('style',originalIndicator);}
      else {indicator.style.height=originalIndicatorHeight;indicator.style.borderRadius=originalIndicatorRadius;indicator.style.removeProperty('opacity');}
    }
    for(const el of touched){for(const name of ['data-hyper-held','data-hyper-optic','data-hyper-track','data-hyper-travelling'])el.removeAttribute(name);el.style.removeProperty('--hyper-optic-filter');}
    for(const el of surfaces){el.removeAttribute('data-hyper-surface');el.removeAttribute('data-hyper-anchor');}surfaces.clear();
    for(const el of hosts){el.removeAttribute('data-hyper-host');el.removeAttribute('data-hyper-anchor');}hosts.clear();pendingSurfaces.clear();
    groups.clear();maps.clear();pendingGroups.clear();svg.remove();
    for(const name of ['data-hyper-runtime','data-hyper-scrolling','data-hyper-quality'])root.removeAttribute(name);
    if(window.islandHyperGlassDiagnostics===diagnose){if(previousDiagnostics)window.islandHyperGlassDiagnostics=previousDiagnostics;else delete window.islandHyperGlassDiagnostics;}
  };
}
