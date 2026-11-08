import assert from 'node:assert/strict';
import test from 'node:test';
import { figureCueAt, figureLoopProgress, nextFigureLoopPose, transformationPlaybackIssue } from '../src/figure-playback.js';

test('phased playback holds, reverses at a different rate and selects the cue on the shared progress', () => {
  const playback = {hold_start:2,hold_end:4,return_seconds:6};
  const times = [0,1,2,7,12,15,19,22];
  const progress = times.map(time => figureLoopProgress(playback,10,time));
  assert.deepEqual(progress,[0,0,0,.5,1,1,.5,0]);
  const cues = [{at:0,text:'Folded'},{at:.5,text:'Swing'},{at:1,text:'Deployed'}];
  assert.deepEqual(progress.map(t=>figureCueAt(cues,t)),
    ['Folded','Folded','Folded','Swing','Deployed','Deployed','Swing','Folded']);
});

test('transformation playback reverses at each endpoint without skipping a pose', () => {
  let state = { stage: 0, direction: 1 };
  const poses = [];
  for (let i = 0; i < 8; i++) {
    state = nextFigureLoopPose(state.stage, 2, state.direction);
    poses.push(state.stage);
  }
  assert.deepEqual(poses, [1, 2, 1, 0, 1, 2, 1, 0]);
  assert.deepEqual(nextFigureLoopPose(0, 0, 1), { stage: 0, direction: 1 });
});

test('truck-to-robot playback refuses parts that pop into view', () => {
  const scene = {steps:[{label:'Truck form'},{label:'Robot form'}],parts:[
    {id:'cab',shape:{kind:'box'},step:0},
    {id:'head',shape:{kind:'box'},step:0,motion:{keys:[{step:0,show:false},{step:1,show:true}]}},
  ]};
  assert.match(transformationPlaybackIssue(scene, 'Animate a truck transforming into a robot'), /reveals parts abruptly/);
  scene.parts[1].motion.keys = [{step:0,rotate:[0,0,-90]},{step:1,rotate:[0,0,0]}];
  assert.equal(transformationPlaybackIssue(scene, 'Animate a truck transforming into a robot'), '');
  scene.parts[1].step = 1;
  assert.match(transformationPlaybackIssue(scene, 'Animate a truck transforming into a robot'), /reveals parts abruptly/);
});

test('a visibility switch on an assembly also hides its attached geometry', () => {
  const scene = {steps:[{}, {}],parts:[
    {id:'hinge',shape:{kind:'group'},motion:{keys:[{step:0,show:false},{step:1,show:true,rotate:[90,0,0]}]}},
    {id:'panel',parent:'hinge',shape:{kind:'box'}},
  ]};
  assert.match(transformationPlaybackIssue(scene, 'Transform a folding panel'), /reveals parts abruptly/);
});

test('a transformation with only intermediate movement needs a distinct endpoint', () => {
  const scene = {steps:[{}, {}, {}], parts:[{shape:{kind:'box'},step:0,motion:{keys:[
    {step:0,rotate:[0,0,0]}, {step:1,rotate:[0,0,90]}, {step:2,rotate:[0,0,0]},
  ]}}]};
  assert.match(transformationPlaybackIssue(scene, 'Transform a folding chair into a flat pack'), /distinct final pose/);
  scene.parts[0].motion.keys[2].rotate[2] = 120;
  assert.equal(transformationPlaybackIssue(scene, 'Transform a folding chair into a flat pack'), '');
});

test('request-aware playback handles other subjects without blocking a staged lesson', () => {
  const scene = {steps:[{label:'Flat sheet'},{label:'Crane'}],parts:[
    {id:'wing',shape:{kind:'box'},step:0,motion:{keys:[{step:0,show:false},{step:1,show:true}]}},
  ]};
  assert.match(transformationPlaybackIssue(scene, 'Transform a paper sheet into an origami crane'), /reveals parts abruptly/);
  assert.equal(transformationPlaybackIssue(scene, 'Show a 3D experiment in stages'), '');
  scene.parts[0].motion.keys = [{step:0,rotate:[0,0,0]},{step:1,rotate:[0,0,75]}];
  assert.equal(transformationPlaybackIssue(scene, 'Transform a paper sheet into an origami crane'), '');
  scene.parts[0].motion.keys = [{step:0,show:true},{step:1,show:false}];
  assert.match(transformationPlaybackIssue(scene, 'Animate a wind turbine spinning'), /no spatial motion/);
});

import { channelPose, figurePose } from '../src/figure-motion.js';
import { Vector3 } from 'three';

test('independent channels preserve overlapping easing and child attachment', () => {
  const channels = [
    {channel:'translate.y',start:0,end:3,from:0,to:2},
    {channel:'rotate.x',start:1,end:5,from:0,to:90},
    {channel:'translate.x',start:2,end:4,from:0,to:1},
  ];
  const group = {id:'g',step:0,shape:{kind:'group'},motion:{pivot:[0,0,0],keys:[],channels}};
  const child = {id:'p',step:0,parent:'g',parent_local:true,shape:{kind:'box'}};
  const ease = t => { const q=Math.min(1,Math.max(0,t));return q*q*(3-2*q); };
  for(let t=0;t<=6;t+=.125) {
    const pose=channelPose(channels,t);
    assert.ok(Math.abs(pose.translate[1]-2*ease(t/3))<1e-10);
    assert.ok(Math.abs(pose.rotate[0]-90*ease((t-1)/4))<1e-10);
    const frames=figurePose([group,child],t);
    assert.deepEqual(frames.get('g').elements,frames.get('p').elements);
    const p=new Vector3(0,0,1).applyMatrix4(frames.get('p'));
    const angle=pose.rotate[0]*Math.PI/180;
    assert.ok(Math.abs(p.y-(pose.translate[1]-Math.sin(angle)))<1e-10);
  }
  assert.equal(transformationPlaybackIssue({parts:[group,child],steps:[{},{},{},{},{},{}]},'Animate a folding mechanism'),'');
});
