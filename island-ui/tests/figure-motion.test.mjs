import { figurePoseInspection, figureJointDefects, figureAssemblyGaps } from '../src/figure-motion.js';
import assert from 'node:assert/strict';
import test from 'node:test';
import { Vector3 } from 'three';
import { figureAppearance, figureBindMatrix, figureMotionBounds, figurePose, figureVisibility } from '../src/figure-motion.js';

const parts = [
  { id: 'chassis', step: 0, bounds: { center: [2, 0, 0], size: [2, .1, .1] },
    motion: { pivot: [1, 0, 0], keys: [{ step: 1, rotate: [0, 0, 90], translate: [0, 0, 0] }] } },
  { id: 'wheel', parent: 'chassis', step: 0, bounds: { center: [3, 0, 0], size: [.5, .5, .5] },
    motion: { pivot: [3, 0, 0], keys: [{ step: 1, rotate: [0, 0, 0], translate: [1, 0, 0] }] } },
];

function close(actual, expected) {
  assert.ok(actual.distanceTo(new Vector3(...expected)) < 1e-8, actual.toArray().join(','));
}

test('a hinge stays in place while its chassis and child move through intermediate poses', () => {
  const middle = figurePose(parts, .5);
  close(new Vector3(1, 0, 0).applyMatrix4(middle.get('chassis')), [1, 0, 0]);
  close(new Vector3(2, 0, 0).applyMatrix4(middle.get('chassis')), [1 + Math.SQRT1_2, Math.SQRT1_2, 0]);
  const final = figurePose(parts, 1);
  close(new Vector3(3, 0, 0).applyMatrix4(final.get('wheel')), [1, 3, 0]);
  assert.ok(figureMotionBounds(parts, 1).containsPoint(new Vector3(1, 3, 0)));
});

test('continuous spin composes with a staged hinge and carries its child', () => {
  const spinning = [
    { ...parts[0], motion: { ...parts[0].motion, spin: { axis: 'z', period: 4 } } },
    parts[1],
  ];
  const matrices = figurePose(spinning, 1, 1);
  close(new Vector3(1, 0, 0).applyMatrix4(matrices.get('chassis')), [1, 0, 0]);
  close(new Vector3(3, 0, 0).applyMatrix4(matrices.get('wheel')), [-2, 0, 0]);
  const repeated = figurePose(spinning, 1, 5);
  close(new Vector3(3, 0, 0).applyMatrix4(repeated.get('wheel')), [-2, 0, 0]);
});

test('bob and pulse preserve the pivot and move descendants', () => {
  const nodes = [
    { id: 'root', step: 0, motion: { pivot: [1, 0, 0], keys: [],
      bob: { offset: [0, 2, 0], period: 4 }, pulse: { amplitude: .5, period: 4 } } },
    { id: 'child', parent: 'root', step: 0 },
  ];
  const pose = figurePose(nodes, 0, 1);
  close(new Vector3(1, 0, 0).applyMatrix4(pose.get('root')), [1, 2, 0]);
  close(new Vector3(3, 0, 0).applyMatrix4(pose.get('child')), [4, 2, 0]);
});

test('framing contains a child through unrelated spin, bob and pulse periods', () => {
  const nodes = [
    { id: 'root', step: 0, bounds: { center: [0, 0, 0], size: [1, 1, 1] },
      motion: { pivot: [0, 0, 0], keys: [], spin: { axis: 'y', period: 4 },
        bob: { offset: [0, 2, 0], period: 7 } } },
    { id: 'child', parent: 'root', step: 0, bounds: { center: [4, 0, 0], size: [2, 2, 2] },
      motion: { pivot: [0, 0, 0], keys: [], pulse: { amplitude: .5, period: 3 } } },
  ];
  const envelope = figureMotionBounds(nodes, 0);
  for (let tick = 0; tick < 840; tick++) {
    const pose = figurePose(nodes, 0, tick / 10).get('child');
    for (const x of [3, 5]) for (const y of [-1, 1]) for (const z of [-1, 1]) {
      assert.ok(envelope.containsPoint(new Vector3(x, y, z).applyMatrix4(pose)));
    }
  }
});

