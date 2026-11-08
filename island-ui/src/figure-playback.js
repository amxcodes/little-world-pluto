import { channelPose } from "./figure-motion.js";

export function figureLoopProgress({ hold_start = 0, hold_end = 0, return_seconds }, forward, elapsed) {
  const returnStart = hold_start + forward + hold_end;
  const phase = elapsed % (returnStart + return_seconds);
  return phase < returnStart
    ? Math.min(1, Math.max(0, (phase - hold_start) / forward))
    : 1 - (phase - returnStart) / return_seconds;
}

export function figureCueAt(cues, progress) {
  return cues.findLast(cue => cue.at <= progress)?.text ?? '';
}

export function nextFigureLoopPose(stage, last, direction) {
  if (last < 1) return { stage: 0, direction: 1 };
  const nextDirection = stage >= last ? -1 : stage <= 0 ? 1 : direction;
  return { stage: Math.max(0, Math.min(last, stage + nextDirection)), direction: nextDirection };
}

// A staged build may legitimately add parts. Continuous physical motion may not
// substitute stage visibility for a path. Use the original request as the
// intent signal; stage names are author-supplied and may be misleading.
export function transformationPlaybackIssue(scene, request = '') {
  const words = request.toLowerCase();
  const abstract = /data|chart|graph|workflow|flowchart|interface|ui animation/.test(words);
  const transforms = !abstract && /transform|morph|fold into|unfold|reshape|turn into/.test(words);
  const animates = !abstract && /animate|animation|moving model|in motion|spinning|rotating|orbiting|folding|unfolding/.test(words);
  const steps = scene?.steps ?? [];
  if (!transforms && !animates) return '';
  const parts = scene?.parts ?? [];
  const spatialMotion = parts.some(part => {
    const motion = part.motion;
    return motion && (motion.channels?.some(track => track.from !== track.to) || motion.spin || motion.bob || motion.orbit || motion.path ||
      motion.tick?.translate || motion.tick?.rotate ||
      motion.keys?.some(key => key.translate?.some(value => value !== 0) ||
        key.rotate?.some(value => value !== 0)));
  });
  if (transforms) {
    if (steps.length < 2) return 'This transformation needs a start and end pose.';
    const physical = new Set();
    for (const part of parts.slice().reverse()) {
      if (!['group', 'arrow'].includes(part.shape?.kind) || physical.has(part.id)) {
        physical.add(part.id);
        if (part.parent) physical.add(part.parent);
      }
    }
    const abrupt = parts.find(part => physical.has(part.id) &&
      (part.step > 0 || part.motion?.tick?.show ||
        part.motion?.keys?.some(key => key.show != null ||
          (key.scale != null && key.scale < .1))));
    if (abrupt) return 'This transformation reveals parts abruptly and needs a revised motion model.';
    if (spatialMotion && !parts.some(part => {
      const motion = part.motion;
      if (motion?.channels?.length) {
        return JSON.stringify(channelPose(motion.channels, 0)) !== JSON.stringify(channelPose(motion.channels, steps.length - 1));
      }
      if (!motion || motion.cycle || !motion.keys?.length) return false;
      const first = motion.keys.find(key => (key.at ?? key.step) === 0);
      const last = motion.keys.at(-1);
      return ['translate', 'rotate'].some(channel =>
        (last[channel] ?? []).some((value, axis) => value !== (first?.[channel]?.[axis] ?? 0)));
    })) return 'This transformation needs a distinct final pose.';
  }
  return spatialMotion ? '' : 'This animation has no spatial motion and needs a revised motion model.';
}
