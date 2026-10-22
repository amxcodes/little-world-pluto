// Compact statuses are modules with explicit eligibility, a protected priority
// lane, and a shared renderer payload. Keep the lane floor here so new status
// types cannot accidentally demote recording or live screen-tool work.
export const compactStatusModules = [
  {
    id: 'recording', label: 'Recording', icon: 'Record', fixed: true,
    lane: 'protected', detail: 'Always first while recording so Stop stays available.',
    resolve: context => context.recording ? { elapsed: context.recordingElapsed, stopping: context.recordingStopping } : null,
  },
  {
    id: 'tool-active', label: 'Active screen tools', icon: 'Capture', fixed: true,
    lane: 'protected', detail: 'Pinned above summaries until capture, OCR, or QR work completes.',
    resolve: context => {
      const feedback = [...context.feedbacks].reverse().find(item => item.phase === 'active' && item.kind !== 'record');
      return feedback ? { feedback } : null;
    },
  },
  {
    id: 'agent-approval', label: 'Agent approval', icon: 'Lock', fixed: true,
    lane: 'protected', detail: 'A pending agent decision stays reachable until answered.',
    resolve: context => context.agentApproval ? { approval: context.agentApproval } : null,
  },
  {
    id: 'reminder', label: 'Due reminders', lane: 'attention', icon: 'Reminder',
    detail: 'A due note stays available until it is acknowledged or cleared.',
    resolve: context => {
      const feedback = context.feedbacks.find(item => item.kind === 'reminder');
      return feedback ? { feedback } : null;
    },
  },
  {
    id: 'notification', label: 'Notices', lane: 'attention', icon: 'Check',
    detail: 'Errors and short system notices wait in a bounded queue.',
    resolve: context => {
      const feedback = context.feedbacks.find(item => item.kind === 'notice' && item.phase !== 'active');
      return feedback ? { feedback } : null;
    },
  },
  {
    id: 'tool-result', label: 'Recent results', lane: 'attention', icon: 'Check',
    detail: 'Copy, capture, OCR/QR, and other completed tool results.',
    resolve: context => {
      const feedback = context.feedbacks.find(item => !['reminder', 'notice'].includes(item.kind) && item.phase !== 'active');
      return feedback ? { feedback } : null;
    },
  },
  {
    id: 'media', label: 'Media playback', lane: 'context', icon: 'Play',
    detail: 'The current track or video while it is playing.',
    resolve: context => context.media ? { media: context.media } : null,
  },
  {
    id: 'agent', label: 'Agent activity', lane: 'context', icon: 'Agents',
    detail: 'The active OpenCode or Codex task and its work indicator.',
    resolve: context => context.activeRuns ? { activeRuns: context.activeRuns, activeSession: context.activeSession } : null,
  },
  {
    id: 'weather', label: 'Rain forecast', lane: 'context', icon: 'Weather',
    detail: 'Rain predicted in the next six hours.',
    resolve: context => context.rain ? { rain: context.rain } : null,
  },
  {
    id: 'updates', label: 'Software updates', lane: 'context', icon: 'Refresh',
    detail: 'Briefly offers ready updates when live work has cleared.',
    resolve: context => context.update ? { update: context.update } : null,
  },
  {
    id: 'idle', label: 'Idle', icon: 'Clock', fallback: true,
    resolve: () => ({}),
  },
];

export const compactStatusLanes = [
  { id: 'attention', label: 'Attention and results' },
  { id: 'context', label: 'Background context' },
];

export const defaultCompactStatusOrder = compactStatusModules
  .filter(module => !module.fixed && !module.fallback)
  .map(module => module.id);

export function normalizeCompactStatusOrder(value) {
  const allowed = new Set(defaultCompactStatusOrder);
  const requested = Array.isArray(value) ? value.filter(id => allowed.has(id)) : [];
  const unique = [...new Set(requested)];
  const complete = [...unique, ...defaultCompactStatusOrder.filter(id => !unique.includes(id))];
  return compactStatusLanes.flatMap(lane => complete.filter(id => compactStatusModules.find(module => module.id === id)?.lane === lane.id));
}

export function resolveCompactStatus(context, preferredOrder = defaultCompactStatusOrder) {
  const modules = new Map(compactStatusModules.map(module => [module.id, module]));
  const feedbacks = Array.isArray(context.feedbacks) ? context.feedbacks : context.feedback ? [context.feedback] : [];
  const safeContext = { ...context, feedbacks };
  const fixed = compactStatusModules.filter(module => module.fixed).map(module => module.id);
  const order = [...fixed, ...normalizeCompactStatusOrder(preferredOrder)];
  for (const id of order) {
    const module = modules.get(id);
    const payload = module?.resolve(safeContext);
    if (payload) return { id, payload };
  }
  return { id: 'idle', payload: {} };
}