test('orbit translates without rotating the part and stays inside the camera envelope', () => {
  const nodes = [{ id: 'arm', step: 0, bounds: { center: [3, 0, 0], size: [2, 1, 1] },
    motion: { pivot: [3, 0, 0], keys: [], orbit: { axis: 'y', center: [1, 0, 0], period: 4 } } }];
  const matrix = figurePose(nodes, 0, 1).get('arm');
  close(new Vector3(3, 0, 0).applyMatrix4(matrix), [1, 0, -2]);
  close(new Vector3(4, 0, 0).applyMatrix4(matrix), [2, 0, -2]);
  assert.ok(figureMotionBounds(nodes, 0).containsPoint(new Vector3(2, 0, -2)));
});

test('a meshless group animates its child and does not contribute geometry to framing', () => {
  const nodes = [
    { id: 'joint', step: 0, motion: { pivot: [10, 0, 0], keys: [],
      spin: { axis: 'z', period: 4 } } },
    { id: 'arm', parent: 'joint', step: 0, bounds: { center: [12, 0, 0], size: [1, 1, 1] } },
  ];
  const position = new Vector3(12, 0, 0).applyMatrix4(figurePose(nodes, 0, 1).get('arm'));
  close(position, [10, 2, 0]);
  const bounds = figureMotionBounds(nodes, 0);
  assert.ok(bounds.containsPoint(position));
  assert.ok(!bounds.containsPoint(new Vector3()));
});

test('one-shot path settles at its endpoint while a loop returns to rest', () => {
  const path = { points: [[0, 0, 0], [3, 0, 0], [3, 2, 0]], period: 4 };
  const nodes = [{ id: 'body', step: 0, motion: { pivot: [0, 0, 0], keys: [], path } }];
  close(new Vector3().applyMatrix4(figurePose(nodes, 0, 4).get('body')), [0, 0, 0]);
  path.once = true;
  close(new Vector3().applyMatrix4(figurePose(nodes, 0, 2).get('body')), [3, 0, 0]);
  close(new Vector3().applyMatrix4(figurePose(nodes, 0, 8).get('body')), [3, 2, 0]);
});

test('keyed scale eases around a pivot, composes with pulse, and reaches descendants', () => {
  const nodes = [
    { id: 'base', step: 0, motion: { pivot: [1, 0, 0],
      keys: [{step: 1, scale: 3, rotate: [0, 0, 0], translate: [0, 0, 0]}],
      pulse: {amplitude: .5, period: 4} } },
    { id: 'child', parent: 'base', step: 0, bounds: { center: [3, 0, 0], size: [1, 1, 1] } },
  ];
  const middle = figurePose(nodes, .5, 1);
  close(new Vector3(1, 0, 0).applyMatrix4(middle.get('base')), [1, 0, 0]);
  close(new Vector3(3, 0, 0).applyMatrix4(middle.get('child')), [7, 0, 0]);
  assert.ok(figureMotionBounds(nodes, 1).containsPoint(new Vector3(12, 0, 0)));
});

test('visibility persists through unset keys and gates descendants', () => {
  const nodes = [
    {id:'group', motion:{keys:[{step:0, show:false}, {step:1}, {step:2, show:true}]}},
    {id:'child', parent:'group', motion:{keys:[{step:0, show:true}]}},
    {id:'sibling'},
  ];
  assert.deepEqual([...figureVisibility(nodes, 1.5).values()], [false,false,true]);
  assert.deepEqual([...figureVisibility(nodes, 2).values()], [true,true,true]);
});

test('appearance eases from the base and keeps the last explicit fields', () => {
  const part = {step:0, motion:{keys:[{step:1,tint:[1,0,0],glow:.8},{step:2}]}};
  assert.deepEqual(figureAppearance(part,.5,[0,0,1]),{tint:[.5,0,.5],glow:.4});
  assert.deepEqual(figureAppearance(part,2,[0,0,1]),{tint:[1,0,0],glow:.8});
});

