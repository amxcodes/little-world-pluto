export function rangeValueAt(clientX, left, width, min, max, step) {
  if (width <= 0) return min;
  const fraction = Math.max(0, Math.min(1, (clientX - left) / width));
  return Math.max(min, Math.min(max, min + Math.round((fraction * (max - min)) / step) * step));
}

export function rangeEdge(progress) {
  return Math.min(progress, 100 - progress, 20) / 20;
}
