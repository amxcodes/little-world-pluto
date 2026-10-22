// A bounded layout pass for the 160 × 70 explanation canvas. Providers describe
// parts and relations; the app owns coordinates. No object-specific templates.
const WIDTH = 160, HEIGHT = 70;
const clamp = (value, min, max) => Math.max(min, Math.min(max, value));

function orderedRoots(roots, links, items) {
  const rootOf = id => {
    let node = items.find(item => item.id === id);
    return node?.parent && node.place && node.place !== 'inside' ? id : (node?.parent || id);
  };
  const edges = links.map(link => [rootOf(link.from), rootOf(link.to)]).filter(([a, b]) => a !== b);
  const remaining = new Set(roots.map(item => item.id)), result = [];
  while (remaining.size) {
    const ready = roots.find(item => remaining.has(item.id) && !edges.some(([a, b]) => b === item.id && remaining.has(a)));
    const next = ready || roots.find(item => remaining.has(item.id)); // cycles keep model order
    result.push(next); remaining.delete(next.id);
  }
  return result;
}

function hasUsableLegacyGeometry(items) {
  return items.length > 0 && items.every(item => [item.x, item.y, item.width, item.height].every(Number.isFinite)
    && item.width >= 3 && item.height >= 3 && item.x >= 0 && item.y >= 0
    && item.x + item.width <= 100 && item.y + item.height <= 70 && !item.parent)
    && items.every((item, i) => items.slice(i + 1).every(other =>
      item.x + item.width + 1 <= other.x || other.x + other.width + 1 <= item.x
      || item.y + item.height + 1 <= other.y || other.y + other.height + 1 <= item.y));
}

