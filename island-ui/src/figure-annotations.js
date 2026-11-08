import { BufferGeometry, CanvasTexture, Float32BufferAttribute, Group, Line, LineBasicMaterial, Sprite, SpriteMaterial, Vector3 } from 'three';

export function figureAnnotations(labels, parts, selectedId = '') {
  const requested = labels?.length ? labels : defaultFigureLabels(parts, selectedId);
  return requested.slice(0, 12).flatMap(label => {
    const part = label.part ? parts.find(part => part.id === label.part) : null;
    if (label.part && !part) return [];
    const point = label.point ?? part?.bounds?.center ?? part?.motion?.pivot ?? part?.transform?.position;
    return point ? [{ text: label.text, part: label.part, point }] : [];
  });
}
function defaultFigureLabels(parts, selectedId) {
  const candidates = parts.filter(part => part.bounds && part.label?.trim() && part.shape?.kind !== 'arrow');
  const size = part => (part.bounds.size ?? []).reduce((sum, axis) => sum + axis * axis, 0);
  candidates.sort((a, b) => Number(b.id === selectedId) - Number(a.id === selectedId) || size(b) - size(a));
  const usedGroups = new Set(), usedText = new Set();
  const labels = [];
  for (const part of candidates) {
    const group = part.parent ?? part.id;
    const text = part.label.trim();
    const name = text.toLowerCase();
    if (usedGroups.has(group) || usedText.has(name)) continue;
    labels.push({ text, part: part.id });
    usedGroups.add(group);
    usedText.add(name);
    if (labels.length === 4) break;
  }
  return labels;
}

export function layoutFigureLabel(anchor, requestedWidth, width, height, occupied) {
  const boxWidth = Math.min(requestedWidth, width - 8);
  let best = null, bestOverlap = Infinity;
  for (const side of [1, -1]) {
    for (let row = 0; row <= 24; row++) {
      const shift = row === 0 ? 0 : Math.ceil(row / 2) * 28 * (row % 2 ? -1 : 1);
      const left = Math.max(4, Math.min(width - boxWidth - 4,
        side === 1 ? anchor[0] + 10 : anchor[0] - boxWidth - 10));
      const top = Math.max(4, Math.min(height - 28, anchor[1] - 32 + shift));
      const box = [left, top, left + boxWidth, top + 24];
      const overlap = occupied.reduce((area, other) => area
        + Math.max(0, Math.min(box[2] + 2, other[2]) - Math.max(box[0] - 2, other[0]))
          * Math.max(0, Math.min(box[3] + 2, other[3]) - Math.max(box[1] - 2, other[1])), 0);
      if (overlap < bestOverlap) { best = box; bestOverlap = overlap; }
      if (overlap === 0) { occupied.push(box); return box; }
    }
  }
  occupied.push(best);
  return best;
}

export function createFigureAnnotations(scene, camera) {
  const group = new Group();
  const leaders = new Group();
  scene.add(group, leaders);
  let key = '';
  const clear = () => {
    for (const sprite of group.children) {
      sprite.material.map.dispose();
      sprite.material.dispose();
    }
    group.clear();
    for (const line of leaders.children) {
      line.geometry.dispose();
      line.material.dispose();
    }
    leaders.clear();
  };
  return {
    draw(labels, poses, visibility, height) {
      const width = height * (camera.right - camera.left) / (camera.top - camera.bottom);
      group.visible = leaders.visible = width >= 40 && height >= 30;
      if (!group.visible) return;
      const next = JSON.stringify(labels.map(label => label.text));
      if (key !== next) {
        clear();
        key = next;
        for (const label of labels) {
          const canvas = document.createElement('canvas');
          const context = canvas.getContext('2d');
          if (!context) continue;
          context.font = '600 13px "Segoe UI", sans-serif';
          const width = Math.min(300, Math.ceil(context.measureText(label.text).width) + 16);
          canvas.width = width * 2;
          canvas.height = 48;
          context.scale(2, 2);
          context.fillStyle = 'rgba(10,13,18,.9)';
          context.beginPath();
          context.roundRect(0, 0, width, 24, 7);
          context.fill();
          context.fillStyle = '#fff';
          context.font = '600 13px "Segoe UI", sans-serif';
          context.fillText(label.text, 8, 17, width - 16);
          const material = new SpriteMaterial({ map: new CanvasTexture(canvas), depthTest: false, depthWrite: false });
          const sprite = new Sprite(material);
          sprite.center.set(0, 0);
          sprite.renderOrder = 100;
          sprite.userData.width = width;
          group.add(sprite);
          const geometry = new BufferGeometry();
          geometry.setAttribute('position', new Float32BufferAttribute(new Float32Array(9), 3));
          const line = new Line(geometry, new LineBasicMaterial({ color: '#d4dae6', depthTest: false, depthWrite: false }));
          line.frustumCulled = false;
          line.renderOrder = 99;
          leaders.add(line);
        }
      }
      const pixel = (camera.top - camera.bottom) / height;
      const occupied = [];
      labels.forEach((label, index) => {
        const sprite = group.children[index];
        if (!sprite) return;
        sprite.visible = !label.part || visibility?.get(label.part) !== false;
        sprite.position.set(...label.point);
        const pose = poses?.get(label.part);
        if (pose) sprite.position.applyMatrix4(pose);
        const projected = sprite.position.clone().project(camera);
        sprite.visible &&= Math.abs(projected.x) <= 1 && Math.abs(projected.y) <= 1 && Math.abs(projected.z) <= 1;
        const line = leaders.children[index];
        line.visible = sprite.visible;
        if (!sprite.visible) return;
        const anchor = [(projected.x + 1) / 2 * width, (1 - projected.y) / 2 * height];
        const box = layoutFigureLabel(anchor, sprite.userData.width, width, height, occupied);
        const unproject = (x, y) => new Vector3(x / width * 2 - 1, 1 - y / height * 2, projected.z).unproject(camera);
        sprite.position.copy(unproject(box[0], box[3]));
        sprite.scale.set((box[2] - box[0]) * pixel, 24 * pixel, 1);
        const end = [Math.max(box[0], Math.min(box[2], anchor[0])), Math.max(box[1], Math.min(box[3], anchor[1]))];
        const positions = line.geometry.getAttribute('position');
        for (const [i, point] of [anchor, [end[0], anchor[1]], end].entries()) {
          const world = unproject(...point);
          positions.setXYZ(i, world.x, world.y, world.z);
        }
        positions.needsUpdate = true;
      });
    },
    dispose() { clear(); scene.remove(group, leaders); },
  };
}
