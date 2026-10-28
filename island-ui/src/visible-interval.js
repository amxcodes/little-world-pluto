// UI clocks catch up on reveal; hidden WebViews do not need periodic renders.
// Capture/VAD timers deliberately do not use this helper.
export function visibleInterval(callback, delay) {
  let timer;
  const sync = () => {
    clearInterval(timer);
    timer = undefined;
    if (document.hidden) return;
    callback();
    timer = setInterval(callback, delay);
  };
  document.addEventListener('visibilitychange', sync);
  sync();
  return () => {
    clearInterval(timer);
    document.removeEventListener('visibilitychange', sync);
  };
}