test('tick overrides replace only specified motion channels', () => {
  const nodes = [{id:'part',step:0,bounds:{center:[2,0,0],size:[1,1,1]},
    motion:{pivot:[1,0,0],keys:[{step:1,translate:[9,0,0],rotate:[0,0,90],scale:2}],
      bob:{offset:[0,2,0],period:4}, tick:{period:4,translate:[[0,0,0],[0,4,0]]}}}];
  const pose = figurePose(nodes,1,1).get('part');
  close(new Vector3(1,0,0).applyMatrix4(pose),[1,2,0]);
  close(new Vector3(2,0,0).applyMatrix4(pose),[1,4,0]);
  assert.ok(figureMotionBounds(nodes,1).containsPoint(new Vector3(1,4,0)));
});

test('cyclic appearance overrides staged values and inherits visibility', () => {
  const nodes = [
    {id:'root',step:0,motion:{keys:[{step:0,glow:1}],
      tick:{period:4,tint:[[1,0,0],[0,0,1]],glow:[0,.5],show:[true,false]}}},
    {id:'child',step:0,parent:'root'},
  ];
  assert.deepEqual([...figureVisibility(nodes,0,2).values()],[false,false]);
  assert.deepEqual([...figureVisibility(nodes,0,4).values()],[true,true]);
  assert.deepEqual(figureAppearance(nodes[0],0,[1,1,1],2),{tint:[0,0,1],glow:.5});
});

test('future-stage parts stay hidden in a start-pose review image', () => {
  const parts = [
    {id:'chassis', step:0},
    {id:'head', step:2, motion:{keys:[{step:2,show:true}]}},
    {id:'face', step:2, parent:'head'},
  ];
  assert.deepEqual([...figureVisibility(parts,0).values()], [true,false,false]);
  assert.deepEqual([...figureVisibility(parts,2).values()], [true,true,true]);
});


test('parent-local children inherit group placement and rotation without changing legacy parts', () => {
  const group = {id:'joint',step:0,transform:{position:[3,0,0],rotation:[0,0,90],scale:[1,1,1]}};
  const child = {id:'child',parent:'joint',parent_local:true,step:0,transform:{position:[2,0,0],rotation:[0,0,0],scale:[1,1,1]}};
  const matrices = figurePose([group,child],0);
  const point = new Vector3(2,0,0).applyMatrix4(matrices.get('child'));
  assert.ok(point.distanceTo(new Vector3(3,2,0)) < 1e-6);
  assert.equal(figurePose([group,{...child,parent_local:false}],0).has('child'), false);
});


test('nested local assemblies keep a detail attached through a joint turn', () => {
  const t = position => ({position,rotation:[0,0,0],scale:[1,1,1]});
  const parts = [
    {id:'joint',step:0,transform:t([3,0,0]),motion:{pivot:[3,0,0],keys:[{step:1,translate:[0,0,0],rotate:[0,0,90]}]}},
    {id:'assembly',step:0,parent:'joint',parent_local:true,transform:t([2,0,0])},
    {id:'detail',step:0,parent:'assembly',parent_local:true,transform:t([1,0,0])},
  ];
  for (const time of [0,.25,.5,.75,1]) {
    const pose = figurePose(parts,time);
    const assembly = new Vector3(2,0,0).applyMatrix4(pose.get('assembly'));
    const detail = new Vector3(1,0,0).applyMatrix4(pose.get('detail'));
    assert.ok(Math.abs(assembly.distanceTo(detail)-1) < 1e-6);
  }
  const endpoint = new Vector3(1,0,0).applyMatrix4(figurePose(parts,1).get('detail'));
  assert.ok(endpoint.distanceTo(new Vector3(3,3,0)) < 1e-6);
});


test('review measurements resolve local parts into world coordinates', () => {
  const parts = [
    {id:'group',step:0,transform:{position:[3,0,0],rotation:[0,0,0],scale:[1,1,1]}},
    {id:'detail',parent:'group',parent_local:true,step:0,bounds:{center:[1,0,0],size:[1,1,1]}},
  ];
  assert.match(figurePoseInspection(parts,1), /detail: center 4.00,0.00,0.00/);
});


