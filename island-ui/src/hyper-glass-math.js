// Bevel profile adapted from Zaki Sheriff's MIT Liquid-Glass reference (2026).
// Analytic critical damping follows its closed-form spring approach.
export function springStep(value, velocity, target, seconds, frequency = 24, damping = 1) {
  seconds = Math.max(0, Math.min(.064, seconds));
  if (damping < 1) {
    // Reference's closed-form underdamped spring: elastic releases without
    // Euler drift, including on high refresh and temporarily slow frames.
    const x=value-target,half=frequency*Math.max(.05,damping),w=Math.sqrt(frequency*frequency-half*half);
    const decay=Math.exp(-half*seconds),sin=Math.sin(w*seconds),cos=Math.cos(w*seconds);
    return [target+decay*(x*cos+(velocity+half*x)/w*sin),decay*(velocity*cos-(half*velocity+frequency*frequency*x)/w*sin)];
  }
  const x = value - target, b = velocity + frequency * x;
  const decay = Math.exp(-frequency * seconds);
  return [target + (x + b * seconds) * decay, (velocity - frequency * b * seconds) * decay];
}
export function bevelShift(distance, bezel, thickness, ior = 1.5) {
  if (distance < 0 || distance >= bezel) return 0;
  const t = Math.max(.002, distance / bezel), q = 1 - t;
  const base = 1 - q ** 4, height = base ** .25;
  const incident = Math.atan(q ** 3 / Math.max(.0001, base ** .75));
  return Math.tan(incident - Math.asin(Math.sin(incident) / ior)) * thickness * (.35 + .65 * height);
}
export function displacementField(width, height, radius, samplesX, samplesY, depth) {
  const pixels = new Uint8ClampedArray(samplesX * samplesY * 4);
  const bezel = Math.min(10, Math.min(width, height) * .24);
  const scale = 12, hw = width / 2, hh = height / 2;
  radius = Math.min(hw, hh, radius);
  // Ray-trace 256 profile samples once, as the reference does, rather than
  // repeating pow/atan/asin/tan for every bitmap pixel on the press path.
  const profile=Float32Array.from({length:256},(_,i)=>bevelShift(i/256*bezel,bezel,bezel*.55)*depth);
  for (let y = 0; y < samplesY; y++) for (let x = 0; x < samplesX; x++) {
    const px = (x + .5) / samplesX * width - hw, py = (y + .5) / samplesY * height - hh;
    const qx = Math.abs(px) - hw + radius, qy = Math.abs(py) - hh + radius;
    const ox = Math.max(qx, 0), oy = Math.max(qy, 0), len = Math.hypot(ox, oy);
    const distance = radius - (len + Math.min(Math.max(qx, qy), 0));
    const nx = len ? ox / len * Math.sign(px) : qx > qy ? Math.sign(px) : 0;
    const ny = len ? oy / len * Math.sign(py) : qy >= qx ? Math.sign(py) : 0;
    const at=Math.max(0,distance/bezel*256),index=Math.min(255,Math.floor(at));
    const shift=distance<0||distance>=bezel?0:profile[index]+((profile[index+1]||0)-profile[index])*(at-index);
    const bend = Math.min(scale * .45, shift);
    const i = (y * samplesX + x) * 4;
    pixels[i] = Math.round(128 - nx * bend / scale * 255);
    pixels[i + 1] = Math.round(128 - ny * bend / scale * 255);
    pixels[i + 2] = 128; pixels[i + 3] = 255;
  }
  return { pixels, scale };
}

export function rubberband(distance, limit, coefficient = .55) {
  return Math.sign(distance)*limit*(1-1/(Math.abs(distance)*coefficient/limit+1));
}

export function pressShape(width,height,dx=0,dy=0,mouse=true) {
  const base=1+Math.min(.22,Math.max(.07,9/Math.sqrt(width*height)))*(mouse?.75:1);
  const distance=Math.hypot(dx,dy),cx=distance?(dx/distance)**2:0,cy=distance?(dy/distance)**2:0;
  const reach=rubberband(distance,Math.min(width*cx+height*cy,64)*.32||1);
  const ex=reach*cx/width,ey=reach*cy/height;
  return [Math.sign(dx)*reach*cx*base/2+rubberband(dx,4,.4),Math.sign(dy)*reach*cy*base/2+rubberband(dy,4,.4),base*(1+ex-ey*.35),base*(1+ey-ex*.35),1];
}

// Keep an expanded press inside the cached visible scrollport. Move the
// centre away from an edge before limiting scale; never read layout per frame.
export function fitPress(shape,box,clip) {
  if(!clip)return shape;
  const [x,y,sx,sy,lift]=shape,w=box.width,h=box.height;
  const scaleX=Math.min(sx,(clip.right-clip.left)/w),scaleY=Math.min(sy,(clip.bottom-clip.top)/h);
  const cx=box.left+w/2,cy=box.top+h/2;
  return [Math.max(clip.left-cx+w*scaleX/2,Math.min(clip.right-cx-w*scaleX/2,x)),Math.max(clip.top-cy+h*scaleY/2,Math.min(clip.bottom-cy-h*scaleY/2,y)),scaleX,scaleY,lift];
}

// A tiny lossless BMP avoids waking a canvas/GPU context and synchronously
// encoding PNG during the first interaction. Image decoding remains browser-owned.
export function fieldBitmap(pixels, width, height) {
  const stride=(width*3+3)&~3,bytes=new Uint8Array(54+stride*height),v=new DataView(bytes.buffer);
  v.setUint16(0,0x4d42,true);v.setUint32(2,bytes.length,true);v.setUint32(10,54,true);
  v.setUint32(14,40,true);v.setInt32(18,width,true);v.setInt32(22,height,true);v.setUint16(26,1,true);v.setUint16(28,24,true);v.setUint32(34,stride*height,true);
  for(let y=0;y<height;y++)for(let x=0;x<width;x++){
    const s=(y*width+x)*4,d=54+(height-y-1)*stride+x*3;
    bytes[d]=pixels[s+2];bytes[d+1]=pixels[s+1];bytes[d+2]=pixels[s];
  }
  return bytes;
}