export function resolveSketchLayout(items = [], links = []) {
  if (hasUsableLegacyGeometry(items)) return { width: 100, items: items.map(item => ({ ...item })), auto: false };
  const inside = item => item.parent && (!item.place || item.place === 'inside');
  const roots = orderedRoots(items.filter(item => !inside(item)), links, items);
  const primary = roots.filter(item => items.some(child => child.parent === item.id && inside(child)));
  const boxes = new Map();
  const pad = 6, gap = 5;
  if (primary.length === 1 && roots.length === 2) {
    // One containing subject and one external input read best as a compact
    // left-to-right composition. A full-width bottom row turns one source
    // into a giant shape and sends connectors through the subject.
    const main = primary[0];
    const other = roots.find(item => item !== main);
    boxes.set(main.id, { x: 46, y: 5, width: 108, height: 60 });
    boxes.set(other.id, { x: 6, y: 28, width: 34, height: 14 });
  } else if (primary.length === 1 && roots.length > 1 && roots.length <= 6) {
    const main = primary[0];
    boxes.set(main.id, { x: 20, y: 7, width: 120, height: 38 });
    const others = roots.filter(item => item !== main);
    const cell = Math.min(42, (WIDTH - pad * 2 - gap * (others.length - 1)) / others.length);
    const groupWidth = others.length * cell + (others.length - 1) * gap;
    others.forEach((item, i) => boxes.set(item.id, {
      x: (WIDTH - groupWidth) / 2 + i * (cell + gap), y: 50, width: cell, height: 14,
    }));
  } else if (roots.length) {
    const cols = roots.length <= 4 ? roots.length : Math.min(4, Math.ceil(Math.sqrt(roots.length * 1.6)));
    const rows = Math.ceil(roots.length / cols);
    const cellW = (WIDTH - 2 * pad - gap * (cols - 1)) / cols;
    const top = 8;
    const cellH = (64 - top - gap * (rows - 1)) / rows;
    roots.forEach((item, i) => {
      const col = i % cols, row = Math.floor(i / cols);
      const hasChildren = items.some(child => child.parent === item.id && inside(child));
      const scale = hasChildren ? 1 : item.scale === 'small' ? .72 : item.scale === 'large' ? 1 : .88;
      const width = cellW * scale, height = cellH * scale;
      boxes.set(item.id, {
        x: pad + col * (cellW + gap) + (cellW - width) / 2,
        y: top + row * (cellH + gap) + (cellH - height) / 2,
        width, height,
      });
    });
  }
  for (const item of roots) {
    const box = boxes.get(item.id);
    const aspect = item.shape === 'circle' ? 1 : item.aspect;
    if (!box || !Number.isFinite(aspect) || aspect < .2 || aspect > 5) continue;
    const width = Math.min(box.width, box.height * aspect);
    const height = width / aspect;
    if (items.some(child => child.parent === item.id && inside(child)) && (width < 24 || height < 32)) continue;
    boxes.set(item.id, { x: box.x + (box.width - width) / 2, y: box.y + (box.height - height) / 2, width, height });
  }
  // Relative placement is a hint, not a demand for raw coordinates. Try the
  // requested side while retaining the collision-free grid as a fallback.
  for (const item of roots) {
    if (!item.parent || !['left', 'right', 'above', 'below'].includes(item.place)) continue;
    const parent = boxes.get(item.parent), original = boxes.get(item.id);
    if (!parent || !original) continue;
    const candidate = { ...original };
    if (item.place === 'left') { candidate.x = parent.x - candidate.width - gap; candidate.y = parent.y + (parent.height - candidate.height) / 2; }
    if (item.place === 'right') { candidate.x = parent.x + parent.width + gap; candidate.y = parent.y + (parent.height - candidate.height) / 2; }
    if (item.place === 'above') { candidate.x = parent.x + (parent.width - candidate.width) / 2; candidate.y = parent.y - candidate.height - gap; }
    if (item.place === 'below') { candidate.x = parent.x + (parent.width - candidate.width) / 2; candidate.y = parent.y + parent.height + gap; }
    if (candidate.x < pad || candidate.y < 5 || candidate.x + candidate.width > WIDTH - pad || candidate.y + candidate.height > 65) continue;
    if ([...boxes].some(([id, other]) => id !== item.id && candidate.x < other.x + other.width + 2
      && candidate.x + candidate.width + 2 > other.x && candidate.y < other.y + other.height + 2
      && candidate.y + candidate.height + 2 > other.y)) continue;
    boxes.set(item.id, candidate);
  }
  // Children occupy their parent surface. A 2-column interior is predictable
  // for low-capability models and keeps parts attached during resize.
  for (const parent of roots) {
    const children = items.filter(item => item.parent === parent.id && inside(item));
    const box = boxes.get(parent.id);
    if (!box || !children.length) continue;
    const cols = children.length <= 2 ? children.length : 2, rows = Math.ceil(children.length / cols);
    const innerX = box.x + 3, innerY = box.y + 9;
    const innerW = box.width - 6, innerH = box.height - 12, childGap = 2;
    const width = (innerW - childGap * (cols - 1)) / cols;
    const height = (innerH - childGap * (rows - 1)) / rows;
    children.forEach((child, i) => boxes.set(child.id, {
      x: innerX + (i % cols) * (width + childGap),
      y: innerY + Math.floor(i / cols) * (height + childGap), width, height,
    }));
  }
  for (const item of items.filter(inside)) {
    const box = boxes.get(item.id), aspect = item.shape === 'circle' ? 1 : item.aspect;
    if (!box || !Number.isFinite(aspect) || aspect < .2 || aspect > 5) continue;
    const width = Math.min(box.width, box.height * aspect), height = width / aspect;
    boxes.set(item.id, { x: box.x + (box.width - width) / 2, y: box.y + (box.height - height) / 2, width, height });
  }
  return { width: WIDTH, auto: true, items: items.map(item => ({ ...item, ...(boxes.get(item.id) || {}) })) };
}

function strokeMask(width, strokes) {
  const mask = new Uint8Array(width * HEIGHT);
  for (const stroke of strokes) {
    if (stroke.on || (stroke.role && stroke.role !== 'contour')) continue;
    const points = stroke.points || [];
    for (let i = 1; i < points.length; i++) {
      const a = points[i - 1], b = points[i], steps = Math.ceil(Math.hypot(b[0] - a[0], b[1] - a[1]) * width / 100);
      for (let step = 0; step <= steps; step++) {
        const x = Math.round((a[0] + (b[0] - a[0]) * step / Math.max(steps, 1)) * width / 100);
        const y = Math.round(a[1] + (b[1] - a[1]) * step / Math.max(steps, 1));
        for (let dy = -1; dy <= 1; dy++) for (let dx = -1; dx <= 1; dx++) {
          const xx = x + dx, yy = y + dy;
          if (xx >= 0 && xx < width && yy >= 0 && yy < HEIGHT) mask[yy * width + xx] = 1;
        }
      }
    }
  }
  return mask;
}