test('declared joints remain connected under rotation and report intermediate separation', () => {
  const t = position => ({position,rotation:[0,0,0],scale:[1,1,1]});
  const parts = [
    {id:'base',step:0,transform:t([3,0,0])},
    {id:'hinge',parent:'base',parent_local:true,step:0,transform:t([1,0,0]),
      joint:{anchor:[0,0,0],parent_anchor:[1,0,0],tolerance:.01},
      motion:{pivot:[1,0,0],keys:[{step:1,translate:[0,0,0],rotate:[0,0,90]}]}},
  ];
  assert.deepEqual(figureJointDefects(parts,1), []);
  parts[1].motion.keys = [{step:1,translate:[0,1,0],rotate:[0,0,0]},{step:2,translate:[0,0,0],rotate:[0,0,0]}];
  const defects = figureJointDefects(parts,2);
  assert.equal(defects.length,1);
  assert.equal(defects[0].stage,1);
  assert.ok(Math.abs(defects[0].gap-1)<1e-6);
  assert.match(figurePoseInspection(parts,2), /JOINT FAILURE hinge -> base/);
});

test('endpoint inspection catches opposite folding on an attached paired hinge', () => {
  const pose = (at, angle) => ({at, translate: [0,0,0], rotate: [angle,0,0]});
  const position = x => ({position: [x,0,0]});
  const parts = [{id: 'root', step: 0, transform: position(4), motion: {
    pivot: [4,0,0], keys: [pose(0,0), pose(1,30)],
  }}];
  const assemblies = [];
  for (const [id, x, angle] of [['left',-1,90], ['right',1,-90]]) {
    parts.push({id, step: 0, parent: 'root', parent_local: true, transform: position(x),
      joint: {anchor:[0,0,0], parent_anchor:[x,0,0], tolerance:.01},
      motion: {pivot:[x,0,0], keys:[pose(0,0),pose(1,angle)]}});
    assemblies.push({id, landmarks: [1,2].map(y => ({
      point:[0,y,0], start:[x,y,0], end:[x,0,y], tolerance:.05,
    }))});
  }
  assert.deepEqual(figureJointDefects(parts,1), []);
  const plan = {assemblies};
  const inspection = figurePoseInspection(parts,1,plan);
  assert.match(inspection,/ENDPOINT FAILURE right end/);
  assert.doesNotMatch(inspection,/ENDPOINT FAILURE left/);
  parts[2].motion.keys[1] = pose(1,90);
  assert.doesNotMatch(figurePoseInspection(parts,1,plan),/ENDPOINT FAILURE/);
});

test('review identifies stationary landmarks and inventories authored detail by support', () => {
  const parts = [
    {id:'body',step:0},
    {id:'guide',parent:'body',parent_local:true,step:0},
    {id:'panel',parent:'guide',parent_local:true,step:0,shape:{kind:'box'},bounds:{center:[0,0,0],size:[1,1,1]}},
  ];
  const plan = {assemblies:[{id:'guide',landmarks:[{point:[0,0,0],start:[0,0,0],end:[0,0,1],tolerance:.05}]}]};
  const review = figurePoseInspection(parts,1,plan);
  assert.match(review,/ENDPOINT FAILURE guide end: .* stays .* must travel 1.00/);
  assert.match(review,/AUTHORED GEOMETRY BY SUPPORTING GROUP: guide: panel \(box\)/);
});

test('an endpoint can reach its target but its assembled geometry can still fail to fit', () => {
  const parts = [
    {id:'root',step:0,transform:{position:[4,0,0]}},
    {id:'pack',parent:'root',parent_local:true,step:0,transform:{position:[0,1,0]}},
    {id:'case',parent:'pack',parent_local:true,step:0,shape:{kind:'box'},bounds:{center:[0,0,0],size:[1,1,3]}},
  ];
  const role = {id:'pack',end_box:{min:[-1,0,-1],max:[1,2,1]}};
  assert.match(figurePoseInspection(parts,1,{assemblies:[role]}),/FIT FAILURE pack end:.*-0.50,0.50,-1.50/);
  role.end_box.min[2] = -1.6;
  role.end_box.max[2] = 1.6;
  assert.doesNotMatch(figurePoseInspection(parts,1,{assemblies:[role]}),/FIT FAILURE/);
});

