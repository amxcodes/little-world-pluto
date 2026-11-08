import {test} from 'node:test';
import assert from 'node:assert/strict';
import {resolveAppearance,normalizeHyper} from '../src/appearance-state.js';
import {springStep,displacementField,fieldBitmap,pressShape,fitPress,rubberband,bevelShift} from '../src/hyper-glass-math.js';
test('existing mode survives absent or invalid Hyper Glass preference, explicit mode stays independent',()=>{
 assert.equal(resolveAppearance(null,{enabled:true}),'glass');assert.equal(resolveAppearance(null,{enabled:false}),'standard');
 assert.equal(resolveAppearance('broken',{enabled:true}),'glass');assert.equal(resolveAppearance('hyper-glass',{enabled:true}),'hyper-glass');
 assert.equal(resolveAppearance('standard',{enabled:true}),'standard');
 assert.deepEqual(normalizeHyper({motion:false,depth:10000,frost:-4,quality:'bad'}),{motion:false,depth:100,frost:0,light:48,quality:'auto'});
});

test('elastic releases stay stable, overshoot gently and settle at different refresh rates',()=>{
 for(const hz of [30,60,120,240]){
  let x=1.15,v=0,minimum=1;for(let i=0;i<hz*2;i++){[x,v]=springStep(x,v,1,1/hz,19,.58);minimum=Math.min(minimum,x);assert.ok(Number.isFinite(x)&&x>.95&&x<1.2);}
  assert.ok(minimum<1);assert.ok(Math.abs(x-1)<.0001);
 }
});

test('physical press follows a drag, is bounded and keeps the trailing side attached',()=>{
 for(const [w,h] of [[32,32],[120,32],[400,48]]){
  const rest=pressShape(w,h),right=pressShape(w,h,20,0),left=pressShape(w,h,-20,0);
  assert.ok(rest[2]>1&&rest[2]<=1.1651);assert.equal(rest[0],0);
  assert.ok(right[0]>0&&left[0]<0);assert.ok(right[2]>rest[2]&&right[3]<rest[3]);
  assert.ok(Math.abs(rubberband(100000,12))<12);
 }
});

test('cached optical profile remains close to the original ray calculation',()=>{
 const {pixels,scale}=displacementField(100,32,16,100,32,.45),bezel=32*.24;
 for(let x=1;x<8;x++){
  const distance=x+.5,bend=Math.min(scale*.45,bevelShift(distance,bezel,bezel*.55)*.45);
  assert.ok(Math.abs(pixels[(16*100+x)*4]-Math.round(128+bend/scale*255))<=1);
 }
});

test('edge presses preserve expansion without crossing the cached scrollport',()=>{
 const clip={left:0,top:0,right:46,bottom:200};
 for(const box of [{left:6,top:6,width:34,height:34},{left:0,top:0,width:34,height:34},{left:12,top:166,width:34,height:34}]){
  for(const [dx,dy] of [[0,0],[-80,-80],[80,80]]){
   const [x,y,sx,sy]=fitPress(pressShape(34,34,dx,dy),box,clip),cx=box.left+17+x,cy=box.top+17+y;
   assert.ok(sx>1&&sy>1);assert.ok(cx-17*sx>=clip.left-1e-8&&cx+17*sx<=clip.right+1e-8);
   assert.ok(cy-17*sy>=clip.top-1e-8&&cy+17*sy<=clip.bottom+1e-8);
  }
 }
 const shape=pressShape(34,34);assert.equal(fitPress(shape,{},null),shape);
});
test('analytic spring is stable across frame rates and interruption',()=>{
 for(const hz of [30,60,120,240]){
  let x=0,v=0;for(let i=0;i<hz;i++){[x,v]=springStep(x,v,100,1/hz);assert.ok(Number.isFinite(x));assert.ok(x>=0&&x<=100.001);}
  assert.ok(Math.abs(x-100)<.001);for(let i=0;i<hz;i++)[x,v]=springStep(x,v,0,1/hz);assert.ok(Math.abs(x)<.001);
 }
});
test('optical map is bounded, neutral in the center and bends opposite rim sides',()=>{
 const {pixels,scale}=displacementField(100,32,16,100,32,1);assert.equal(pixels.length,12800);assert.equal(scale,12);
 const at=(x,y,c)=>pixels[(y*100+x)*4+c];assert.equal(at(50,16,0),128);assert.equal(at(50,16,1),128);
 assert.ok(at(1,16,0)>128);assert.ok(at(98,16,0)<128);assert.ok(at(50,1,1)>128);assert.ok(at(50,30,1)<128);
 const zero=displacementField(100,32,16,10,4,0).pixels;assert.ok(zero.every((x,i)=>i%4===3?x===255:x===128));
 const bmp=fieldBitmap(zero,10,4),header=new DataView(bmp.buffer);assert.equal(header.getUint16(0,true),0x4d42);assert.equal(header.getUint32(2,true),bmp.length);assert.equal(header.getInt32(18,true),10);assert.equal(header.getUint16(28,true),24);
});