const centre = item => [item.x + item.width / 2, item.y + item.height / 2];
function port(item, other, vertical) {
  const [x, y] = centre(item), [ox, oy] = centre(other);
  return vertical ? [Math.round(x), Math.round(y + (oy > y ? item.height : -item.height) / 2)]
    : [Math.round(x + (ox > x ? item.width : -item.width) / 2), Math.round(y)];
}

function shortestPath(start, end, blocked, width, occupied) {
  const from = clamp(start[1], 1, HEIGHT - 2) * width + clamp(start[0], 1, width - 2);
  const to = clamp(end[1], 1, HEIGHT - 2) * width + clamp(end[0], 1, width - 2);
  const previous = new Int32Array(width * HEIGHT).fill(-1);
  const queue = new Int32Array(width * HEIGHT);
  let head = 0, tail = 0;
  queue[tail++] = from; previous[from] = from;
  const towardX = end[0] >= start[0] ? 1 : -1, towardY = end[1] >= start[1] ? 1 : -1;
  const moves = [towardX, towardY * width, -towardY * width, -towardX];
  while (head < tail && previous[to] === -1) {
    const cell = queue[head++], x = cell % width, y = (cell / width) | 0;
    for (const move of moves) {
      const nx = x + (Math.abs(move) === 1 ? move : 0), ny = y + (Math.abs(move) === width ? move / width : 0);
      if (nx < 2 || nx >= width - 2 || ny < 2 || ny >= HEIGHT - 2) continue;
      const next = cell + move;
      if (previous[next] !== -1 || (blocked[next] && next !== to) || (occupied?.[next] && next !== to)) continue;
      previous[next] = cell; queue[tail++] = next;
    }
  }
  if (previous[to] === -1) return null;
  const cells = [];
  for (let at = to; at !== from; at = previous[at]) cells.push(at);
  cells.push(from); cells.reverse();
  const points = cells.map(cell => [cell % width, (cell / width) | 0]);
  return points.filter((point, index) => index === 0 || index === points.length - 1
    || (points[index - 1][0] - point[0]) !== (point[0] - points[index + 1][0])
    || (points[index - 1][1] - point[1]) !== (point[1] - points[index + 1][1]));
}

export function routeSketchLinks(items, links, width, strokes) {
  const contours = strokeMask(width, strokes);
  const occupied = new Uint8Array(width * HEIGHT);
  return links.map(link => {
    const from = items.find(item => item.id === link.from), to = items.find(item => item.id === link.to);
    if (!from || !to) return null;
    const blocked = contours.slice();
    for (const item of items) {
      const endpoint = item === from || item === to;
      // Containers are a visible frame; a wire may enter them to reach a
      // child. Other children remain obstacles. Never route through an
      // endpoint's own label area just because its port is allowed.
      if ((item.id === from.parent || item.id === to.parent) && !endpoint) continue;
      if (endpoint && (from.parent === to.id || to.parent === from.id) && item.id === (from.parent || to.parent)) continue;
      const margin = endpoint ? -1 : 1;
      for (let y = clamp(Math.floor(item.y - margin), 0, HEIGHT - 1); y <= clamp(Math.ceil(item.y + item.height + margin), 0, HEIGHT - 1); y++) {
        for (let x = clamp(Math.floor(item.x - margin), 0, width - 1); x <= clamp(Math.ceil(item.x + item.width + margin), 0, width - 1); x++) blocked[y * width + x] = 1;
      }
    }
    const [fx, fy] = centre(from), [tx, ty] = centre(to);
    const preferredVertical = Math.abs(ty - fy) > Math.abs(tx - fx);
    const candidates = [preferredVertical, !preferredVertical].map(vertical => [port(from, to, vertical), port(to, from, vertical)]);
    let path = null;
    for (const [a, b] of candidates) {
      path = shortestPath(a, b, blocked, width, occupied) || shortestPath(a, b, blocked, width, null);
      if (path) break;
    }
    if (path) for (const [x, y] of path) occupied[y * width + x] = 1;
    return path;
  });
}
