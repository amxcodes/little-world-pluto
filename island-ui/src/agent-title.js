export function cleanAgentTitle(value, fallback = 'Untitled task') {
  const title = String(value || '')
    .split(/additional instructions configured in pluto:/i, 1)[0]
    .replace(/[\s—|:·-]+$/u, '')
    .trim()
    .slice(0, 80);
  return title || fallback;
}

export function cleanAgentText(value) {
  let text = String(value || '').replace(/\r\n?/g, '\n');
  const marker = /additional instructions configured in pluto:/i;
  for (;;) {
    const match = marker.exec(text);
    if (!match) break;
    const end = text.indexOf('\n\n', match.index);
    const before = text.slice(0, match.index).trimEnd();
    const after = end < 0 ? '' : text.slice(end + 2).trimStart();
    text = [before, after].filter(Boolean).join('\n\n');
  }
  return text.replace(/\n{3,}/g, '\n\n').trim();
}