test('cyclic tracks can have a phase offset while using one elapsed time', () => {
  const parts = [0,.5].map((phase,index) => ({id: String(index), step:0, motion:{
    pivot:[0,0,0], keys:[], tick:{period:4,phase,translate:[[0,0,0],[2,0,0]]},
  }}));
  const start = figurePose(parts,0,0);
  const half = figurePose(parts,0,2);
  assert.deepEqual([start.get('0').elements[12],start.get('1').elements[12]],[0,2]);
  assert.deepEqual([half.get('0').elements[12],half.get('1').elements[12]],[2,0]);
});


test('shared timeline staggers two hinge moves and holds earlier targets', () => {
  const move = (id, start, end) => ({id,step:0,motion:{pivot:[0,0,0],keys:[
    {at:start,translate:[0,0,0],rotate:[0,0,0]},
    {at:end,translate:[0,0,0],rotate:[0,0,90]},
  ]}});
  const parts = [move('early',.125,.5),move('late',.5,1)];
  const point = (part, time) => new Vector3(1,0,0).applyMatrix4(figurePose(parts,time).get(part));
  assert.ok(point('late',.25).distanceTo(new Vector3(1,0,0)) < 1e-6);
  assert.ok(point('early',.75).distanceTo(new Vector3(0,1,0)) < 1e-6);
  assert.ok(point('late',.75).distanceTo(new Vector3(Math.SQRT1_2,Math.SQRT1_2,0)) < 1e-6);
});


test('assembly inspection catches an intermediate gap without a declared joint', () => {
  const bounds = center => ({center, size: [1, 1, 1]});
  const parts = [
    {id:'base',step:0,bounds:bounds([0,0,0])},
    {id:'slider',step:0,bounds:bounds([1,0,0]),motion:{pivot:[1,0,0],keys:[
      {at:1,translate:[.5,0,0],rotate:[0,0,0]},
      {at:2,translate:[0,0,0],rotate:[0,0,0]},
    ]}},
  ];
  assert.deepEqual(figureAssemblyGaps(parts, 2), [{ids:['slider'],gap:.5,stage:1}]);
  assert.deepEqual(figureAssemblyGaps(parts.map(({motion, ...part}) => part), 2), []);
});

test('a converted group-local pivot stays fixed on a translated and rotated carrier', () => {
  const carrier = {
    id: 'carrier', step: 0, transform: {position:[5,2,0],rotation:[0,0,90],scale:[1,1,1]},
    motion: {pivot:[5,3,0],keys:[],channels:[
      {channel:'rotate.z',start:0,end:1,from:0,to:90}
    ]}
  };
  for(const stage of [0,.125,.25,.5,.75,.875,1]){
    const frame = figurePose([carrier],stage).get('carrier').clone().multiply(figureBindMatrix(carrier));
    close(new Vector3(1,0,0).applyMatrix4(frame),[5,3,0]);
  }
  const end=figurePose([carrier],1).get('carrier').clone().multiply(figureBindMatrix(carrier));
  close(new Vector3(2,0,0).applyMatrix4(end),[4,3,0]);
});


test('single-pose inspection detects a floating static component', () => {
  const mesh=(id,y)=>({id,step:0,shape:{kind:'box'},transform:{position:[0,y,0]},bounds:{center:[0,y,0],size:[1,1,1]}});
  assert.match(figurePoseInspection([mesh('base',0),mesh('cap',3)],0),/ASSEMBLY GAP/);
  assert.doesNotMatch(figurePoseInspection([mesh('base',0),mesh('cap',1)],0),/ASSEMBLY GAP/);
});


test('a signed group frame mirrors attached detail without changing its size', () => {
  const carrier={id:'carrier',step:0,transform:{position:[-2,0,0],scale:[-1,1,1]}};
  const detail={id:'detail',parent:'carrier',parent_local:true,step:0,transform:{position:[1,0,0]}};
  const pose=figurePose([carrier,detail],0);
  assert.ok(new Vector3(1,0,0).applyMatrix4(pose.get('detail')).distanceTo(new Vector3(-3,0,0))<1e-6);
  const a=new Vector3(0,0,0).applyMatrix4(pose.get('detail'));
  const b=new Vector3(1,0,0).applyMatrix4(pose.get('detail'));
  assert.ok(Math.abs(a.distanceTo(b)-1)<1e-6);
});
