import React, { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { createRoot } from 'react-dom/client';
import { createPortal } from 'react-dom';
import { PageBoundary } from './page-boundary.jsx';
import { HugeiconsIcon } from '@hugeicons/react';
import { compactStatusLanes, compactStatusModules, defaultCompactStatusOrder, normalizeCompactStatusOrder, resolveCompactStatus } from './compact-status.js';
import { AgentSettings, AgentsWorkspace, readAgentPreferences, saveAgentPreferences, defaultAskInstructions } from './agents-ui.jsx';
import { SettingsDisclosure, HomeActionSettings, PrioritySettings } from './settings-controls.jsx';
import { ConversionWorkspace, readConversionDefaults, saveConversionDefaults } from './conversion-ui.jsx';
import { Home01Icon, ClipboardIcon, InboxIcon, TextFontIcon, BotIcon, SlidersHorizontalIcon,
  NotebookTextIcon, CloudIcon, Settings02Icon, PinIcon, VolumeHighIcon, PlayIcon,
  PauseIcon, NextIcon, PreviousIcon, Add01Icon, CopyIcon, TrashIcon, RefreshCwIcon,
  Wifi02Icon, BluetoothIcon, Moon01Icon, BatteryIcon, Mic01Icon, Camera01Icon,
  SpeakerIcon, FolderAddIcon, Search01Icon, NoteEditIcon,
  Location01Icon, GoBackward10SecIcon, GoForward10SecIcon,
  AiScanTextIcon, CodeCircleIcon, Settings01Icon, RecordIcon, ScreenShareIcon,
  Calendar01Icon, ArrowLeft01Icon, ArrowRight01Icon, CornerDownLeftIcon, LockIcon, ShutDownIcon,
  ArrowUp01Icon, ArrowDown01Icon, CheckmarkCircle02Icon, Cancel01Icon,
  Loading03Icon, Link01Icon, Copy01Icon, Task01Icon, TerminalIcon,
  Time01Icon, Clock01Icon, HistoryIcon, MoreHorizontalIcon, PaperclipIcon, QrCodeScanIcon,
  ColorPickerIcon, CommandIcon, AlarmClockIcon, File01Icon, File02Icon,
  Image01Icon, Video01Icon, FileAudioIcon, FileArchiveIcon, FileCodeIcon,
  FileTextIcon, Folder01Icon, Pdf01Icon, ComputerIcon, SignatureIcon,
  Download01Icon, ListMusicIcon, SendIcon, ClipboardPasteIcon } from '@hugeicons/core-free-icons';

const initial = {
  page: 'Home', dark: true, accentGradient: 'lime', mediaCardStyle: 'standard', expanded: false, pinned: false, status: '', modules: {}, activity: '', clipboard: [], shelf: [],
  text: '', note: '', weatherCity: '', weatherLines: [], weatherLoading: false,
  wakeWord: { status: 'off', phrase: 'Hey Island', revision: 0, local: true },
  outputDevices: [], selectedOutputDeviceId: null, defaultOutputDeviceId: null, commandResult: null, commandError: null,
};

function send(kind, data = {}) {
  window.ipc?.postMessage(JSON.stringify({ kind, ...data }));
}

function mergeMedia(previous, incoming) {
  if (incoming == null) return incoming;
  return {
    ...incoming,
    artwork: Object.prototype.hasOwnProperty.call(incoming, 'artwork')
      ? incoming.artwork
      : incoming.artworkRevision === previous?.artworkRevision
        ? previous?.artwork ?? null
        : null,
  };
}

const pages = ['Home', 'Clipboard', 'Shelf', 'Convert', 'Text', 'Agents', 'Ask', 'Note', 'Weather', 'Controls'];
const accentGradients = [['lime', 'Lime'], ['ice', 'Ice'], ['rose', 'Rose'], ['violet', 'Violet']];
const isEditor = element => element?.matches?.('input, textarea, [contenteditable="true"]');
const icons = {
  Home: Home01Icon, Clipboard: ClipboardIcon, Shelf: InboxIcon, Convert: File01Icon, Text: TextFontIcon,
  VoiceWake: RecordIcon, Question: NoteEditIcon, Ask: Mic01Icon, Agents: CodeCircleIcon, Bot: BotIcon, Note: NotebookTextIcon, Weather: CloudIcon,
  Controls: SlidersHorizontalIcon, Settings: Settings02Icon, Pin: PinIcon,
  Volume: VolumeHighIcon, Play: PlayIcon, Pause: PauseIcon, Next: NextIcon,
  Previous: PreviousIcon, Add: Add01Icon, Copy: CopyIcon, Trash: TrashIcon,
  Refresh: RefreshCwIcon, Wifi: Wifi02Icon, Bluetooth: BluetoothIcon,
  Moon: Moon01Icon, Battery: BatteryIcon, Clock: Time01Icon, Mic: Mic01Icon, Camera: Camera01Icon,
  Speaker: SpeakerIcon, FolderAdd: FolderAddIcon, Search: Search01Icon, Edit: NoteEditIcon,
  Location: Location01Icon, Back10: GoBackward10SecIcon, Forward10: GoForward10SecIcon,
  Capture: ScreenShareIcon, OCR: AiScanTextIcon, Record: RecordIcon,
  Calendar: Calendar01Icon, PreviousMonth: ArrowLeft01Icon, NextMonth: ArrowRight01Icon,
  Up: ArrowUp01Icon, Down: ArrowDown01Icon,
  Lock: LockIcon, Power: ShutDownIcon,
  Send: ArrowUp01Icon, AgentSend: SendIcon, ChevronDown: ArrowDown01Icon, ChevronRight: ArrowRight01Icon, Check: CheckmarkCircle02Icon,
  Cancel: Cancel01Icon, Loading: Loading03Icon, Link: Link01Icon, Attach: PaperclipIcon,
  CopyCode: Copy01Icon, Task: Task01Icon, Terminal: TerminalIcon,
  Pending: Time01Icon, History: HistoryIcon, More: MoreHorizontalIcon,
  MediaQueue: ListMusicIcon,
  Qr: QrCodeScanIcon, ColorPicker: ColorPickerIcon, Command: CommandIcon,
  Reminder: AlarmClockIcon, ReminderTime: Clock01Icon, File: File01Icon, Computer: ComputerIcon,
  Sign: SignatureIcon, Download: Download01Icon, Paste: ClipboardPasteIcon, Image: Image01Icon,
  Folder: Folder01Icon, Enter: CornerDownLeftIcon,
  Video: Video01Icon, FileText: FileTextIcon, FileCode: FileCodeIcon,
  Archive: FileArchiveIcon, Audio: FileAudioIcon, Pdf: Pdf01Icon,
};
function Icon({ name, size = 17 }) {
  return <HugeiconsIcon icon={icons[name] || CloudIcon} size={size} color="currentColor" strokeWidth={1.7} aria-hidden="true" />;
}

const homeActionCatalog = [
  { id: 'capture', label: 'Capture', detail: 'Capture a region or display', icon: 'Capture', contexts: ['browser', 'coding', 'spreadsheet', 'office', 'design', 'media'] },
  { id: 'record', label: 'Record', detail: 'Record a region or display', icon: 'Record', contexts: ['browser', 'coding', 'design', 'media'] },
  { id: 'ocr', label: 'OCR', detail: 'Read text from the screen', icon: 'OCR', contexts: ['browser', 'coding', 'spreadsheet', 'office', 'design'] },
  { id: 'qr', label: 'QR', detail: 'Scan a code on the screen', icon: 'Qr', contexts: ['browser'] },
  { id: 'color', label: 'Color', detail: 'Pick a color from the screen', icon: 'ColorPicker', contexts: ['browser', 'design'] },
  { id: 'command', label: 'Command', detail: 'Find files, apps, and actions', icon: 'Command', contexts: ['browser', 'coding', 'spreadsheet', 'office', 'design', 'media', 'other'] },
  { id: 'dictate', label: 'Dictate', detail: 'Speak text into Island', icon: 'Mic', contexts: ['browser', 'coding', 'spreadsheet', 'office', 'design', 'media', 'other'] },
  { id: 'sign', label: 'Sign', detail: 'Draw and export a signature', icon: 'Sign', contexts: ['office'] },
  { id: 'remind', label: 'Remind', detail: 'Open notes and reminders', icon: 'Reminder', contexts: ['browser', 'coding', 'spreadsheet', 'office', 'design', 'media', 'other'] },
  { id: 'task', label: 'Task', detail: 'Start an OpenCode task', icon: 'Agents', contexts: ['coding'] },
  { id: 'clipboard-page', label: 'Clipboard', detail: 'Open clipboard history', icon: 'Clipboard', contexts: ['coding', 'spreadsheet', 'office', 'design'] },
  { id: 'shelf-page', label: 'Shelf', detail: 'Open saved files', icon: 'Shelf', contexts: ['coding', 'spreadsheet', 'office', 'design'] },
  { id: 'text-page', label: 'Text', detail: 'Open the scratchpad', icon: 'Text', contexts: ['coding', 'spreadsheet', 'office'] },
  { id: 'agents-page', label: 'Agents', detail: 'Open agent activity', icon: 'Agents', contexts: ['coding'] },
  { id: 'notes-page', label: 'Notes', detail: 'Open your notes', icon: 'Note', contexts: ['browser', 'coding', 'spreadsheet', 'office', 'design', 'media', 'other'] },
  { id: 'weather-page', label: 'Weather', detail: 'Open the forecast', icon: 'Weather', contexts: ['other'] },
  { id: 'controls-page', label: 'Controls', detail: 'Open system controls', icon: 'Controls', contexts: ['media', 'other'] },
  { id: 'convert-page', label: 'Convert', detail: 'Convert or resize a file', icon: 'Convert', contexts: ['spreadsheet', 'office', 'design'] },
];
const defaultHomeActions = ['capture', 'record', 'ocr', 'qr', 'color', 'command', 'dictate', 'sign', 'remind', 'task'];
const normalizeHomeActions = value => {
  const seen = new Set();
  return Array.from({ length: 10 }, (_, index) => {
    const id = Array.isArray(value) ? value[index] : null;
    if (!homeActionCatalog.some(action => action.id === id) || seen.has(id)) return null;
    seen.add(id);
    return id;
  });
};
const loadHomeActions = () => {
  try {
    const saved = JSON.parse(localStorage.getItem('island.home-actions') || 'null');
    return Array.isArray(saved) ? normalizeHomeActions(saved) : [...defaultHomeActions];
  } catch { return [...defaultHomeActions]; }
};
const loadCompactStatusOrder = () => {
  try {
    return normalizeCompactStatusOrder(JSON.parse(localStorage.getItem('island.compact-status-order') || 'null'));
  } catch { return [...defaultCompactStatusOrder]; }
};

function NestedPageHeader({ title, icon, onBack, backLabel = 'Home', detail, titleId, className = '' }) {
  const hasParentPage = backLabel !== 'Home';
  return <header className={`nested-page-header ${className}`.trim()}>
    {hasParentPage && <>
      <button type="button" className="nested-page-back" onClick={onBack} aria-label={`Back to ${backLabel}`} title={`Back to ${backLabel}`}>
        <span className="nested-page-back-icon"><Icon name="PreviousMonth" size={15} /></span>
        <span>{backLabel}</span>
      </button>
      <span className="nested-page-divider" aria-hidden="true" />
    </>}
    <div className="nested-page-identity">
      {icon && <span className="nested-page-icon"><Icon name={icon} size={16} /></span>}
      <span className="nested-page-copy"><h2 id={titleId}>{title}</h2>{detail && <small>{detail}</small>}</span>
    </div>
  </header>;
}

const signaturePaper = '#F5F3EE';

function drawSignatureStroke(context, points, width, height) {
  if (!points.length) return;
  context.lineWidth = 2.6;
  context.lineCap = 'round';
  context.lineJoin = 'round';
  context.strokeStyle = '#171719';
  context.fillStyle = '#171719';
  const first = points[0];
  if (points.length === 1) {
    context.beginPath();
    context.arc(first.x * width, first.y * height, context.lineWidth / 2, 0, Math.PI * 2);
    context.fill();
    return;
  }
  context.beginPath();
  context.moveTo(first.x * width, first.y * height);
  for (let index = 1; index < points.length; index += 1) {
    const previous = points[index - 1];
    const current = points[index];
    context.quadraticCurveTo(previous.x * width, previous.y * height,
      (previous.x + current.x) * width / 2, (previous.y + current.y) * height / 2);
  }
  const last = points.at(-1);
  context.lineTo(last.x * width, last.y * height);
  context.stroke();
}

function SignaturePanel({ onBack, strokesRef }) {
  const canvasRef = useRef(null);
  const activeStrokeRef = useRef(null);
  const redrawRef = useRef(() => {});
  const [hasInk, setHasInk] = useState(() => strokesRef.current.length > 0);
  const [format, setFormat] = useState('png');
  const [status, setStatus] = useState('');

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return undefined;
    const redraw = () => {
      const bounds = canvas.getBoundingClientRect();
      if (!bounds.width || !bounds.height) return;
      const scale = Math.min(3, Math.max(1, window.devicePixelRatio || 1));
      const pixelWidth = Math.max(1, Math.round(bounds.width * scale));
      const pixelHeight = Math.max(1, Math.round(bounds.height * scale));
      if (canvas.width !== pixelWidth || canvas.height !== pixelHeight) {
        canvas.width = pixelWidth;
        canvas.height = pixelHeight;
      }
      const context = canvas.getContext('2d');
      if (!context) return;
      context.setTransform(scale, 0, 0, scale, 0, 0);
      context.clearRect(0, 0, bounds.width, bounds.height);
      strokesRef.current.forEach(stroke => drawSignatureStroke(context, stroke, bounds.width, bounds.height));
    };
    redrawRef.current = redraw;
    const observer = new ResizeObserver(redraw);
    observer.observe(canvas);
    window.addEventListener('resize', redraw);
    redraw();
    return () => {
      observer.disconnect();
      window.removeEventListener('resize', redraw);
      if (redrawRef.current === redraw) redrawRef.current = () => {};
    };
  }, [strokesRef]);

  useEffect(() => {
    if (!status) return undefined;
    const timer = setTimeout(() => setStatus(''), 2200);
    return () => clearTimeout(timer);
  }, [status]);

  useEffect(() => {
    const onKeyDown = event => {
      if (event.key === 'Escape') {
        event.preventDefault();
        onBack();
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [onBack]);

  const pointFromEvent = (event, canvas) => {
    const bounds = canvas.getBoundingClientRect();
    if (!bounds.width || !bounds.height) return null;
    return {
      x: Math.max(0, Math.min(1, (event.clientX - bounds.left) / bounds.width)),
      y: Math.max(0, Math.min(1, (event.clientY - bounds.top) / bounds.height)),
    };
  };

  const startStroke = event => {
    if (event.pointerType === 'touch' || event.button !== 0) return;
    event.preventDefault();
    const canvas = canvasRef.current;
    const point = pointFromEvent(event, canvas);
    const bounds = canvas.getBoundingClientRect();
    const context = canvas.getContext('2d');
    if (!point || !context) return;
    canvas.setPointerCapture(event.pointerId);
    const stroke = [point];
    strokesRef.current.push(stroke);
    activeStrokeRef.current = { id: event.pointerId, points: stroke, midpoint: point };
    drawSignatureStroke(context, stroke, bounds.width, bounds.height);
    setHasInk(true);
    setStatus('');
  };

  const continueStroke = event => {
    const active = activeStrokeRef.current;
    const canvas = canvasRef.current;
    if (!active || active.id !== event.pointerId || !canvas) return;
    event.preventDefault();
    const bounds = canvas.getBoundingClientRect();
    const context = canvas.getContext('2d');
    if (!bounds.width || !bounds.height || !context) return;
    const nativeEvent = event.nativeEvent;
    const samples = nativeEvent.getCoalescedEvents?.() ?? [nativeEvent];
    for (const sample of samples) {
      const point = pointFromEvent(sample, canvas);
      if (!point) continue;
      const previous = active.points.at(-1);
      if (Math.abs(point.x - previous.x) + Math.abs(point.y - previous.y) < 0.00015) continue;
      const middle = { x: (previous.x + point.x) / 2, y: (previous.y + point.y) / 2 };
      context.lineWidth = 2.6;
      context.lineCap = 'round';
      context.lineJoin = 'round';
      context.strokeStyle = '#171719';
      context.beginPath();
      context.moveTo(active.midpoint.x * bounds.width, active.midpoint.y * bounds.height);
      context.quadraticCurveTo(previous.x * bounds.width, previous.y * bounds.height,
        middle.x * bounds.width, middle.y * bounds.height);
      context.stroke();
      active.midpoint = middle;
      active.points.push(point);
    }
  };

  const finishStroke = event => {
    const active = activeStrokeRef.current;
    if (!active || active.id !== event.pointerId) return;
    const canvas = canvasRef.current;
    const bounds = canvas?.getBoundingClientRect();
    const context = canvas?.getContext('2d');
    const last = active.points.at(-1);
    if (bounds?.width && bounds?.height && context && last) {
      context.lineWidth = 2.6;
      context.lineCap = 'round';
      context.strokeStyle = '#171719';
      context.beginPath();
      context.moveTo(active.midpoint.x * bounds.width, active.midpoint.y * bounds.height);
      context.lineTo(last.x * bounds.width, last.y * bounds.height);
      context.stroke();
    }
    activeStrokeRef.current = null;
  };

  const editStroke = action => {
    activeStrokeRef.current = null;
    if (action === 'clear') strokesRef.current = [];
    else strokesRef.current.pop();
    const nextHasInk = strokesRef.current.length > 0;
    setHasInk(nextHasInk);
    setStatus(action === 'clear' ? 'Signature cleared' : 'Stroke undone');
    requestAnimationFrame(() => redrawRef.current());
  };

  const createExportCanvas = (imageFormat = 'png') => {
    const source = canvasRef.current;
    const strokes = strokesRef.current;
    if (!source || !strokes.length || !source.clientWidth || !source.clientHeight) return null;
    const points = strokes.flat();
    if (!points.length) return null;
    const width = source.clientWidth;
    const height = source.clientHeight;
    const scale = source.width / width;
    const inset = 18;
    const left = Math.max(0, Math.min(...points.map(point => point.x)) * width - inset);
    const top = Math.max(0, Math.min(...points.map(point => point.y)) * height - inset);
    const right = Math.min(width, Math.max(...points.map(point => point.x)) * width + inset);
    const bottom = Math.min(height, Math.max(...points.map(point => point.y)) * height + inset);
    const cropWidth = Math.max(1, right - left);
    const cropHeight = Math.max(1, bottom - top);
    const output = document.createElement('canvas');
    output.width = Math.max(1, Math.ceil(cropWidth * scale));
    output.height = Math.max(1, Math.ceil(cropHeight * scale));
    const context = output.getContext('2d');
    if (!context) return null;
    if (imageFormat === 'jpeg') {
      context.fillStyle = signaturePaper;
      context.fillRect(0, 0, output.width, output.height);
    }
    context.drawImage(source, left * scale, top * scale, cropWidth * scale, cropHeight * scale,
      0, 0, output.width, output.height);
    return output;
  };

  const exportSignature = async () => {
    const imageFormat = format === 'jpeg' ? 'jpeg' : 'png';
    const mime = imageFormat === 'jpeg' ? 'image/jpeg' : 'image/png';
    const canvas = createExportCanvas(imageFormat);
    if (!canvas) { setStatus('Draw a signature first'); return; }
    try {
      const blob = await new Promise(resolve => canvas.toBlob(resolve, mime, imageFormat === 'jpeg' ? 0.94 : undefined));
      if (!blob) throw new Error('Image export was empty');
      const url = URL.createObjectURL(blob);
      const link = document.createElement('a');
      link.href = url;
      link.download = `signature.${imageFormat === 'jpeg' ? 'jpg' : 'png'}`;
      link.click();
      setTimeout(() => URL.revokeObjectURL(url), 5000);
      setStatus(`${imageFormat.toUpperCase()} exported`);
    } catch {
      setStatus('Could not export the signature');
    }
  };

  const startNativeDrag = event => {
    if (event.button !== 0 || !hasInk) return;
    const canvas = createExportCanvas('png');
    if (!canvas) { setStatus('Could not prepare the signature PNG'); return; }
    const base64 = canvas.toDataURL('image/png').split(',')[1] || '';
    if (!base64 || base64.length > 5_600_000) { setStatus('Signature is too large to drag'); return; }
    setStatus('Drag over an app, then release');
    send('signatureDrag', { png: base64 });
  };

  useEffect(() => {
    const receive = result => setStatus(result?.message || 'Drag ended');
    window.islandSignatureDragResult = receive;
    return () => { if (window.islandSignatureDragResult === receive) delete window.islandSignatureDragResult; };
  }, []);

  return <section className="signature-panel" aria-label="Signature pad">
    <NestedPageHeader title="Sign" icon="Sign" onBack={onBack} detail="Draw with your mouse" className="signature-heading" />
    <div className="signature-pad-frame">
      <canvas ref={canvasRef} className="signature-canvas" aria-label="Signature drawing area. Draw with the mouse. Use Undo or Clear to edit."
        onPointerDown={startStroke} onPointerMove={continueStroke} onPointerUp={finishStroke} onPointerCancel={finishStroke} />
      <span className="signature-baseline" aria-hidden="true" />
      {!hasInk && <span className="signature-placeholder" aria-hidden="true"><Icon name="Sign" size={17} />Sign here</span>}
    </div>
    <div className="signature-toolbar">
      <button type="button" onClick={() => editStroke('undo')} disabled={!hasInk} title="Undo last stroke">Undo</button>
      <button type="button" onClick={() => editStroke('clear')} disabled={!hasInk} title="Clear signature">Clear</button>
      <span className="signature-toolbar-spacer" />
      <div className="signature-format" role="group" aria-label="Export format">
        <button type="button" aria-pressed={format === 'png'} className={format === 'png' ? 'active' : ''} onClick={() => setFormat('png')}>PNG</button>
        <button type="button" aria-pressed={format === 'jpeg'} className={format === 'jpeg' ? 'active' : ''} onClick={() => setFormat('jpeg')}>JPEG</button>
      </div>
      <button type="button" className="signature-export" onClick={exportSignature} disabled={!hasInk} title={`Export ${format.toUpperCase()}`}><Icon name="Download" size={15} />Export</button>
      <button type="button" className="signature-drag" onPointerDown={startNativeDrag} onClick={event => { if (event.detail === 0) exportSignature(); }} disabled={!hasInk} title="Press and drag a PNG into another app" aria-label="Drag signature PNG into another app">Drag</button>
    </div>
    <span className="signature-status" role="status" aria-live="polite">{status || 'Transparent PNG · press Drag PNG and move to an app'}</span>
  </section>;
}

const shelfTypeIcons = {
  image: Image01Icon, video: Video01Icon, audio: FileAudioIcon,
  archive: FileArchiveIcon, code: FileCodeIcon, text: FileTextIcon,
  document: File02Icon, pdf: Pdf01Icon, folder: Folder01Icon, app: ComputerIcon, file: File01Icon,
};

function ShelfVisual({ kind = 'file', visual, compact = false }) {
  const isMedia = kind === 'image' || kind === 'video';
  const showsShellVisual = (isMedia || kind === 'app') && visual;
  return <span className={compact ? 'strip-app-icon' : 'file-glyph'} data-kind={kind} aria-hidden="true">
    {showsShellVisual
      ? <img className={isMedia ? 'shelf-preview-image' : 'shelf-native-icon'} src={visual} alt="" draggable="false" />
      : <HugeiconsIcon icon={shelfTypeIcons[kind] || File01Icon} size={compact ? 19 : 27} color="currentColor" strokeWidth={1.7} />}
  </span>;
}

function BatteryGlyph({ percent, charging = false }) {
  const value = percent == null ? null : Math.max(0, Math.min(100, Number(percent) || 0));
  return <span className="battery-symbol-case" aria-hidden="true">
    {value != null && <i style={{ width: `${value}%` }} />}
    {charging && <b />}
  </span>;
}

function CompactTransition(props) {
  const { state, compactStatus } = props;
  const feedback = compactStatus.payload?.feedback;
  // Status identity only: timer, meter and battery updates must never restart motion.
  const identity = `${compactStatus.id}:${feedback?.id || ''}:${feedback?.phase || ''}:${compactStatus.id === 'recording' && props.screenRecordingStopping ? 'saving' : ''}`;
  const [systemReduced, setSystemReduced] = useState(() => window.matchMedia('(prefers-reduced-motion: reduce)').matches);
  useEffect(() => {
    const preference = window.matchMedia('(prefers-reduced-motion: reduce)');
    const update = () => setSystemReduced(preference.matches);
    preference.addEventListener('change', update);
    update();
    return () => preference.removeEventListener('change', update);
  }, []);
  const reduced = !!state.shellSettings?.reducedMotion || systemReduced;
  const [contextOpen, setContextOpen] = useState(false);
  const [contextPresent, setContextPresent] = useState(false);
  const contextTimer = useRef(null);
  const leaveTimer = useRef(null);
  const swipe = useRef(null);
  const suppressPillClick = useRef(false);
  const canSuggest = !!state.shellSettings?.openOnClick && props.contextTools?.length > 0
    && ['idle', 'media'].includes(compactStatus.id)
    && !state.media?.playing && !state.media?.sessions?.some(session => session.playing) && !state.foregroundApp?.fullscreen;
  useEffect(() => {
    if (!canSuggest) setContextOpen(false);
    return () => { window.clearTimeout(contextTimer.current); window.clearTimeout(leaveTimer.current); };
  }, [canSuggest]);
  const contextVisible = contextOpen && canSuggest;
  useLayoutEffect(() => {
    if (contextVisible) {
      setContextPresent(true);
      return;
    }
    if (reduced || !canSuggest) {
      setContextPresent(false);
      return;
    }
    const timer = window.setTimeout(() => setContextPresent(false), 180);
    return () => window.clearTimeout(timer);
  }, [contextVisible, reduced, canSuggest]);
  // Hold the native width until the outgoing controls have faded.
  const contextToolCount = contextPresent && canSuggest ? props.contextTools.length : 0;
  useEffect(() => {
    send('compactContext', { count: contextToolCount });
  }, [contextToolCount]);
  useEffect(() => () => send('compactContext', { count: 0 }), []);
  const latest = useRef(props);
  const [handoff, setHandoff] = useState({ identity, current: props, previous: null, returning: false });
  const immediate = reduced || state.compactFaded || ['recording', 'tool-active', 'agent-approval'].includes(compactStatus.id);
  if (handoff.identity !== identity && (!handoff.previous || immediate)) {
    // Coalesce ordinary updates until the current blend finishes. Critical
    // controls preempt immediately, and never revive a partially faded layer.
    setHandoff({
      identity, current: props,
      previous: reduced || state.compactFaded || handoff.previous ? null : latest.current,
      returning: compactStatus.id === 'idle',
    });
  }
  const current = handoff.identity === identity ? props : handoff.current;
  useLayoutEffect(() => { latest.current = current; });
  useEffect(() => {
    if (!handoff.previous) return;
    if (reduced || state.compactFaded) {
      setHandoff(current => ({ ...current, previous: null }));
      return;
    }
    const timer = setTimeout(() => setHandoff(current => ({ ...current, previous: null })), 280);
    return () => clearTimeout(timer);
  }, [handoff.identity, !!handoff.previous, reduced, state.compactFaded]);
  const privacyCount = Number(!!state.microphone) + Number(!!state.camera);
  const startContextReveal = () => {
    if (!canSuggest) return;
    window.clearTimeout(leaveTimer.current);
    window.clearTimeout(contextTimer.current);
    if (contextPresent) setContextOpen(true);
    else contextTimer.current = window.setTimeout(() => setContextOpen(true), 360);
  };
  const closeContextReveal = () => {
    swipe.current = null;
    suppressPillClick.current = false;
    window.clearTimeout(contextTimer.current);
    window.clearTimeout(leaveTimer.current);
    leaveTimer.current = window.setTimeout(() => setContextOpen(false), 160);
  };
  const onContextPointerDown = event => {
    if (!canSuggest || event.button !== 0 || event.target.closest('button')) return;
    swipe.current = { x: event.clientX, y: event.clientY, opened: false };
  };
  const onContextPointerMove = event => {
    if (!swipe.current || swipe.current.opened) return;
    const dx = event.clientX - swipe.current.x;
    const dy = event.clientY - swipe.current.y;
    if (dx < -30 && Math.abs(dx) > Math.abs(dy) * 1.15) {
      swipe.current.opened = true;
      suppressPillClick.current = true;
      window.clearTimeout(contextTimer.current);
      window.clearTimeout(leaveTimer.current);
      setContextOpen(true);
    }
  };
  const onContextPointerUp = () => {
    if (swipe.current?.opened) window.setTimeout(() => { suppressPillClick.current = false; }, 0);
    swipe.current = null;
  };
  const contextRail = contextPresent && canSuggest && <div className={'compact-context-rail' + (!contextVisible ? ' is-leaving' : '')} inert={!contextVisible} aria-hidden={!contextVisible} role="toolbar" aria-label={`${state.foregroundApp.name} tools`}>
    <button type="button" className="compact-context-app" title="Open Home" onClick={() => send('compactOpen', { page: 'Home' })}>{state.foregroundApp.name}</button>
    {props.contextTools.map(action => <button type="button" key={action.id} title={action.detail} aria-label={`${action.label}: ${action.detail}`} onClick={event => { event.stopPropagation(); setContextOpen(false); props.onRunContextTool(action.id); }}><Icon name={action.icon} size={16} /></button>)}
  </div>;
  return <div className={'compact-transition' + (handoff.returning ? ' is-returning' : '') + (contextOpen && canSuggest ? ' is-context-open' : '')}
    style={{ '--compact-privacy-space': `${privacyCount ? privacyCount * 21 + (privacyCount - 1) * 3 + 8 : 0}px` }}
    onPointerEnter={() => { send('compactPointer', { inside: true }); startContextReveal(); }}
    onPointerLeave={() => { send('compactPointer', { inside: false }); closeContextReveal(); }}
    onPointerDown={onContextPointerDown} onPointerMove={onContextPointerMove} onPointerUp={onContextPointerUp} onPointerCancel={onContextPointerUp}
    onClickCapture={event => { if (suppressPillClick.current) { event.preventDefault(); event.stopPropagation(); suppressPillClick.current = false; } }}>
    {handoff.previous && !reduced && <div key={`out-${handoff.identity}`} className="compact-swap-old" aria-hidden="true" inert={true}><CompactPill {...handoff.previous} /></div>}
    <div key={handoff.identity} inert={handoff.identity !== identity || (contextOpen && canSuggest)} className={handoff.previous && !reduced ? 'compact-swap-current is-arriving' : 'compact-swap-current'}><CompactPill {...current} /></div>
    {contextRail}
    <span className="compact-privacy">
      {state.microphone && <button type="button" className="compact-mic" title="Microphone in use" aria-label="Microphone in use, open Controls" onClick={() => send('compactOpen', { page: 'Controls' })}><Icon name="Mic" size={14} /></button>}
      {state.camera && <button type="button" className="compact-camera" title="Camera in use" aria-label="Camera in use, open Controls" onClick={() => send('compactOpen', { page: 'Controls' })}><Icon name="Camera" size={14} /></button>}
    </span>
  </div>;
}

function CompactPill({ state, compactStatus, screenRecordingAt, screenRecordingElapsed, screenRecordingStopping, batteryView }) {
  const statusId = compactStatus.id;
  const payload = compactStatus.payload || {};
  const media = statusId === 'media' ? payload.media : null;
  const feedback = ['reminder', 'notification', 'tool-active', 'tool-result'].includes(statusId) ? payload.feedback : null;
  const rain = statusId === 'weather' ? payload.rain : null;
  const update = statusId === 'updates' ? payload.update : null;
  const activeRuns = statusId === 'agent' ? payload.activeRuns : 0;
  const activeSession = payload.activeSession;
  const approval = statusId === 'agent-approval' ? payload.approval : null;
  const agentLabel = activeSession?.title?.trim() || 'Agent running';
  const otherAgentCount = Math.max(0, activeRuns - 1);
  const childAgentCount = readAgentPreferences().showChildCount ? activeSession?.childCount || 0 : 0;
  const isRecording = statusId === 'recording' && !!screenRecordingAt;
  const batteryPercent = Math.max(0, Math.min(100, Number(state.batteryPercent) || 0));
  const batteryConnected = state.batteryCharging || state.batteryPlugged;
  const battery = state.batteryEnabled && state.batteryPercent != null && <span className={'compact-battery ' + (batteryConnected ? 'is-plugged ' : '') + (batteryPercent <= 20 ? 'is-low' : '')} aria-label={`Battery ${batteryPercent}%, ${state.batteryCharging ? 'charging' : state.batteryPlugged ? 'plugged in' : 'on battery'}`}>{batteryView.mode !== 'percent' && <BatteryGlyph percent={batteryPercent} charging={state.batteryCharging} />}{batteryView.mode !== 'icon' && <span>{batteryPercent}%</span>}</span>;
  const compactMode = feedback ? `feedback-${feedback.kind}` : statusId;
  const feedbackTitle = feedback?.phase === 'active' ? feedback.label : ({ ocr: feedback?.copy ? 'Text copied' : 'Reading text', qr: feedback?.copy ? 'QR copied' : 'Reading QR', color: 'Color picked', copy: 'Copied', capture: 'Screenshot saved', record: 'Recording saved', reminder: 'Reminder' }[feedback?.kind] || feedback?.label);
  const showFeedbackDetail = feedback && feedback.phase !== 'active' && (feedback.copy || feedback.filePath || feedback.kind !== 'notice');
  const destination = feedback?.kind === 'reminder' ? 'Note' : media ? 'Home' : statusId === 'weather' ? 'Weather' : ['agent', 'agent-approval'].includes(statusId) ? 'Agents' : 'Home';
  const ariaLabel = feedback ? `${feedbackTitle}${feedback.repeatCount > 1 ? `, repeated ${feedback.repeatCount} times` : ''}. Open ${destination}`
    : isRecording ? `Recording ${mediaTime(screenRecordingElapsed * 1000)}. Open Island`
      : approval ? `${approval.provider === 'codex' ? 'Codex' : 'OpenCode'} ${approval.kind === 'question' ? 'question' : 'approval'} needed. Open Agents`
      : update ? `${update.label} update ${update.version} available. ${update.busy ? 'Finish active work first.' : 'Click to update in the background.'}`
      : media ? `${media.title || 'Playing'}. Open Home`
        : statusId === 'agent' ? `${agentLabel}. Open Agents`
          : rain ? `${rain.active ? 'Rain is forecast now' : `Rain expected around ${forecastClock(rain.time)}`}${rain.probability == null ? '' : `, ${rain.probability}% chance`}. Open Weather`
            : 'Open Island';
  const openSelectedStatus = () => {
    if (update) { if (!update.busy) send('systemUpdate', { component: update.id }); return; }
    if (approval) window.islandAgentFocus = { provider: approval.provider, sessionId: approval.sessionId };
    else if (statusId === 'agent' && activeSession?.sessionId) window.islandAgentFocus = { provider: activeSession.provider, sessionId: activeSession.sessionId };
    send('compactOpen', { page: destination });
  };
  return <div data-mode={compactMode} className={'compact-content ' + (feedback ? `compact-feedback feedback-${feedback.kind}` : statusId === 'recording' ? 'compact-recording' : statusId === 'media' ? 'compact-media' : statusId === 'agent' ? 'compact-agent' : statusId === 'agent-approval' ? 'compact-agent-approval' : statusId === 'weather' ? 'compact-weather' : statusId === 'updates' ? 'compact-update' : 'compact-idle')}
    role="button" tabIndex={0} aria-label={ariaLabel}
    onClick={openSelectedStatus}
    onKeyDown={event => { if (event.target === event.currentTarget && (event.key === 'Enter' || event.key === ' ')) { event.preventDefault(); openSelectedStatus(); } }}>
    {feedback ? <><span className={'compact-feedback-mark ' + (feedback.color ? 'is-color' : '')} style={feedback.color ? { '--picked-color': feedback.color } : undefined}>{!feedback.color && <Icon name={{ ocr: 'Text', qr: 'Qr', color: 'ColorPicker', capture: 'Capture', record: 'Record', reminder: 'Reminder', copy: 'Clipboard' }[feedback.kind] || 'Check'} size={16} />}</span><span className="compact-feedback-copy"><strong>{feedbackTitle}{feedback.repeatCount > 1 && <span className="compact-feedback-repeat"> · {feedback.repeatCount}</span>}</strong>{showFeedbackDetail && <small>{feedback.label}</small>}</span>{feedback.copy && <button type="button" className="compact-feedback-action" aria-label="Copy result again" onClick={event => { event.stopPropagation(); send('compactFeedbackCopy', { feedbackId: feedback.id }); }}><Icon name="Copy" size={14} /></button>}{feedback.filePath && <button type="button" className="compact-feedback-action" aria-label={feedback.kind === 'capture' ? 'Copy screenshot image' : 'Copy recording file'} onClick={event => { event.stopPropagation(); send('screenToolFile', { action: feedback.kind === 'capture' ? 'copyImage' : 'copyFile', path: feedback.filePath }); }}><Icon name="Copy" size={14} /></button>}{feedback.filePath && <button type="button" className="compact-feedback-action" aria-label="Open saved file" onClick={event => { event.stopPropagation(); send('screenToolFile', { action: 'open', path: feedback.filePath }); }}><Icon name="File" size={14} /></button>}{feedback.kind === 'reminder' && <button type="button" className="compact-feedback-action compact-reminder-ack" title="Mark reminder noticed" aria-label="Mark reminder noticed" onClick={event => { event.stopPropagation(); send('reminderNoticed', { feedbackId: feedback.id }); }}><Icon name="Check" size={14} /></button>}</>
      : isRecording ? <><span className={'compact-record-mark' + (screenRecordingStopping ? ' is-saving' : '')} aria-hidden="true">{screenRecordingStopping ? <Icon name="Loading" size={14} /> : <span className="compact-record-dot" />}</span><strong className="compact-record-time">{mediaTime(screenRecordingElapsed * 1000)}</strong><span className="compact-record-name">{screenRecordingStopping ? 'Saving' : 'Recording'}</span><button type="button" className="compact-record-stop" disabled={screenRecordingStopping} title="Stop recording" aria-label="Stop recording" onClick={event => { event.stopPropagation(); send('screenToolStop'); }}><span /></button></>
      : approval ? <><span className="compact-agent-mark"><Icon name={approval.kind === 'question' ? 'Bot' : 'Lock'} size={14} /></span><span className="compact-agent-label" title={approval.action}><strong>{approval.kind === 'question' ? 'Question waiting' : 'Approval needed'}</strong></span>{approval.count > 1 && <span className="compact-agent-children" aria-label={`${approval.count} pending decisions`}>{approval.count}</span>}<span className="compact-agent-open"><Icon name="NextMonth" size={14} /></span></>
      : update ? <><span className="compact-update-mark"><Icon name="Refresh" size={14} /></span><span className="compact-update-name" title={`${update.label} ${update.version}`}><strong>{update.label} update</strong></span><span className="compact-update-action">{update.busy ? 'Later' : 'Update'}</span></>
      : media ? <><span className="compact-artwork">{media.artwork ? <img src={media.artwork} alt="" draggable="false" /> : <Icon name="Play" size={14} />}</span><span className="compact-media-name" title={media.title || media.artist || 'Playing'}>{media.title || media.artist || 'Playing'}</span><span className="compact-wave" aria-hidden="true">{[.48, .82, 1, .65, .94, .55].map((weight, index) => <i key={index} style={{ '--weight': weight }} />)}</span></>
      : activeRuns ? <><span className="compact-agent-mark"><Icon name={activeSession?.provider === 'codex' ? 'Agents' : 'Terminal'} size={14} /></span><span className="compact-agent-label" title={agentLabel}><strong>{agentLabel}</strong></span>{otherAgentCount + childAgentCount > 0 && <span className="compact-agent-children" aria-label={`${otherAgentCount} other tasks, ${childAgentCount} child agents`}>+{otherAgentCount + childAgentCount}</span>}<span className="compact-agent-bars" aria-hidden="true"><i /><i /><i /></span></>
      : rain ? <><span className="compact-weather-mark">{weatherGlyph(61, 17)}</span><span className="compact-weather-copy"><strong>{rain.active ? 'Rain now' : 'Rain expected'}</strong><small>{rain.active ? 'In the forecast' : forecastClock(rain.time)}{rain.probability == null ? '' : ` · ${rain.probability}%`}</small></span></>
      : <><span className="compact-idle-leading">{batteryView.side === 'left' && battery}<strong className="compact-clock">{state.clockEnabled ? state.clockLabel : state.status || 'Island'}</strong></span><span className="compact-idle-trailing">{batteryView.side !== 'left' && battery}</span></>}
  </div>;
}

const safeOpenCodeUrl = value => {
  try {
    const parsed = new URL(value);
    return parsed.protocol === 'https:' || parsed.protocol === 'http:' ? parsed.href : '';
  } catch { return ''; }
};

function openCodeReplySources(text) {
  const sources = new Map();
  const add = (label, rawUrl) => {
    const url = safeOpenCodeUrl(rawUrl.replace(/[.,;]+$/, ''));
    if (!url || sources.has(url)) return;
    let host = url;
    try { host = new URL(url).hostname.replace(/^www\./, ''); } catch {}
    sources.set(url, { label: String(label || host).slice(0, 80), url, host });
  };
  for (const line of String(text || '').split(/\r?\n/)) {
    const definition = line.match(/^\s*\[([^\]]+)\]:\s*(https?:\/\/\S+)(?:\s+.*)?$/i);
    if (definition) add(definition[1], definition[2]);
    for (const match of line.matchAll(/\[([^\]]+)\]\((https?:\/\/[^\s)]+)(?:\s+"[^"]*")?\)/gi)) add(match[1], match[2]);
  }
  return [...sources.values()].slice(0, 12);
}

function OpenCodeCodeBlock({ language, code }) {
  const [copied, setCopied] = useState(false);
  useEffect(() => {
    if (!copied) return undefined;
    const timer = setTimeout(() => setCopied(false), 1400);
    return () => clearTimeout(timer);
  }, [copied]);
  return <section className="opencode-code-block" aria-label={`${language || 'Code'} code block`}>
    <div className="opencode-code-head"><span>{language || 'Code'}</span><button type="button" aria-label={copied ? 'Code copied' : 'Copy code'} title={copied ? 'Copied' : 'Copy code'} onClick={() => { send('opencodeCopy', { text: code }); setCopied(true); }}><Icon name={copied ? 'Check' : 'CopyCode'} size={13} /><span>{copied ? 'Copied' : 'Copy'}</span></button></div>
    <pre><code>{code}</code></pre>
  </section>;
}

function OpenCodeImage({ src, alt }) {
  const [failed, setFailed] = useState(false);
  if (failed) return <button type="button" className="opencode-image-fallback" onClick={() => send('openExternalUrl', { url: src })}><Icon name="Link" size={13} />Image unavailable · Open image</button>;
  return <button type="button" className="opencode-image-open" title="Open image" onClick={() => send('openExternalUrl', { url: src })}>
    <img src={src} alt={alt || 'Image from OpenCode'} loading="lazy" decoding="async" onError={() => setFailed(true)} />
  </button>;
}

function renderOpenCodeReply(text) {
  const lines = String(text || '').split(/\r?\n/);
  const references = new Map();
  let imageCount = 0;
  const bodyLines = lines.filter(line => {
    const definition = line.match(/^\s*\[([^\]]+)\]:\s*(https?:\/\/\S+)(?:\s+.*)?$/i);
    if (!definition) return true;
    const url = safeOpenCodeUrl(definition[2].replace(/[.,;]+$/, ''));
    if (url) references.set(definition[1].toLowerCase(), url);
    return false;
  });
  const blocks = [];
  let paragraph = [];
  const emphasis = (value, key) => value.split(/(\*\*[^*\n]+\*\*|__[^_\n]+__)/g).map((part, index) => part.length > 4 && ((part.startsWith('**') && part.endsWith('**')) || (part.startsWith('__') && part.endsWith('__'))) ? <strong key={`${key}-bold-${index}`}>{part.slice(2, -2)}</strong> : <React.Fragment key={`${key}-plain-${index}`}>{part}</React.Fragment>);
  const inline = (line, key) => line.split(/(`[^`]+`|\[[^\]]+\]\(https?:\/\/[^\s)]+(?:\s+"[^"]*")?\)|\[[^\]]+\])/gi).map((part, index) => {
    if (part.startsWith('`') && part.endsWith('`')) return <code key={`${key}-${index}`}>{part.slice(1, -1)}</code>;
    const link = part.match(/^\[([^\]]+)\]\((https?:\/\/[^\s)]+)(?:\s+"([^"]*)")?\)$/i);
    const reference = link ? null : part.match(/^\[([^\]]+)\]$/);
    const url = safeOpenCodeUrl(link?.[2] || (reference ? references.get(reference[1].toLowerCase()) : ''));
    if (url) {
      const label = link?.[1] || reference?.[1] || 'Source';
      return <button type="button" className="opencode-inline-citation" key={`${key}-${index}`} title={link?.[3] || url} onClick={() => send('openExternalUrl', { url })}><Icon name="Link" size={12} /><span>{label}</span></button>;
    }
    return <React.Fragment key={`${key}-${index}`}>{emphasis(part, `${key}-${index}`)}</React.Fragment>;
  });
  const flushParagraph = () => {
    if (!paragraph.length) return;
    const content = paragraph;
    paragraph = [];
    const image = content.length === 1 ? content[0].match(/^\s*!\[([^\]]*)\]\((https?:\/\/[^\s)]+)(?:\s+"([^"]*)")?\)\s*$/i) : null;
    const imageUrl = image && safeOpenCodeUrl(image[2]);
    if (imageUrl && imageCount < 4) {
      imageCount += 1;
      blocks.push(<OpenCodeImage key={`image-${blocks.length}`} src={imageUrl} alt={image[1] || image[3] || 'Image from OpenCode'} dark={dark} />);
      return;
    }
    if (imageUrl) {
      blocks.push(<button type="button" className="opencode-inline-citation" key={`image-link-${blocks.length}`} title={imageUrl} onClick={() => send('openExternalUrl', { url: imageUrl })}><Icon name="Link" size={12} /><span>Open image</span></button>);
      return;
    }
    const heading = content.length === 1 ? content[0].match(/^(#{1,3})\s+(.+)$/) : null;
    if (heading) {
      const Tag = `h${heading[1].length}`;
      blocks.push(<Tag key={`block-${blocks.length}`}>{inline(heading[2], `heading-${blocks.length}`)}</Tag>);
      return;
    }
    if (content.every(line => /^\s*[-*]\s+/.test(line))) {
      blocks.push(<ul key={`block-${blocks.length}`}>{content.map((line, index) => <li key={index}>{inline(line.replace(/^\s*[-*]\s+/, ''), `list-${blocks.length}-${index}`)}</li>)}</ul>);
      return;
    }
    if (content.every(line => /^\s*\d+[.)]\s+/.test(line))) {
      blocks.push(<ol key={`block-${blocks.length}`}>{content.map((line, index) => <li key={index}>{inline(line.replace(/^\s*\d+[.)]\s+/, ''), `list-${blocks.length}-${index}`)}</li>)}</ol>);
      return;
    }
    if (content.every(line => /^\s*>\s?/.test(line))) {
      blocks.push(<blockquote key={`block-${blocks.length}`}>{content.map((line, index) => <p key={index}>{inline(line.replace(/^\s*>\s?/, ''), `quote-${blocks.length}-${index}`)}</p>)}</blockquote>);
      return;
    }
    blocks.push(<p key={`block-${blocks.length}`}>{content.map((line, index) => <React.Fragment key={index}>{index > 0 && <br />}{inline(line, `line-${blocks.length}-${index}`)}</React.Fragment>)}</p>);
  };
  for (let index = 0; index < bodyLines.length; index += 1) {
    const fence = bodyLines[index].match(/^\s*```\s*([^\s`]*)?.*$/);
    if (!fence) {
      if (!bodyLines[index].trim()) flushParagraph();
      else paragraph.push(bodyLines[index]);
      continue;
    }
    flushParagraph();
    const code = [];
    index += 1;
    while (index < bodyLines.length && !/^\s*```/.test(bodyLines[index])) code.push(bodyLines[index++]);
    blocks.push(<OpenCodeCodeBlock language={fence[1] || ''} code={code.join('\n')} key={`code-${blocks.length}`} />);
  }
  flushParagraph();
  return blocks;
}

function OpenCodeSources({ sources }) {
  if (!sources?.length) return null;
  return <section className="opencode-sources" aria-label="Sources"><div className="opencode-sources-heading"><Icon name="Link" size={13} /><strong>Sources</strong><small>{sources.length}</small></div><div className="opencode-source-list">{sources.map(source => <button type="button" key={source.url} title={source.url} onClick={() => send('openExternalUrl', { url: source.url })}><Icon name="Link" size={12} /><span>{source.label}</span><small>{source.host}</small></button>)}</div></section>;
}

function OpenCodeSessionPicker({ open, sessions, selectedId, query, onQuery, onSelect, hasMore, onMore }) {
  return <div className="opencode-session-picker" inert={!open} aria-hidden={!open}>
    <div className="opencode-session-heading"><Icon name="History" size={16} /><strong>Recent sessions</strong><small>{sessions.length} loaded</small></div>
    <input aria-label="Find OpenCode session" placeholder="Find a session" value={query} onChange={event => onQuery(event.target.value)} />
    <div className="opencode-sessions">{sessions.length ? sessions.map(session => <button key={session.id} className={'opencode-session ' + (session.id === selectedId ? 'selected' : '')} onClick={() => onSelect(session)} title={session.directory}><span className={'opencode-dot ' + (session.active ? 'active' : '')} /><span><strong>{session.title}</strong><small>{session.directory || 'No project'}</small></span><small className="opencode-session-state">{session.active ? 'Running' : ''}</small></button>) : <span className="opencode-picker-empty">No matching sessions</span>}{hasMore && <button className="opencode-more" onClick={onMore}><Icon name="ChevronDown" size={14} />Load more</button>}</div>
  </div>;
}

const profileRoles = [
  { value: 'primary', label: 'Primary agent', detail: 'Can lead a session' },
  { value: 'subagent', label: 'Mentionable subagent', detail: 'Call with @ in a prompt' },
  { value: 'all', label: 'Primary and subagent', detail: 'Available in both roles' },
];

function ProfileSelect({ label, value, options, onChange, renderPreview, className = '' }) {
  const [open, setOpen] = useState(false);
  const [activeIndex, setActiveIndex] = useState(0);
  const [menuPosition, setMenuPosition] = useState(null);
  const rootRef = useRef(null);
  const menuRef = useRef(null);
  const optionRefs = useRef([]);
  const selectedIndex = Math.max(0, options.findIndex(option => option.value === value));
  const selected = options[selectedIndex];
  useLayoutEffect(() => {
    if (!open) return undefined;
    const positionMenu = () => {
      const rect = rootRef.current?.getBoundingClientRect();
      if (!rect) return;
      const inset = 10;
      const gap = 5;
      const width = Math.min(Math.max(rect.width, 204), window.innerWidth - inset * 2);
      const wantedHeight = Math.min(options.length * 37 + 10, 210);
      const below = window.innerHeight - rect.bottom - inset - gap;
      const above = rect.top - inset - gap;
      const placeBelow = below >= wantedHeight || below >= above;
      const available = Math.max(40, placeBelow ? below : above);
      setMenuPosition({
        left: Math.max(inset, Math.min(rect.left, window.innerWidth - width - inset)),
        top: placeBelow ? rect.bottom + gap : Math.max(inset, rect.top - gap - Math.min(wantedHeight, available)),
        width,
        maxHeight: Math.min(wantedHeight, available),
      });
    };
    positionMenu();
    window.addEventListener('resize', positionMenu);
    window.addEventListener('scroll', positionMenu, true);
    return () => { window.removeEventListener('resize', positionMenu); window.removeEventListener('scroll', positionMenu, true); };
  }, [open, options.length]);
  useEffect(() => {
    if (!open) return undefined;
    const closeOutside = event => { if (!rootRef.current?.contains(event.target) && !menuRef.current?.contains(event.target)) setOpen(false); };
    const closeEscape = event => {
      if (event.key === 'Escape') { setOpen(false); rootRef.current?.querySelector('button[role="combobox"]')?.focus(); }
    };
    document.addEventListener('pointerdown', closeOutside);
    document.addEventListener('keydown', closeEscape);
    return () => { document.removeEventListener('pointerdown', closeOutside); document.removeEventListener('keydown', closeEscape); };
  }, [open]);
  const focusOption = index => {
    const next = (index + options.length) % options.length;
    setActiveIndex(next);
    requestAnimationFrame(() => optionRefs.current[next]?.focus());
  };
  const choose = option => { onChange(option.value); setOpen(false); };
  return <div className={'opencode-profile-select ' + className} ref={rootRef}>
    <button type="button" className="opencode-profile-select-trigger" role="combobox" aria-label={label} aria-haspopup="listbox" aria-expanded={open}
      onClick={() => { setActiveIndex(selectedIndex); setOpen(current => !current); }}
      onKeyDown={event => {
        if (event.key === 'ArrowDown' || event.key === 'ArrowUp') { event.preventDefault(); setOpen(true); focusOption(selectedIndex + (event.key === 'ArrowUp' ? -1 : 1)); }
        if (event.key === 'Escape') setOpen(false);
      }}>
      <span className="opencode-profile-select-value">{renderPreview?.(selected)}{selected?.label || 'Choose an option'}</span><Icon name="ChevronDown" size={14} />
    </button>
    {open && menuPosition && createPortal(<div className="opencode-profile-select-menu opencode-profile-select-portal" ref={menuRef} role="listbox" aria-label={label} style={menuPosition}>
      {options.map((option, index) => <button type="button" role="option" aria-selected={option.value === value} key={option.value}
        ref={node => { optionRefs.current[index] = node; }} onClick={() => choose(option)}
        onKeyDown={event => {
          if (event.key === 'ArrowDown' || event.key === 'ArrowUp') { event.preventDefault(); focusOption(index + (event.key === 'ArrowUp' ? -1 : 1)); }
          if (event.key === 'Enter' || event.key === ' ') { event.preventDefault(); choose(option); }
          if (event.key === 'Escape') { setOpen(false); rootRef.current?.querySelector('button[role="combobox"]')?.focus(); }
        }}>
        {renderPreview?.(option)}<span><strong>{option.label}</strong>{option.detail && <small>{option.detail}</small>}</span>{option.value === value && <Icon name="Check" size={14} />}
      </button>)}
    </div>, document.querySelector('.island') || document.body)}
  </div>;
}

function OpenCodeAgentManager({ agents = [], models = [], notice = '', error = '', onRefresh, onSave, onDelete }) {
  const [selectedId, setSelectedId] = useState('');
  const [draft, setDraft] = useState(null);
  const [editSection, setEditSection] = useState('details');
  const selected = agents.find(agent => agent.id === selectedId);
  useEffect(() => {
    if (selectedId && !agents.some(agent => agent.id === selectedId)) { setSelectedId(''); setDraft(null); }
  }, [agents, selectedId]);
  const beginNew = () => { setEditSection('details'); setSelectedId(''); setDraft({ id: '', description: '', mode: 'subagent', model: '', system: '' }); };
  const chooseAgent = agent => {
    setEditSection('details');
    setSelectedId(agent.id);
    setDraft({ id: agent.id, description: agent.description || '', mode: agent.mode || 'all', model: agent.model || '', system: agent.system || '' });
  };
  const update = (key, value) => setDraft(current => ({ ...current, [key]: value }));
  const save = event => {
    event.preventDefault();
    if (!draft) return;
    const id = draft.id.trim().toLocaleLowerCase().replace(/\s+/g, '-');
    if (id) {
      onSave({ ...draft, id, description: draft.description.trim(), system: draft.system.trim() });
    }
  };
  return <section className="opencode-agent-manager" aria-label="OpenCode agent profiles">
    <aside className="opencode-profile-roster">
      <div className="opencode-profile-roster-head"><strong>Profiles</strong><button type="button" onClick={onRefresh} title="Refresh agents" aria-label="Refresh agents"><Icon name="Refresh" size={14} /></button></div>
      <button type="button" className="opencode-profile-new" onClick={beginNew}><Icon name="Add" size={14} /><span>New profile</span></button>
      <div className="opencode-profile-list">{agents.filter(agent => !agent.hidden).map(agent => <button type="button" key={agent.id} className={'opencode-profile-row ' + (selectedId === agent.id ? 'selected' : '')} onClick={() => chooseAgent(agent)}>
        <span className="opencode-profile-row-icon" aria-hidden="true"><Icon name="Agents" size={14} /></span><span><strong>{agent.id}</strong><small>{agent.mode === 'subagent' ? 'Subagent' : agent.mode === 'primary' ? 'Primary' : 'All roles'}{agent.managed ? ' · Island profile' : ''}</small></span>
      </button>)}{!agents.some(agent => !agent.hidden) && <p className="opencode-profile-empty">No agents were returned by OpenCode.</p>}</div>
      {(error || (notice && notice !== 'Task sent to OpenCode')) && <p className={'opencode-profile-message ' + (error ? 'is-error' : '')} role={error ? 'alert' : 'status'}>{error || notice}</p>}
    </aside>
    {draft ? <form className="opencode-profile-editor" onSubmit={save}>
      <div className="opencode-profile-editor-head"><button type="button" className="opencode-profile-back" onClick={() => { setDraft(null); setSelectedId(''); }}>Profiles</button><h3>{!selectedId ? 'New agent' : selected?.managed ? `Edit ${selected.id}` : selected?.id}</h3></div>
      {!selectedId || selected?.managed ? <>
        <div className="opencode-profile-sections" role="tablist" aria-label="Profile settings">
          {[['details', 'Details'], ['instructions', 'Instructions']].map(([section, label]) => <button key={section} type="button" role="tab" aria-selected={editSection === section} onClick={() => setEditSection(section)}>{label}</button>)}
        </div>
        {editSection === 'details' && <div className="opencode-profile-fields">
          <label className="opencode-profile-field"><span>Name</span><input required disabled={!!selected?.managed} maxLength={48} pattern="[A-Za-z0-9_-]+" autoComplete="off" value={draft.id} onChange={event => update('id', event.target.value.toLocaleLowerCase().replace(/[^a-z0-9_-]/g, '-'))} placeholder="reviewer" /></label>
          <div className="opencode-profile-field"><span>Role</span><ProfileSelect label="Agent role" value={draft.mode} options={profileRoles} onChange={value => update('mode', value)} /></div>
          <label className="opencode-profile-field"><span>Description</span><input maxLength={500} value={draft.description} onChange={event => update('description', event.target.value)} placeholder="What this agent is good at" /></label>
          <div className="opencode-profile-field"><span>Preferred model</span><ProfileSelect label="Preferred model" value={draft.model} options={[{ value: '', label: 'Use session model', detail: 'Inherit the model selected for the session' }, ...(draft.model && !models.some(model => model.key === draft.model) ? [{ value: draft.model, label: draft.model, detail: 'Unavailable in the current catalog' }] : []), ...models.map(model => ({ value: model.key, label: model.name, detail: model.provider }))]} onChange={value => update('model', value)} /></div>
        </div>}
        {editSection === 'instructions' && <label className="opencode-profile-field opencode-profile-instructions"><span>Instructions</span><textarea maxLength={24000} value={draft.system} onChange={event => update('system', event.target.value)} placeholder="Describe this agent’s focus, approach, and boundaries…" /></label>}
        <div className="opencode-profile-actions"><button type="submit" disabled={!draft.id.trim() || (!draft.description.trim() && !draft.system.trim())}>Save profile</button>{selected?.managed && <button type="button" className="opencode-profile-delete" onClick={() => onDelete(selected.id)} aria-label={`Delete ${selected.id} profile`} title="Delete profile"><Icon name="Trash" size={15} /></button>}</div>
      </> : <div className="opencode-profile-readonly"><strong>{selected?.description || 'No description supplied.'}</strong><p>This profile is managed by OpenCode or another tool. Island leaves its file untouched. Create a new profile to add an Island-managed agent.</p><button type="button" onClick={() => { setDraft({ ...draft, id: `${draft.id}-copy`, system: '' }); setSelectedId(''); }}>Use as a template</button></div>}
    </form> : <div className="opencode-profile-placeholder"><span className="opencode-profile-placeholder-mark"><Icon name="Agents" size={25} /></span><strong>Select or create a profile</strong>{error && <small>{error}</small>}</div>}
  </section>;
}

function OpenCodeTaskSteps({ items, label }) {
  const current = items.findIndex(item => item.status === 'active');
  const failed = items.findIndex(item => item.status === 'error');
  const done = items.filter(item => item.status === 'complete').length;
  const spoken = failed >= 0
    ? `Failed at ${items[failed].label}`
    : current < 0 && items.length > 0 && done === items.length
      ? `All ${items.length} steps complete`
      : current >= 0
        ? `${items[current].label}, step ${current + 1} of ${items.length}`
        : '';
  return <>
    <ol className="opencode-task-steps" aria-label={label}>
      {items.map((item, index) => {
        const status = item.status === 'complete' ? 'complete' : item.status === 'active' ? 'active' : item.status === 'error' ? 'error' : item.status === 'cancelled' ? 'cancelled' : 'pending';
        const stateName = status === 'complete' ? 'Done' : status === 'active' ? 'Running' : status === 'error' ? 'Failed' : status === 'cancelled' ? 'Skipped' : 'Queued';
        return <li key={item.id} className={`is-${status}`} aria-current={status === 'active' ? 'step' : undefined} aria-label={`${stateName}: ${item.label}`}>
          <span className="opencode-task-marker" aria-hidden="true"><i /></span>
          {status === 'active' ? <span className="opencode-task-label is-shimmering">{item.label}</span> : <span className="opencode-task-label">{item.label}</span>}
        </li>;
      })}
    </ol>
    <span className="sr-only" role="status" aria-live="polite">{spoken}</span>
  </>;
}

function OpenCodeProgress({ plan = [], steps = [], active = false, waiting = false, dark = true, visible = false }) {
  const [manualOpen, setManualOpen] = useState(null);
  if (!visible || (!plan.length && !steps.length && !active && !waiting)) return null;
  const expanded = manualOpen ?? (active || waiting);
  const complete = plan.filter(item => item.status === 'complete').length;
  const activeStep = steps.find(step => step.status === 'active');
  const activeTask = plan.find(item => item.status === 'active');
  const hasActivity = plan.length > 0 || steps.length > 0;
  const summary = [steps.length ? `${steps.length} ${steps.length === 1 ? 'tool' : 'tools'}` : '', plan.length ? `${complete}/${plan.length} steps` : ''].filter(Boolean).join(' · ');
  const phase = activeStep?.title || activeTask?.content || (waiting ? 'Planning the next step' : active ? 'Working on your task' : 'Work complete');
  const detail = activeStep ? 'Using a tool' : activeTask ? 'Following the plan' : hasActivity ? summary : waiting || active ? 'Waiting for the first activity update' : '';
  return <section className={'opencode-progress ' + (active ? 'is-active' : waiting ? 'is-waiting' : '')} aria-label="OpenCode plan and activity">
    <div className="opencode-phase"><span className={'opencode-agent-orb ' + (!active && !waiting ? 'is-finished' : '')} aria-hidden="true" /><div className="opencode-phase-copy" role="status" aria-live="polite"><strong>{phase}</strong>{detail && <small>{detail}</small>}</div>
      {hasActivity && <button className="opencode-progress-toggle" type="button" aria-expanded={expanded} aria-label={`${expanded ? 'Hide' : 'Show'} OpenCode activity`} onClick={() => setManualOpen(!expanded)}><span>{summary}</span><Icon name="ChevronDown" size={14} /></button>}
    </div>
    {hasActivity && <div className={'opencode-progress-details-shell ' + (expanded ? 'is-open' : '')} aria-hidden={!expanded} inert={!expanded}><div className="opencode-progress-content">
      {!!plan.length && <div className="opencode-activity-group"><div className="opencode-activity-label"><span>PLAN</span><small>{complete} of {plan.length} complete</small></div><OpenCodeTaskSteps label="Task progress" items={plan.map(item => ({ id: item.id, label: item.content, status: item.status }))} /></div>}
      {!!steps.length && <div className="opencode-activity-group"><div className="opencode-activity-label"><span>TOOL USAGE</span><small>{steps.length} {steps.length === 1 ? 'step' : 'steps'}</small></div><OpenCodeTaskSteps label="OpenCode tool activity" items={steps.map(step => ({ id: step.id, label: step.detail && step.status === 'error' ? `${step.title} · ${step.detail}` : step.title, status: step.status }))} /></div>}
    </div></div>}
  </section>;
}

const preview = value => value?.replace(/\s+/g, ' ').trim() || '';
const futureAction = label => <button className="future-action" key={label} disabled title={`${label} is not connected yet`}><span>{label}</span></button>;
// Original 13×13 dot glyphs: a crisp Nothing-inspired matrix.
// Official Nothing Weather marks are proprietary, so these are drawn for Island.
const weatherDotPatterns = {
  sun: ['......A......', '......A......', '..A.......A..', '...A.....A...', '......A......', '.....AAA.....', 'AA..AAAAA..AA', '.....AAA.....', '......A......', '...A.....A...', '..A.......A..', '......A......', '......A......'],
  moon: ['.............', '....#####....', '...####......', '..####.......', '.####........', '.###.........', '.###.........', '.###.........', '.####........', '..####.......', '...####......', '....#####....', '.............'],
  partly: ['........A....', '......A.A.A..', '.......AAA...', '.....###AA.A.', '....#####A...', '..#########..', '..##########.', '.###########.', '.###########.', '..#########..', '.............', '.............', '.............'],
  partlyNight: ['.......AA....', '......AA.....', '.....AAA.....', '.....###.....', '....#####....', '..#########..', '..##########.', '.###########.', '.###########.', '..#########..', '.............', '.............', '.............'],
  cloud: ['.............', '.............', '......#......', '....#####....', '...#######...', '..#########..', '.###########.', '.###########.', '..#########..', '.............', '.............', '.............', '.............'],
  fog: ['.............', '.............', '......#......', '....#####....', '...#######...', '..#########..', '.###########.', '.###########.', '..#########..', '..A.A.A.A.A..', '.............', '...A.A.A.A.A.', '.............'],
  rain: ['.............', '.............', '......#......', '....#####....', '...#######...', '..#########..', '.###########.', '.###########.', '..#########..', '...A..A..A...', '....A.A...A..', '....A..A..A..', '.............'],
  snow: ['.............', '.............', '......#......', '....#####....', '...#######...', '..#########..', '.###########.', '.###########.', '..#########..', '...A.....A...', '..AAA...AAA..', '...A.....A...', '...A.....A...'],
  storm: ['.............', '.............', '......#......', '....#####....', '...#######...', '..#########..', '.###########.', '.###########.', '..#####A###..', '......AA.....', '.....AA......', '....AA.......', '.............'],
};
const weatherDotPoints = Object.fromEntries(Object.entries(weatherDotPatterns).map(([kind, rows]) => [kind,
  rows.flatMap((row, y) => [...row].flatMap((dot, x) => dot === '#' || dot === 'A' ? [[x, y, dot === 'A']] : [])),
]));
const weatherGlyph = (code, size = 20, isDay = true) => {
  const kind = code == null ? 'cloud'
    : code === 0 ? (isDay ? 'sun' : 'moon')
      : code <= 3 ? (isDay ? 'partly' : 'partlyNight')
        : code <= 48 ? 'fog'
          : code <= 67 ? 'rain'
            : code <= 77 ? 'snow'
              : code <= 86 ? 'rain' : 'storm';
  const points = weatherDotPoints[kind];
  return <svg className={'wx-glyph wx-glyph-' + kind} width={size} height={size} viewBox="0 0 130 130" aria-hidden="true" focusable="false">
    {points.map(([x, y, accent]) => <circle key={`${x}-${y}`} className={accent ? 'wx-glyph-accent' : undefined} cx={5 + x * 10} cy={5 + y * 10} r="3.25" />)}
  </svg>;
};
const weatherName = code => code == null ? 'Forecast' : code === 0 ? 'Clear sky' : code <= 3 ? 'Partly cloudy' : code <= 48 ? 'Cloudy' : code <= 67 ? 'Rain' : code <= 77 ? 'Snow' : code <= 86 ? 'Showers' : 'Thunderstorm';
const weatherWallTime = value => {
  const match = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})/.exec(value ?? '');
  return match ? Date.UTC(Number(match[1]), Number(match[2]) - 1, Number(match[3]), Number(match[4]), Number(match[5])) : NaN;
};
const weatherWallNow = (timestamp, timezone) => {
  if (!timezone) return timestamp;
  try {
    const parts = new Intl.DateTimeFormat('en-CA', {
      timeZone: timezone, year: 'numeric', month: '2-digit', day: '2-digit',
      hour: '2-digit', minute: '2-digit', second: '2-digit', hourCycle: 'h23',
    }).formatToParts(new Date(timestamp));
    const values = Object.fromEntries(parts.map(part => [part.type, part.value]));
    return Date.UTC(Number(values.year), Number(values.month) - 1, Number(values.day), Number(values.hour), Number(values.minute), Number(values.second));
  } catch { return timestamp; }
};
const forecastClock = value => {
  const timestamp = weatherWallTime(value);
  return Number.isFinite(timestamp)
    ? new Date(timestamp).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit', timeZone: 'UTC' })
    : null;
};
const upcomingRain = (forecast, nowMs = Date.now()) => {
  const hourly = forecast?.hourly;
  const times = hourly?.time;
  const now = weatherWallNow(nowMs, forecast?.timezone);
  if (!Array.isArray(times) || !Number.isFinite(now)) return null;
  const horizon = now + 6 * 60 * 60 * 1000;
  const rainAt = position => {
    const code = Number(hourly.weather_code?.[position]);
    const isRain = (code >= 51 && code <= 67) || (code >= 80 && code <= 82) || (code >= 95 && code <= 99);
    if (!isRain) return false;
    const probability = hourly.precipitation_probability?.[position];
    const amount = hourly.precipitation?.[position];
    return (Number.isFinite(probability) && probability >= 40) || (Number.isFinite(amount) && amount >= 0.1);
  };
  const timestamps = times.map(weatherWallTime);
  const index = timestamps.findIndex((timestamp, position) =>
    Number.isFinite(timestamp) && timestamp <= horizon && timestamp + 60 * 60 * 1000 > now && rainAt(position));
  if (index < 0) return null;
  const start = timestamps[index];
  let end = start + 60 * 60 * 1000;
  for (let position = index + 1; position < times.length; position++) {
    const timestamp = timestamps[position];
    if (!Number.isFinite(timestamp) || timestamp > end + 5 * 60 * 1000 || !rainAt(position)) break;
    end = Math.max(end, timestamp + 60 * 60 * 1000);
  }
  if (now >= end) return null;
  const probability = hourly.precipitation_probability?.[index];
  return { time: times[index], probability: Number.isFinite(probability) ? probability : null, active: now >= start, endsAt: end };
};
const localTime = value => value ? new Date(value).toLocaleTimeString([], { hour: 'numeric' }) : '';
const localDay = value => value ? new Date(`${value}T12:00:00`).toLocaleDateString([], { weekday: 'short' }) : '';
const mediaTime = milliseconds => {
  if (milliseconds == null) return '–:––';
  const seconds = Math.floor(milliseconds / 1000);
  const minutes = Math.floor(seconds / 60);
  return minutes >= 60
    ? `${Math.floor(minutes / 60)}:${String(minutes % 60).padStart(2, '0')}:${String(seconds % 60).padStart(2, '0')}`
    : `${minutes}:${String(seconds % 60).padStart(2, '0')}`;
};
const mediaSourceName = source => {
  const id = (source || '').toLowerCase();
  const compactId = id.replace(/[^a-z0-9]/g, '');
  if (id.includes('spotify')) return 'Spotify';
  if (id.includes('zunemusic')) return 'Music';
  if (id.includes('chrome')) return 'Chrome';
  if (id.includes('msedge') || id.includes('microsoftedge')) return 'Edge';
  if (id.includes('firefox')) return 'Firefox';
  if (id.includes('brave')) return 'Brave';
  // Helium's Windows media session can expose only the package publisher ID
  // (the same suffix used by its installed AUMID), without the app name.
  if (id.includes('helium') || compactId.includes('j6r2fcfqkp2dxe33fqapcoblym')) return 'Helium';
  if (id.includes('vivaldi')) return 'Vivaldi';
  if (id.includes('vlc')) return 'VLC';
  if (id.includes('youtube')) return 'YouTube';
  if (id.includes('netflix')) return 'Netflix';
  const appId = (source || '').split('!').at(-1).split(/[\\/]/).at(-1).replace(/_[^_]+$/, '');
  const readable = appId.split(/[._-]+/).filter(Boolean).at(-1)?.replace(/([a-z0-9])([A-Z])/g, '$1 $2');
  return readable || 'Media';
};

function MediaSessionSwitcher({ sessions = [], selectedId, onSelect, reducedMotion = false }) {
  const [open, setOpen] = useState(false);
  const [present, setPresent] = useState(false);
  const [activeIndex, setActiveIndex] = useState(0);
  const rootRef = useRef(null);
  const panelRef = useRef(null);
  const optionRefs = useRef([]);
  const selectedIndex = Math.max(0, sessions.findIndex(session => session.id === selectedId));
  useLayoutEffect(() => {
    if (open) { setPresent(true); return; }
    if (reducedMotion) { setPresent(false); return; }
    const timer = window.setTimeout(() => setPresent(false), 180);
    return () => window.clearTimeout(timer);
  }, [open, reducedMotion]);
  useEffect(() => {
    if (sessions.length < 2) setOpen(false);
    setActiveIndex(Math.max(0, sessions.findIndex(session => session.id === selectedId)));
  }, [sessions.length, selectedId]);
  useEffect(() => {
    if (!open) return undefined;
    const closeOutside = event => {
      if (!rootRef.current?.contains(event.target) && !panelRef.current?.contains(event.target)) setOpen(false);
    };
    const closeEscape = event => {
      if (event.key === 'Escape') {
        setOpen(false);
        rootRef.current?.querySelector('button')?.focus();
      }
    };
    document.addEventListener('pointerdown', closeOutside);
    document.addEventListener('keydown', closeEscape);
    return () => { document.removeEventListener('pointerdown', closeOutside); document.removeEventListener('keydown', closeEscape); };
  }, [open]);
  useEffect(() => {
    if (!present) return;
    const card = rootRef.current?.closest('.home-primary');
    const content = [...(card?.children || [])].filter(child => !child.classList.contains('media-session-panel'));
    content.forEach(child => { child.inert = true; });
    const frame = requestAnimationFrame(() => panelRef.current?.querySelector('.media-session-close')?.focus());
    return () => {
      cancelAnimationFrame(frame);
      content.forEach(child => { child.inert = false; });
      rootRef.current?.querySelector('button')?.focus({ preventScroll: true });
    };
  }, [present]);
  if (sessions.length < 2) return null;
  const focusOption = index => {
    const next = (index + sessions.length) % sessions.length;
    setActiveIndex(next);
    requestAnimationFrame(() => optionRefs.current[next]?.focus());
  };
  const choose = session => {
    onSelect(session.id);
    setOpen(false);
    requestAnimationFrame(() => rootRef.current?.querySelector('button')?.focus());
  };
  const panelTarget = rootRef.current?.closest('.home-primary');
  return <>
    <div className="media-session-picker" ref={rootRef}>
    <button type="button" className="media-session-trigger" aria-label={`Choose media source, ${sessions.length} sessions`} aria-controls="media-session-options" aria-expanded={open} title="Choose media source"
      onClick={() => { setActiveIndex(selectedIndex); setOpen(current => !current); }}
      onKeyDown={event => {
        if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
          event.preventDefault();
          setOpen(true);
          focusOption(selectedIndex + (event.key === 'ArrowUp' ? -1 : 1));
        }
      }}>
      <Icon name="MediaQueue" size={14} /><span>{sessions.length}</span>
    </button>
    </div>
    {present && panelTarget && createPortal(<section className={`media-session-panel ${reducedMotion ? 'reduce-motion' : ''} ${!open ? 'is-closing' : ''}`} ref={panelRef} inert={!open} aria-hidden={!open} aria-label="Media source selection">
      <header className="media-session-panel-heading"><span className="media-session-panel-icon"><Icon name="MediaQueue" size={15} /></span><div><strong>Media sources</strong><small>{sessions.length} available</small></div><button className="media-session-close" type="button" aria-label="Close media source selection" title="Close" onClick={() => { setOpen(false); requestAnimationFrame(() => rootRef.current?.querySelector('button')?.focus()); }}><Icon name="Cancel" size={14} /></button></header>
      <div id="media-session-options" className="media-session-options" role="listbox" aria-label="Available media sources">
        {sessions.map((session, index) => <button type="button" className={'media-session-option ' + (session.id === selectedId ? 'is-selected' : '')} role="option" aria-selected={session.id === selectedId} key={session.id}
          tabIndex={activeIndex === index ? 0 : -1} ref={node => { optionRefs.current[index] = node; }} onClick={() => choose(session)}
          onKeyDown={event => {
            if (event.key === 'ArrowDown' || event.key === 'ArrowUp') { event.preventDefault(); focusOption(index + (event.key === 'ArrowUp' ? -1 : 1)); }
            if (event.key === 'Enter' || event.key === ' ') { event.preventDefault(); choose(session); }
            if (event.key === 'Escape') { setOpen(false); rootRef.current?.querySelector('button')?.focus(); }
          }}>
          <span className="media-session-option-icon"><Icon name="MediaQueue" size={15} /><i className={'media-session-status ' + (session.playing ? 'is-playing' : '')} aria-hidden="true" /></span>
          <span className="media-session-option-copy"><strong>{mediaSourceName(session.source)}</strong><small>{session.playing ? 'Playing' : 'Paused'}</small></span>
          {session.id === selectedId && <Icon name="Check" size={14} />}
        </button>)}
      </div>
    </section>, panelTarget)}
  </>;
}

function ReminderCalendar({ initialAt, backLabel = 'Reminders', onSet, onClose }) {
  const calendarRef = useRef(null);
  const [selected, setSelected] = useState(() => {
    const hasFutureReminder = Number.isFinite(initialAt) && initialAt * 1000 > Date.now();
    const timestamp = hasFutureReminder
      ? initialAt * 1000
      : Date.now() + 60 * 60 * 1000;
    const date = new Date(timestamp);
    date.setSeconds(0, 0);
    if (!hasFutureReminder) date.setMinutes(Math.ceil(date.getMinutes() / 5) * 5);
    return date;
  });
  const [month, setMonth] = useState(() => new Date(selected.getFullYear(), selected.getMonth(), 1));
  const [monthDirection, setMonthDirection] = useState('next');
  const today = new Date();
  const todayStart = new Date(today.getFullYear(), today.getMonth(), today.getDate());
  const firstDay = new Date(month.getFullYear(), month.getMonth(), 1).getDay();
  const monthKey = `${month.getFullYear()}-${month.getMonth()}`;
  const selectedDate = selected.toLocaleDateString([], { weekday: 'long', month: 'long', day: 'numeric' });
  const selectedTime = selected.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' });
  const isSameDay = (a, b) => a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth() && a.getDate() === b.getDate();
  const dayKey = date => `${date.getFullYear()}-${date.getMonth()}-${date.getDate()}`;
  const moveMonth = amount => {
    setMonthDirection(amount > 0 ? 'next' : 'previous');
    setMonth(value => new Date(value.getFullYear(), value.getMonth() + amount, 1));
  };
  const choose = date => setSelected(previous => new Date(date.getFullYear(), date.getMonth(), date.getDate(), previous.getHours(), previous.getMinutes()));
  const stepTime = (part, amount) => setSelected(previous => new Date(previous.getFullYear(), previous.getMonth(), previous.getDate(), previous.getHours() + (part === 'hour' ? amount : 0), previous.getMinutes() + (part === 'minute' ? amount : 0)));
  const dayCells = Array.from({ length: 42 }, (_, index) => new Date(month.getFullYear(), month.getMonth(), index - firstDay + 1));
  const focusDate = dayCells.find(date => isSameDay(selected, date)) || dayCells.find(date => date >= todayStart);
  const atCurrentMonth = month.getFullYear() === today.getFullYear() && month.getMonth() === today.getMonth();
  useEffect(() => { calendarRef.current?.querySelector('.calendar-day[tabindex="0"]')?.focus(); }, []);
  return <section ref={calendarRef} className="reminder-calendar-page" aria-labelledby="reminder-calendar-title" onKeyDown={event => { if (event.key === 'Escape') { event.preventDefault(); onClose(); } }}>
    <NestedPageHeader title="Schedule reminder" icon="Calendar" onBack={onClose} backLabel={backLabel} titleId="reminder-calendar-title" className="reminder-calendar-heading" />
    <div className="reminder-calendar-layout">
      <section className="reminder-calendar-month" aria-label="Choose a date">
        <div className="calendar-top"><button type="button" aria-label="Previous month" disabled={atCurrentMonth} onClick={() => moveMonth(-1)}><Icon name="PreviousMonth" size={16} /></button><strong aria-live="polite">{month.toLocaleDateString([], { month: 'long', year: 'numeric' })}</strong><button type="button" aria-label="Next month" onClick={() => moveMonth(1)}><Icon name="NextMonth" size={16} /></button></div>
        <div className={`calendar-month-grid is-${monthDirection}`} key={monthKey}>
          <div className="calendar-grid" role="grid" aria-label="Calendar">
            <div className="calendar-week calendar-weekdays" role="row">{['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday'].map((day, index) => <span className="calendar-weekday" key={index} role="columnheader" aria-label={day}>{day.slice(0, 1)}</span>)}</div>
            {Array.from({ length: 6 }, (_, week) => <div className="calendar-week" role="row" key={`week-${week}`}>
              {dayCells.slice(week * 7, week * 7 + 7).map(date => {
                const isSelected = isSameDay(selected, date);
                const isToday = isSameDay(todayStart, date);
                const outsideMonth = date.getMonth() !== month.getMonth();
                return <div key={dayKey(date)} className="calendar-day-cell" role="gridcell" aria-selected={isSelected}>
                  <button type="button" data-date={dayKey(date)} className={`calendar-day ${isSelected ? 'selected ' : ''}${isToday ? 'is-today ' : ''}${outsideMonth ? 'is-outside-month' : ''}`} disabled={date < todayStart} aria-label={date.toLocaleDateString([], { dateStyle: 'full' })} aria-current={isToday ? 'date' : undefined} tabIndex={isSameDay(focusDate, date) ? 0 : -1} onKeyDown={event => {
                    const offset = { ArrowLeft: -1, ArrowRight: 1, ArrowUp: -7, ArrowDown: 7 }[event.key];
                    if (!offset) return;
                    const next = new Date(date.getFullYear(), date.getMonth(), date.getDate() + offset);
                    if (next < todayStart) return;
                    event.preventDefault();
                    choose(next);
                    const nextMonth = next.getFullYear() !== month.getFullYear() || next.getMonth() !== month.getMonth();
                    if (nextMonth) setMonth(new Date(next.getFullYear(), next.getMonth(), 1));
                    const calendar = event.currentTarget.closest('.reminder-calendar-page');
                    requestAnimationFrame(() => calendar?.querySelector(`[data-date="${dayKey(next)}"]`)?.focus());
                  }} onClick={() => { choose(date); if (outsideMonth) setMonth(new Date(date.getFullYear(), date.getMonth(), 1)); }}>{date.getDate()}</button>
                </div>;
              })}
            </div>)}
          </div>
        </div>
      </section>
      <section className="calendar-time-panel" aria-labelledby="calendar-time-title">
        <div className="calendar-time-heading"><span className="calendar-time-icon"><Icon name="ReminderTime" size={16} /></span><div><strong id="calendar-time-title">Time</strong><small>Local time</small></div></div>
        <div className="calendar-time-controls" role="group" aria-label="Reminder time">
          <div className="calendar-time-wheel"><span>Hour</span><div role="group" aria-label="Hour controls"><button type="button" aria-label="Decrease hour" onClick={() => stepTime('hour', -1)}>−</button><output aria-label="Hour" aria-live="polite"><span key={selected.getHours()}>{String(selected.getHours()).padStart(2, '0')}</span></output><button type="button" aria-label="Increase hour" onClick={() => stepTime('hour', 1)}>+</button></div></div>
          <div className="calendar-time-wheel"><span>Minute</span><div role="group" aria-label="Minute controls"><button type="button" aria-label="Decrease minute by five" onClick={() => stepTime('minute', -5)}>−</button><output aria-label="Minute" aria-live="polite"><span key={selected.getMinutes()}>{String(selected.getMinutes()).padStart(2, '0')}</span></output><button type="button" aria-label="Increase minute by five" onClick={() => stepTime('minute', 5)}>+</button></div></div>
        </div>
      </section>
    </div>
    <footer className="calendar-footer"><div className="calendar-selection-summary" aria-live="polite"><Icon name="Calendar" size={16} /><strong>{selectedDate}</strong><span>{selectedTime}</span></div><button type="button" className="calendar-save" disabled={selected.getTime() <= Date.now()} onClick={() => onSet(Math.floor(selected.getTime() / 1000))}>Set reminder</button></footer>
  </section>;
}

function SettingsToggle({ label, value, onChange, disabled = false }) {
  return <div className="settings-line"><span>{label}</span><button type="button" role="switch" aria-label={label} aria-checked={!!value} disabled={disabled} className={'settings-switch ' + (value ? 'on' : '')} onClick={onChange}><i /></button></div>;
}

function SettingsSegments({ value, options, onChange, className = '' }) {
  return <div className={'settings-segments ' + className}>{options.map(([option, label]) => <button key={option} type="button" className={value === option ? 'selected' : ''} aria-pressed={value === option} onClick={() => onChange(option)}>{label}</button>)}</div>;
}

function SettingsCard({ icon, title, detail, children, className = '' }) {
  return <section className={'settings-card ' + className}><div className="settings-card-heading"><Icon name={icon} size={17} /><div><strong>{title}</strong></div></div>{children}</section>;
}

function SettingsPanel({ state, batteryView, setBatteryView, accentGradient, setAccentGradient, mediaCardStyle, setMediaCardStyle, homeActions, setHomeActions, compactPriorityOrder, setCompactPriorityOrder, section, setSection, sectionScrollPositions }) {
  const speech = state.speechSettings || {};
  const [voicePreferences, setVoicePreferences] = useState(readAgentPreferences);
  const changeVoicePreference = patch => { const next = { ...readAgentPreferences(), ...patch }; setVoicePreferences(next); saveAgentPreferences(next); };
  const [wakePhrase, setWakePhrase] = useState(() => readAgentPreferences().wakePhrase || 'Hey Island');
  const [askInstructions, setAskInstructions] = useState(() => readAgentPreferences().askInstructions);
  const wake = state.wakeWord || {};
  const wakeOn = ['arming', 'listening', 'triggered', 'paused'].includes(wake.status);
  const conversion = state.conversion || {};
  const [conversionDefaults, setConversionDefaults] = useState(readConversionDefaults);
  const [confirmEngineRemoval, setConfirmEngineRemoval] = useState(false);
  const [conversionEditor, setConversionEditor] = useState('engine');
  const [speechEditor, setSpeechEditor] = useState('provider');
  const updateConversionDefaults = patch => setConversionDefaults(previous => {
    const next = { ...previous, ...patch }; saveConversionDefaults(next); return next;
  });
  const settingsContentRef = useRef(null);
  const [agentSettingsDetail, setAgentSettingsDetail] = useState(false);
  const [provider, setProvider] = useState(() => speech.baseUrl?.includes('api.groq.com') ? 'groq' : speech.baseUrl?.includes('generativelanguage.googleapis.com') ? 'gemini' : speech.baseUrl && !speech.baseUrl.includes('api.openai.com') ? 'custom' : 'openai');
  const [baseUrl, setBaseUrl] = useState(speech.baseUrl || 'https://api.openai.com/v1');
  const [model, setModel] = useState(speech.model || 'gpt-4o-mini-transcribe');
  const [language, setLanguage] = useState(speech.language || '');
  const [key, setKey] = useState('');
  const [message, setMessage] = useState('');
  const [confirmSpeechKeyRemoval, setConfirmSpeechKeyRemoval] = useState(false);
  const speechAction = useRef('save');
  const speechKeyConfirmTimer = useRef(null);
  const speechKeyConfirmMessage = 'Click again within 5 seconds to remove the saved key.';
  useEffect(() => {
    const receive = result => {
      if (result?.error) { setMessage(result.error); return; }
      setKey('');
      setMessage(speechAction.current === 'remove' ? 'API key removed.' : 'Settings saved.');
    };
    window.islandSpeechSettingsResult = receive;
    return () => {
      if (window.islandSpeechSettingsResult === receive) delete window.islandSpeechSettingsResult;
      window.clearTimeout(speechKeyConfirmTimer.current);
    };
  }, []);
  const cancelSpeechKeyRemoval = () => {
    window.clearTimeout(speechKeyConfirmTimer.current);
    speechKeyConfirmTimer.current = null;
    setConfirmSpeechKeyRemoval(false);
    setMessage(current => current === speechKeyConfirmMessage ? '' : current);
  };
  const chooseProvider = next => {
    cancelSpeechKeyRemoval();
    setProvider(next);
    setBaseUrl(next === 'openai' ? 'https://api.openai.com/v1' : next === 'groq' ? 'https://api.groq.com/openai/v1' : next === 'gemini' ? 'https://generativelanguage.googleapis.com/v1beta/openai' : 'http://127.0.0.1:8000/v1');
    setModel(next === 'openai' ? 'gpt-4o-mini-transcribe' : next === 'groq' ? 'whisper-large-v3-turbo' : next === 'gemini' ? 'gemini-3.8-flash' : 'whisper-1');
    setKey('');
    setMessage('');
  };
  const saveSpeech = () => {
    cancelSpeechKeyRemoval();
    speechAction.current = 'save';
    setMessage('Saving…');
    send('speechSettingsSave', { baseUrl, model, language, apiKey: key });
  };
  const removeSpeechKey = () => {
    speechAction.current = 'remove';
    setMessage('Removing key…');
    send('speechSettingsRemoveKey');
  };
  const requestSpeechKeyRemoval = () => {
    if (confirmSpeechKeyRemoval) {
      window.clearTimeout(speechKeyConfirmTimer.current);
      speechKeyConfirmTimer.current = null;
      setConfirmSpeechKeyRemoval(false);
      removeSpeechKey();
      return;
    }
    setConfirmSpeechKeyRemoval(true);
    setMessage(speechKeyConfirmMessage);
    speechKeyConfirmTimer.current = window.setTimeout(() => {
      speechKeyConfirmTimer.current = null;
      setConfirmSpeechKeyRemoval(false);
      setMessage(current => current === speechKeyConfirmMessage ? '' : current);
    }, 5000);
  };
  const endpointChanged = baseUrl.trim().replace(/\/+$/, '').toLowerCase() !== (speech.baseUrl || '').replace(/\/+$/, '').toLowerCase();
  const speechEndpointNeedsKey = !/^http:\/\/(localhost|127\.0\.0\.1|\[::1\])(?::|\/|$)/i.test(baseUrl.trim());
  const canSaveSpeech = !speechEndpointNeedsKey || !!key.trim() || (!!speech.hasKey && !endpointChanged);
  const speechReady = !speechEndpointNeedsKey || !!key.trim() || (!!speech.configured && !!speech.hasKey && !endpointChanged);
  const providerModels = provider === 'groq' ? ['whisper-large-v3-turbo', 'whisper-large-v3'] : provider === 'gemini' ? ['gemini-3.8-flash'] : ['gpt-4o-mini-transcribe', 'gpt-4o-transcribe'];
  const speechModelChoices = provider === 'custom' ? [] : (providerModels.includes(model) ? providerModels : [model, ...providerModels]);
  useLayoutEffect(() => {
    const content = settingsContentRef.current;
    if (!content) return;
    content.scrollTop = sectionScrollPositions.current.get(section) || 0;
  }, [section]);
  useEffect(() => { if (section !== 'agents') setAgentSettingsDetail(false); }, [section]);
  const shell = state.shellSettings || {};
  const sections = [
    ['general', 'Settings', 'General'],
    ['compact', 'Battery', 'Status'],
    ['priority', 'Controls', 'Priority'],
    ['behavior', 'Refresh', 'Motion'],
    ['tools', 'Home', 'Tools'],
    ['quick-actions', 'Command', 'Quick actions'],
    ['agents', 'Agents', 'Agents'],
    ['dictation', 'Mic', 'Dictation'],
    ['voice', 'VoiceWake', 'Voice'],
    ['ask', 'Question', 'Ask'],
    ['conversion', 'File', 'Conversion'],
  ];
  return <section className={'settings-page ' + (section === 'agents' && agentSettingsDetail ? 'is-agent-detail' : '')} aria-label="Island settings">
    <div className={'settings-workspace ' + (section === 'agents' && agentSettingsDetail ? 'is-agent-detail' : '')}>
      <nav className="settings-sidebar" aria-label="Settings sections">{sections.map(([id, icon, label]) => <button key={id} type="button" className={'settings-nav-item icon-only ' + (section === id ? 'selected' : '')} title={label} aria-label={label} aria-current={section === id ? 'page' : undefined} onClick={() => setSection(id)}><Icon name={icon} size={17} /></button>)}</nav>
      <div className={'settings-content ' + (section === 'agents' && agentSettingsDetail ? 'is-agent-detail' : '')} ref={settingsContentRef} data-section={section} onScroll={event => sectionScrollPositions.current.set(section, event.currentTarget.scrollTop)}>
        {section === 'general' && <>
          <div className="settings-grid">
            <SettingsCard icon="Power" title="Startup" detail="Open Island when you sign in."><SettingsToggle label="Start with Windows" value={!!state.startup?.enabled} disabled={!state.startup?.canChange || state.startup?.busy} onChange={() => send('startupSet', { enabled: !state.startup?.enabled })} /><div className="settings-save-row"><span>{state.startup?.description || 'Checking startup state…'}</span><button type="button" className="settings-inline-action" disabled={state.startup?.busy} onClick={() => send('startupSet')}>Check</button></div></SettingsCard>
            <SettingsCard icon="Moon" title="Appearance" detail="Theme and accent." className="settings-card-appearance"><SettingsSegments className="settings-appearance-segments" value={state.dark ? 'dark' : 'light'} options={ [['dark', 'Dark'], ['light', 'Light']] } onChange={value => { if (state.dark !== (value === 'dark')) send('themeToggle'); }} /><div className="settings-accent-choice"><span>Accent gradient</span><div role="group" aria-label="Accent gradient">{accentGradients.map(([id, label]) => <button type="button" key={id} className={'accent-swatch accent-' + id} aria-label={label} aria-pressed={accentGradient === id} title={label} onClick={() => setAccentGradient(id)}><i aria-hidden="true" />{accentGradient === id && <Icon name="Check" size={12} />}</button>)}</div></div></SettingsCard>
            <SettingsCard icon="Play" title="Media card" className="settings-card-media"><SettingsSegments value={mediaCardStyle} options={ [['standard', 'Standard'], ['backdrop', 'Artwork'], ['studio', 'Studio']] } onChange={setMediaCardStyle} /></SettingsCard>
            <SettingsCard icon="Settings" title="Island behavior" detail="Shape and privacy controls." className="settings-card-wide"><div className="settings-line"><span>Shape</span><SettingsSegments value={shell.shape || 'notch'} options={ [['notch', 'Notch'], ['capsule', 'Capsule']] } onChange={value => send('shellSetting', { key: 'shape', value })} /></div><SettingsToggle label="Privacy indicators" value={!!shell.privacy} onChange={() => send('shellSetting', { key: 'privacy', value: !shell.privacy })} /><SettingsToggle label="Hide during full screen" value={!!shell.hideFullscreen} onChange={() => send('shellSetting', { key: 'hideFullscreen', value: !shell.hideFullscreen })} /><div className="settings-capture-setting"><SettingsToggle label="Exclude Island from screen capture" value={!!shell.excludeFromScreenCapture} disabled={!shell.screenCaptureExclusionSupported} onChange={() => send('shellSetting', { key: 'excludeFromScreenCapture', value: !shell.excludeFromScreenCapture })} /><p className="settings-note">{shell.screenCaptureExclusionSupported ? 'Keeps Island visible on your display while omitting its window from supported capture APIs. Capture behavior varies by app.' : 'Requires Windows 10 version 2004 or later.'}</p></div></SettingsCard>
          </div>
        </>}
        {section === 'compact' && <>
          <div className="settings-grid">
            <SettingsCard icon="Battery" title="Battery" detail="Keep the level glanceable."><div className="settings-setting-row"><span>Display</span><SettingsSegments value={batteryView.mode} options={ [['both', 'Icon + %'], ['icon', 'Icon'], ['percent', 'Percent']] } onChange={value => setBatteryView(previous => ({ ...previous, mode: value }))} /></div><div className="settings-setting-row"><span>Position</span><SettingsSegments value={batteryView.side} options={ [['left', 'Left'], ['right', 'Right']] } onChange={value => setBatteryView(previous => ({ ...previous, side: value }))} /></div><SettingsToggle label="Show battery" value={!!state.batteryEnabled} onChange={() => send('indicatorToggle', { indicator: 'battery' })} /></SettingsCard>
            <SettingsCard icon="Clock" title="Clock" detail="Show local time while idle."><SettingsToggle label="Show clock" value={!!state.clockEnabled} onChange={() => send('indicatorToggle', { indicator: 'clock' })} /><p className="settings-note">Shown when no higher-priority status is active. Reorder status modules in Priority.</p></SettingsCard>
          </div>
        </>}
        {section === 'priority' && <PrioritySettings order={compactPriorityOrder} onChange={setCompactPriorityOrder} modules={compactStatusModules} lanes={compactStatusLanes} Icon={Icon} />}
        {section === 'behavior' && <>
          <div className="settings-grid">
            <SettingsCard icon="Loading" title="Motion" detail="Keep transitions quick and comfortable." className="settings-card-wide"><SettingsToggle label="Reduce motion" value={!!shell.reducedMotion} onChange={() => send('shellSetting', { key: 'reducedMotion', value: !shell.reducedMotion })} />
              <div className="settings-choice"><span>Open notch on</span><SettingsSegments value={shell.openOnClick ? 'click' : 'hover'} options={ [['hover', 'Hover'], ['click', 'Click']] } onChange={value => send('shellSetting', { key: 'openOnClick', value: value === 'click' })} /></div>
              {shell.openOnClick && <div className="settings-choice"><span>Close notch on</span><SettingsSegments value={shell.closeOnOutsideClick ? 'outside' : 'leave'} options={ [['leave', 'Auto close'], ['outside', 'Click outside']] } onChange={value => send('shellSetting', { key: 'closeOnOutsideClick', value: value === 'outside' })} /></div>}
              {!shell.openOnClick && <div className="settings-choice"><span>Open after hover</span><SettingsSegments value={shell.hoverOpen ?? 200} options={ [[0, 'Instant'], [100, '100 ms'], [200, '200 ms'], [350, '350 ms']] } onChange={value => send('shellSetting', { key: 'hoverOpen', value })} /></div>}
              {(!shell.openOnClick || !shell.closeOnOutsideClick) && <div className="settings-choice"><span>Close after leave</span><SettingsSegments value={shell.hoverClose ?? 350} options={ [[200, '200 ms'], [350, '350 ms'], [600, '600 ms'], [900, '900 ms']] } onChange={value => send('shellSetting', { key: 'hoverClose', value })} /></div>}
            </SettingsCard>
            <SettingsCard icon="Settings" title="Transparency" detail="Applied smoothly while the shell moves."><div className="settings-choice"><span>Folded</span><SettingsSegments value={shell.foldedTransparency ?? 0} options={ [[0, '0%'], [10, '10%'], [20, '20%'], [35, '35%']] } onChange={value => send('shellSetting', { key: 'foldedTransparency', value })} /></div><div className="settings-choice"><span>Expanded</span><SettingsSegments value={shell.openTransparency ?? 0} options={ [[0, '0%'], [10, '10%'], [20, '20%'], [35, '35%']] } onChange={value => send('shellSetting', { key: 'openTransparency', value })} /></div></SettingsCard>
          </div>
        </>}
        {section === 'tools' && <>
          <div className="settings-grid">
            <SettingsCard icon="Clipboard" title="Clipboard" detail="History stays on this PC until Island closes."><SettingsToggle label="Capture copied text" value={!!state.clipboardCaptureEnabled} onChange={() => send('clipboardSetting', { captureEnabled: !state.clipboardCaptureEnabled })} /><div className="settings-setting-row settings-history-row"><span>History limit</span><SettingsSegments className="settings-history-segments" value={state.clipboardHistoryLimit || 200} options={ [[50, '50'], [200, '200'], [500, '500'], [1000, '1000']] } onChange={value => send('clipboardSetting', { historyLimit: value })} /></div><button type="button" className="settings-inline-action" onClick={() => send('clipboardClear')}>Clear history</button></SettingsCard>
            <SettingsCard icon="Weather" title="Weather" detail="Forecast units for your saved place."><div className="settings-setting-row settings-weather-units-row"><span>Units</span><SettingsSegments className="settings-weather-units" value={state.weatherUnits || 'metric'} options={ [['metric', '°C · m/s'], ['imperial', '°F · mph']] } onChange={value => send('weatherUnitsSet', { units: value })} /></div><div className="settings-setting-row"><span>Place</span><strong className="settings-value">{state.weatherCity || 'Choose on Weather page'}</strong></div></SettingsCard>
            <SettingsCard icon="Home" title="Pages" detail="Choose which tools appear in navigation." className="settings-card-wide"><div className="settings-page-toggles">{[['clipboard', 'Clipboard'], ['shelf', 'Shelf'], ['convert', 'Convert'], ['text', 'Text'], ['agents', 'Agents'], ['note', 'Note'], ['weather', 'Weather'], ['controls', 'Controls']].map(([id, label]) => <SettingsToggle key={id} label={label} value={state.modules?.[id] !== false} onChange={() => send('moduleSetting', { module: id, enabled: state.modules?.[id] === false })} />)}</div></SettingsCard>
          </div>
        </>}
        {section === 'quick-actions' && <HomeActionSettings actions={homeActions} onChange={setHomeActions} catalog={homeActionCatalog} Icon={Icon} />}
        {section === 'agents' && <AgentSettings state={state} Icon={Icon} send={send} onDetailChange={detail => { setAgentSettingsDetail(detail); if (settingsContentRef.current) settingsContentRef.current.scrollTop = 0; }} />}
        {section === 'conversion' && <div className="settings-conversion-layout">
          <SettingsDisclosure id="engine" icon="Video" title="FFmpeg" summary={conversion.engineBusy ? conversion.enginePhase || 'Installing…' : (conversion.engineSource || 'missing') === 'missing' ? 'Not installed' : conversion.installedVersion || 'Ready'} active={conversionEditor} onOpen={setConversionEditor} Icon={Icon}>
            <div className="settings-engine-status"><span className={'settings-engine-dot is-' + (conversion.engineSource || 'missing')} /><div><strong>{conversion.engineBusy ? conversion.enginePhase || 'Connecting' : conversion.engineSource === 'managed' ? 'Ready to use' : conversion.engineSource === 'selected' ? 'Using selected copy' : conversion.engineSource === 'system' ? 'Found on this PC' : 'Not installed'}</strong><small title={conversion.enginePath || ''}>{conversion.enginePath || 'Install a private copy for this app.'}</small></div></div>
            {(conversion.engineSource || 'missing') !== 'missing' && <div className="settings-engine-version"><span>Installed <strong>{conversion.installedVersion || (conversion.checkingUpdate ? 'Checking…' : 'Unknown')}</strong></span><span>Latest <strong>{conversion.checkingUpdate ? 'Checking…' : conversion.latestVersion || 'Unavailable'}</strong></span>{conversion.updateAvailable && <b>Update ready</b>}</div>}
            <div className="settings-engine-actions">
              {((conversion.engineSource || 'missing') === 'missing' || conversion.updateAvailable) && <button type="button" className="settings-conversion-primary" disabled={conversion.engineBusy || conversion.busy} onClick={() => { setConfirmEngineRemoval(false); send('conversionEngineInstall'); }}>{conversion.updateAvailable ? 'Update FFmpeg' : 'Install FFmpeg'}</button>}
              {(conversion.engineSource || 'missing') !== 'missing' && <button type="button" disabled={conversion.engineBusy || conversion.checkingUpdate} onClick={() => send('conversionEngineCheckUpdate')}>{conversion.checkingUpdate ? 'Checking…' : 'Check for updates'}</button>}
            </div>
            <details className="settings-engine-maintenance"><summary>Manage installation</summary><div className="settings-engine-actions">
              <button type="button" disabled={conversion.engineBusy || conversion.busy} onClick={() => { setConfirmEngineRemoval(false); send('conversionEngine'); }}>Choose existing</button>
              {(conversion.engineSource || 'missing') !== 'missing' && <button type="button" disabled={conversion.engineBusy || conversion.busy} onClick={() => send('conversionEngineInstall')}>Reinstall</button>}
              {conversion.engineSource === 'managed' && <button type="button" className={confirmEngineRemoval ? 'is-armed' : ''} disabled={conversion.engineBusy || conversion.busy} onClick={() => { if (confirmEngineRemoval) { send('conversionEngineRemove'); setConfirmEngineRemoval(false); } else setConfirmEngineRemoval(true); }}>{confirmEngineRemoval ? 'Confirm remove' : 'Remove'}</button>}
              {conversion.engineSource === 'selected' && <button type="button" disabled={conversion.engineBusy || conversion.busy} onClick={() => send('conversionEngineForget')}>Forget path</button>}
            </div></details>
            {conversion.engineBusy && <div className="settings-engine-progress" role="status"><div><span>{conversion.enginePhase || 'Connecting'}</span><strong>{conversion.engineDownloadedBytes ? `${(conversion.engineDownloadedBytes / 1e6).toFixed(1)} MB${conversion.engineTotalBytes ? ` / ${(conversion.engineTotalBytes / 1e6).toFixed(1)} MB` : ''}` : 'Preparing download'}</strong></div>{conversion.engineTotalBytes > 0 && <div className="settings-engine-progress-track" role="progressbar" aria-label="FFmpeg download" aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.min(100, Math.round(100 * conversion.engineDownloadedBytes / conversion.engineTotalBytes))}><i style={{ transform: `scaleX(${Math.min(1, conversion.engineDownloadedBytes / conversion.engineTotalBytes)})` }} /></div>}</div>}
            <p className="settings-note" role="status">{conversion.engineNotice || 'Video and audio support. Images work without FFmpeg.'}</p>
          </SettingsDisclosure>
          <SettingsDisclosure id="output" icon="Folder" title="Output folder" summary={conversion.destination ? 'Custom folder' : 'Beside originals'} active={conversionEditor} onOpen={setConversionEditor} Icon={Icon}>
            <div className="settings-output-location" title={conversion.destination || 'Save beside each original file'}><Icon name="Folder" size={17} /><span>{conversion.destination || 'Beside each original'}</span></div>
            <div className={'settings-output-categories ' + (!conversion.destination ? 'is-preview' : '')} aria-label="Output subfolders"><span><Icon name="Image" size={13} />Images</span><span><Icon name="Video" size={13} />Videos</span><span><Icon name="Audio" size={13} />Audio</span></div>
            <div className="settings-engine-actions"><button type="button" className="settings-conversion-primary" disabled={conversion.busy} onClick={() => send('conversionDestination')}>{conversion.destination ? 'Change folder' : 'Choose folder'}</button>{conversion.destination && <button type="button" disabled={conversion.busy} onClick={() => send('conversionDestinationClear')}>Use original folders</button>}</div>
            <p className="settings-note" role="status">{conversion.destinationNotice || (conversion.destination ? 'Sorted into Images, Videos, and Audio.' : 'Original files are preserved.')}</p>
          </SettingsDisclosure>
          <SettingsDisclosure id="images" icon="Image" title="Image defaults" summary={`${conversionDefaults.format.toUpperCase()} · ${conversionDefaults.quality}%`} active={conversionEditor} onOpen={setConversionEditor} Icon={Icon}>
            <div className="settings-conversion-field"><span>Format</span><SettingsSegments value={conversionDefaults.format} options={ [['png', 'PNG'], ['jpg', 'JPEG'], ['webp', 'WebP'], ['bmp', 'BMP'], ['tiff', 'TIFF'], ['gif', 'GIF']] } onChange={format => updateConversionDefaults({ format })} /></div>
            <div className="settings-conversion-field"><label htmlFor="conversion-default-quality">JPEG quality</label><div className="settings-conversion-range"><input id="conversion-default-quality" type="range" min="35" max="100" value={conversionDefaults.quality} onChange={event => updateConversionDefaults({ quality: +event.target.value })} /><span>{conversionDefaults.quality}%</span></div></div>
          </SettingsDisclosure>
          <SettingsDisclosure id="limit" icon="File" title="Optional size limit" summary={`${conversionDefaults.limit} ${conversionDefaults.unit} · Fit size only`} active={conversionEditor} onOpen={setConversionEditor} Icon={Icon}><div className="settings-conversion-field"><label htmlFor="conversion-default-limit">Starting limit</label><div className="settings-conversion-input"><input id="conversion-default-limit" type="number" min={conversionDefaults.unit === 'MB' ? '.01' : '1'} max="100000" step={conversionDefaults.unit === 'MB' ? '.01' : '1'} value={conversionDefaults.limit} onChange={event => updateConversionDefaults({ limit: Math.max(conversionDefaults.unit === 'MB' ? .01 : 1, Math.min(100000, +event.target.value || 1)) })} /><SettingsSegments value={conversionDefaults.unit} options={ [['KB', 'KB'], ['MB', 'MB']] } onChange={unit => updateConversionDefaults({ unit, limit: unit === 'KB' ? Math.round(conversionDefaults.limit * 1000) : Math.round(conversionDefaults.limit / 10) / 100 })} /></div><small>Applies only when you choose Fit size for a batch.</small></div></SettingsDisclosure>
        </div>}
        {section === 'ask' && <SettingsCard icon="Ask" title="Ask answer style">
          <p className="settings-note">Guide the length, tone, and formatting of spoken answers. Conversation background stays in Ask → Context.</p>
          <label className="ask-context settings-ask-style">Answer instructions<textarea value={askInstructions} maxLength={6000} onChange={e => setAskInstructions(e.target.value)} onBlur={() => saveAgentPreferences({ ...readAgentPreferences(), askInstructions })} /></label>
          <button type="button" className="settings-inline-action" onClick={() => { setAskInstructions(defaultAskInstructions); saveAgentPreferences({ ...readAgentPreferences(), askInstructions: defaultAskInstructions }); }}>Restore default style</button>
        </SettingsCard>}
        {section === 'voice' && <SettingsCard icon="Mic" title="Voice wake">
          <SettingsToggle label="Listen for wake phrase" value={wakeOn} disabled={!wakeOn && !speech.configured} onChange={() => { const phrase = wakePhrase.trim() || 'Hey Island'; saveAgentPreferences({ ...readAgentPreferences(), wakePhrase: phrase }); send('wakeWordSet', { enabled: !wakeOn, phrase }); }}/>
          <div className="settings-choice"><span>Wake sensitivity</span><SettingsSegments value={voicePreferences.wakeSensitivity} options={ [['low', 'Low'], ['balanced', 'Balanced'], ['high', 'High']] } onChange={wakeSensitivity => changeVoicePreference({ wakeSensitivity })}/></div>
          <p className="settings-note">Lower sensitivity reduces false triggers. Higher sensitivity accepts less certain phrase matches.</p>
          <label className="settings-voice-phrase">Wake phrase<input value={wakePhrase} maxLength={64} disabled={wakeOn} onChange={e => setWakePhrase(e.target.value)} onBlur={() => saveAgentPreferences({ ...readAgentPreferences(), wakePhrase: wakePhrase.trim() || 'Hey Island' })}/></label>
          <p className="settings-note" role={wake.error ? 'alert' : 'status'}>{wake.error || (wake.status === 'paused' ? 'Paused while another app uses the microphone. Resumes automatically.' : wake.status === 'arming' ? 'Starting the local listener…' : wake.status === 'triggered' ? 'Handling your question. Listening resumes after the answer.' : wakeOn ? 'Listening locally. Say the phrase, wait for the tone, then ask your question.' : 'Say a distinctive phrase of 2–5 words. Answers use your selected Agents provider and session.')}</p>
          {!speech.configured && <button className="settings-inline-action" onClick={() => setSection('dictation')}>Set up Dictation</button>}
        </SettingsCard>}
        {section === 'dictation' && <div className="settings-speech-layout">

          <SettingsDisclosure id="provider" icon="Mic" title="Transcription" summary={provider === 'custom' ? 'Custom' : provider === 'groq' ? 'Groq' : provider === 'gemini' ? 'Gemini' : 'OpenAI'} active={speechEditor} onOpen={setSpeechEditor} Icon={Icon}>

            <div className="settings-provider-list" role="group" aria-label="Speech provider">
              {[
                ['groq', 'Groq', 'Whisper · fast'],
                ['gemini', 'Gemini', 'Audio understanding'],
                ['openai', 'OpenAI', 'Speech transcription'],
              ].map(([id, label, description]) => <button type="button" key={id} className={provider === id ? 'selected' : ''} aria-pressed={provider === id} onClick={() => chooseProvider(id)}><strong>{label}</strong><small>{description}</small></button>)}
            </div>
            <button type="button" className={'settings-custom-endpoint ' + (provider === 'custom' ? 'selected' : '')} aria-pressed={provider === 'custom'} onClick={() => { if (provider !== 'custom') chooseProvider('custom'); }}><Icon name="Controls" size={13} />Custom compatible endpoint</button>
            {provider === 'custom' && <label className="settings-field settings-endpoint-field">Server URL<input type="url" value={baseUrl} onChange={event => setBaseUrl(event.target.value)} placeholder="http://127.0.0.1:8000/v1" autoComplete="url" /></label>}
            <div className="settings-speech-config-row">
              <div className="settings-field settings-model-field"><span>Model</span>
                {provider === 'custom' ? <input value={model} onChange={event => setModel(event.target.value)} placeholder="whisper-1" maxLength={200} /> : <div className="settings-model-options" role="group" aria-label="Transcription model">{speechModelChoices.map(value => <button type="button" key={value} className={model === value ? 'selected' : ''} aria-pressed={model === value} onClick={() => setModel(value)}>{value}</button>)}</div>}
              </div>
              <label className="settings-field settings-language-field">Language <small>Optional</small><input value={language} onChange={event => setLanguage(event.target.value)} placeholder="Auto detect" maxLength={32} /></label>
            </div>
          </SettingsDisclosure>
          <SettingsDisclosure id="capture" icon="Controls" title="Microphone" summary="Sensitivity and pauses" active={speechEditor} onOpen={setSpeechEditor} Icon={Icon}>
            <div className="settings-choice"><span>Speech detection</span><SettingsSegments value={voicePreferences.transcriptionSensitivity} options={ [['low', 'Low'], ['balanced', 'Balanced'], ['high', 'High']] } onChange={transcriptionSensitivity => changeVoicePreference({ transcriptionSensitivity })}/></div>
            <p className="settings-note">High detects quieter speech. This controls automatic stopping after a wake phrase; holding Space always records until release.</p>
            <div className="settings-choice"><span>Pause before sending</span><SettingsSegments value={voicePreferences.speechPauseMs} options={ [[900, 'Short'], [1500, 'Natural'], [2200, 'Relaxed']] } onChange={speechPauseMs => changeVoicePreference({ speechPauseMs })}/></div>
            <SettingsToggle label="Microphone noise reduction" value={voicePreferences.speechNoiseSuppression} onChange={() => changeVoicePreference({ speechNoiseSuppression: !voicePreferences.speechNoiseSuppression })}/>
            <p className="settings-note">If words sound clipped on a headset, try turning noise reduction off. Set a language above when automatic language detection gets words wrong.</p>
          </SettingsDisclosure>
          <SettingsDisclosure id="credentials" icon="Lock" title="API key" summary={speech.hasKey && !endpointChanged ? 'Saved on this device' : speechEndpointNeedsKey ? 'Add key' : 'Optional'} active={speechEditor} onOpen={setSpeechEditor} Icon={Icon}>

            <label className="settings-field settings-api-key-field"><span>{speech.hasKey && !endpointChanged ? 'Replace saved key' : 'Provider key'}</span><input type="password" value={key} onChange={event => setKey(event.target.value)} placeholder={speech.hasKey && !endpointChanged ? 'Paste a new key to replace it' : provider === 'custom' && !speechEndpointNeedsKey ? 'Not required for localhost' : `Paste ${provider === 'groq' ? 'Groq' : provider === 'gemini' ? 'Google AI Studio' : provider === 'openai' ? 'OpenAI' : 'provider'} API key`} autoComplete="new-password" /></label>
            {speech.hasKey && <button type="button" className={'settings-remove-key ' + (confirmSpeechKeyRemoval ? 'is-armed' : '')} onClick={requestSpeechKeyRemoval} disabled={message === 'Removing key…'}>{confirmSpeechKeyRemoval ? 'Click again to remove' : 'Remove saved key'}</button>}
          </SettingsDisclosure>
          <div className="settings-speech-footer"><span role="status">{message || (speechReady ? 'Ready to dictate' : 'Add a provider key to enable dictation.')}</span><button type="button" className="settings-save" disabled={!canSaveSpeech || message === 'Saving…' || message === 'Removing key…'} onClick={saveSpeech}>Save changes</button></div>
        </div>}
      </div>
    </div>
  </section>;
}
function ScreenSelectionSetup({ mode, onBack, onSelect, recordQuality = 'high', onRecordQualityChange }) {
  const details = {
    capture: { title: 'Capture', detail: 'Choose what to save.', icon: 'Capture', area: 'Select area', full: 'All displays' },
    ocr: { title: 'Read text', detail: 'Choose where to read.', icon: 'OCR', area: 'Select area', full: 'All displays' },
    qr: { title: 'Scan QR', detail: 'Choose where to scan.', icon: 'Qr', area: 'Select area', full: 'All displays' },
    record: { title: 'Record', detail: 'Choose what to record.', icon: 'Record', area: 'Select area', full: 'All displays' },
  }[mode];
  useEffect(() => {
    const onKey = event => { if (event.key === 'Escape') onBack(); };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onBack]);
  if (!details) return null;
  return <>
    <section className="home-primary screen-selection-context" aria-labelledby="screen-selection-title">
      <NestedPageHeader title={details.title} icon={details.icon} onBack={onBack} detail={details.detail} titleId="screen-selection-title" />
      {mode === 'record' ? <div className="record-quality-picker">
        <span className="record-quality-label">Recording quality</span>
        <div className="record-quality-options" role="group" aria-label="Recording quality">
          {[{ value: 'standard', label: 'Standard', detail: '720p · 24 fps' }, { value: 'high', label: 'High', detail: '1080p · 30 fps' }, { value: 'maximum', label: 'Maximum', detail: 'Up to 4K · 30 fps' }].map(option => <button type="button" key={option.value} className={recordQuality === option.value ? 'is-selected' : ''} aria-pressed={recordQuality === option.value} onClick={() => onRecordQualityChange?.(option.value)}><strong>{option.label}</strong><small>{option.detail}</small></button>)}
        </div>
      </div> : <span className="tool-state-footnote">Choose a screen</span>}
    </section>
    <section className="actions tool-scope-actions" aria-label={`Choose ${mode} area`}>
      <button type="button" onClick={() => onSelect('area')}>
        <Icon name={details.icon} size={18} />
        <span className="tool-scope-copy"><strong>{details.area}</strong><small>Drag to choose</small></span>
        <Icon name="NextMonth" size={15} />
      </button>
      <button type="button" onClick={() => onSelect('full')}>
        <Icon name="Computer" size={18} />
        <span className="tool-scope-copy"><strong>{details.full}</strong><small>Entire desktop</small></span>
        <Icon name="NextMonth" size={15} />
      </button>
    </section>
  </>;
}

function DictateVisualizer({ inputLevels, dictating, transcriptionPending, averageLevel, compact = false }) {
  return <div className={'dictate-visualizer ' + (compact ? 'agent-voice-visualizer ' : '') + (dictating ? 'is-live ' : '') + (transcriptionPending ? 'is-processing' : '')} role="meter" aria-label="Microphone input level" aria-valuemin={0} aria-valuemax={100} aria-valuenow={dictating ? Math.round(averageLevel * 100) : 0}>
    <span className="dictate-wave" aria-hidden="true">{inputLevels.map((value, index) => {
      const level = dictating ? value : 0;
      return <i key={index} style={{ '--voice-scale': .08 + level * .92 }} />;
    })}</span>
  </div>;
}

function DictatePanel({ captureSensitivity = 'balanced', pauseMs = 1500, noiseSuppression = true, speechSettings, onClose, onInsert, compact = false, minimal = false, autoSubmit = false, holdCapture = false, transcriptExpanded = false, onTranscriptExpandedChange }) {
  const captureGeneration = useRef(0);
  const holdReleased = useRef(false);
  const startingRef = useRef(false);
  const [starting, setStarting] = useState(false);
  const insertRef = useRef(onInsert);
  insertRef.current = onInsert;
  const audioRecorderRef = useRef(null);
  const audioStreamRef = useRef(null);
  const audioChunksRef = useRef([]);
  const audioChunkBytesRef = useRef(0);
  const discardDictationRef = useRef(false);
  const speechRequestIdRef = useRef(0);
  const recordingStartedRef = useRef(0);
  const [error, setError] = useState('');
  const [dictating, setDictating] = useState(false);
  const [transcriptionPending, setTranscriptionPending] = useState(false);
  const [transcript, setTranscript] = useState('');
  const [recordingSeconds, setRecordingSeconds] = useState(0);
  const [inputLevels, setInputLevels] = useState(Array(25).fill(0));
  const meterAnalyserRef = useRef(null);
  const meterTimerRef = useRef(null);
  const audioContextRef = useRef(null);
  const wakeVadRef = useRef({ noise: 0.004, aboveSince: 0, lastVoiceAt: 0, speechSeen: false, stopped: false });

  const stopInputMeter = () => {
    clearInterval(meterTimerRef.current);
    meterTimerRef.current = null;
    meterAnalyserRef.current = null;
    setInputLevels(Array(25).fill(0));
    const context = audioContextRef.current;
    audioContextRef.current = null;
    if (context && context.state !== 'closed') context.close().catch(() => {});
  };
  const startInputMeter = (stream, autoFinish = false) => {
    const AudioContextType = window.AudioContext || window.webkitAudioContext;
    if (!AudioContextType) return;
    try {
      const context = new AudioContextType();
      const analyser = context.createAnalyser();
      analyser.fftSize = 512;
      analyser.smoothingTimeConstant = 0.68;
      context.createMediaStreamSource(stream).connect(analyser);
      context.resume().catch(() => {});
      audioContextRef.current = context;
      meterAnalyserRef.current = analyser;
      const frequencies = new Uint8Array(analyser.frequencyBinCount);
      const waveform = new Float32Array(analyser.fftSize);
      const bands = Array.from({ length: 25 }, (_, index) => {
        const start = Math.floor(Math.pow(index / 25, 1.7) * frequencies.length);
        return [start, Math.max(start + 1, Math.floor(Math.pow((index + 1) / 25, 1.7) * frequencies.length))];
      });
      meterTimerRef.current = setInterval(() => {
        const meter = meterAnalyserRef.current;
        if (!meter) return;
        meter.getByteFrequencyData(frequencies);
        const nextLevels = bands.map(([start, end]) => {
          let energy = 0;
          for (let bin = start; bin < end; bin += 1) energy += frequencies[bin] / 255;
          return Math.max(0, Math.min(1, (energy / (end - start)) * 3.2));
        });
        if (audioStreamRef.current && document.visibilityState !== "hidden") setInputLevels(nextLevels);
        if (autoFinish && !wakeVadRef.current.stopped) {
          const now = Date.now();
          meter.getFloatTimeDomainData(waveform);
          let energy = 0;
          for (let index = 0; index < waveform.length; index += 1) energy += waveform[index] * waveform[index];
          const level = Math.sqrt(energy / waveform.length);
          const vad = wakeVadRef.current;
          const elapsed = now - recordingStartedRef.current;
          if (!vad.speechSeen && elapsed > 12000) { vad.stopped = true; discardDictationRef.current = true; stopDictation(); onClose(); return; }
          if (elapsed < 160) {
            if (level < .03) vad.noise = vad.noise * .8 + level * .2;
            return;
          }
          const detection = captureSensitivity === 'high' ? { floor: .005, multiplier: 1.6, onset: 120 } : captureSensitivity === 'low' ? { floor: .016, multiplier: 3, onset: 240 } : { floor: .008, multiplier: 2.1, onset: 160 };
          const threshold = Math.max(detection.floor, Math.min(.045, vad.noise * detection.multiplier));
          if (level >= threshold * (vad.speechSeen ? .7 : 1)) {
            vad.aboveSince ||= now;
            if (vad.speechSeen || now - vad.aboveSince >= detection.onset) {
              vad.speechSeen = true;
              vad.lastVoiceAt = now;
            }
          } else {
            vad.aboveSince = 0;
            if (!vad.speechSeen) vad.noise = vad.noise * .94 + level * .06;
            if (vad.speechSeen && now - vad.lastVoiceAt >= pauseMs) {
              vad.stopped = true;
              stopDictation();
            }
          }
        }
      }, 80);
    } catch { stopInputMeter(); }
  };

  useEffect(() => {
    const onKey = event => { if (event.key === 'Escape') onClose(); };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onClose]);
  useEffect(() => {
    const onSpeechResult = event => {
      const result = event.detail;
      if (result?.requestId !== speechRequestIdRef.current) return;
      speechRequestIdRef.current = 0;
      setTranscriptionPending(false);
      if (result.error) setError(result.error);
      else if (!result.text?.trim()) setError('No speech was recognized. Hold to record and try again.');
      else {
        setError('');
        setTranscript(result.text || '');
        if (insertRef.current && result.text?.trim()) insertRef.current(result.text);
      }
    };
    const speechBridge = payload => window.dispatchEvent(new CustomEvent('island:speech-result', { detail: payload }));
    window.addEventListener('island:speech-result', onSpeechResult);
    window.islandSpeechResult = speechBridge;
    return () => {
      window.removeEventListener('island:speech-result', onSpeechResult);
      if (window.islandSpeechResult === speechBridge) delete window.islandSpeechResult;
    };
  }, []);
  useEffect(() => () => {
    captureGeneration.current += 1;
    speechRequestIdRef.current = 0;
    discardDictationRef.current = true;
    const recorder = audioRecorderRef.current;
    if (recorder && recorder.state !== 'inactive') {
      recorder.ondataavailable = null;
      recorder.onstop = null;
      try { recorder.stop(); } catch { /* The device may already be stopped. */ }
    }
    audioStreamRef.current?.getTracks().forEach(track => track.stop());
    clearInterval(meterTimerRef.current);
    audioContextRef.current?.close().catch(() => {});
  }, []);

  const stopDictation = () => {
    const recorder = audioRecorderRef.current;
    if (recorder && recorder.state !== 'inactive') {
      setDictating(false);
      stopInputMeter();
      recorder.stop();
    }
  };
  const startDictation = async () => {
    if (startingRef.current || audioRecorderRef.current?.state === "recording" || transcriptionPending) return;
    onTranscriptExpandedChange?.(false);
    setError(''); setTranscript('');
    if (!speechSettings?.configured) { setError('Set up speech in Island Settings to begin.'); return; }
    if (!navigator.mediaDevices?.getUserMedia || !window.MediaRecorder) {
      setError('Microphone recording is unavailable in this Island WebView. Update WebView2 and try again.');
      return;
    }
    const generation = ++captureGeneration.current;
    startingRef.current = true;
    setStarting(true);
    try {
      const stream = await navigator.mediaDevices.getUserMedia({ audio: { channelCount: 1, echoCancellation: true, noiseSuppression, autoGainControl: true }, video: false });
      if (generation !== captureGeneration.current) { stream.getTracks().forEach(track => track.stop()); return; }
      const mimeType = ['audio/webm;codecs=opus', 'audio/webm', 'audio/mp4'].find(type => MediaRecorder.isTypeSupported?.(type)) || '';
      if (!mimeType) { stream.getTracks().forEach(track => track.stop()); throw new Error('Island could not find a supported microphone audio format.'); }
      audioStreamRef.current = stream;
      audioChunksRef.current = [];
      audioChunkBytesRef.current = 0;
      discardDictationRef.current = false;
      const recorder = new MediaRecorder(stream, { mimeType, audioBitsPerSecond: 96_000 });
      audioRecorderRef.current = recorder;
      recorder.ondataavailable = event => {
        if (generation !== captureGeneration.current || !event.data?.size) return;
        audioChunkBytesRef.current += event.data.size;
        if (audioChunkBytesRef.current > 5 * 1024 * 1024) {
          discardDictationRef.current = true;
          setError('Recording reached the five megabyte limit. Try a shorter dictation.');
          if (recorder.state !== 'inactive') recorder.stop();
          return;
        }
        audioChunksRef.current.push(event.data);
      };
      recorder.onstop = () => {
        stream.getTracks().forEach(track => track.stop());
        if (generation !== captureGeneration.current) return;
        audioStreamRef.current = null;
        stopInputMeter();
        setDictating(false);
        if (discardDictationRef.current) { audioChunksRef.current = []; return; }
        const blob = new Blob(audioChunksRef.current, { type: mimeType.split(';')[0] });
        audioChunksRef.current = [];
        if (!blob.size) { setError('No microphone audio was recorded.'); return; }
        if (blob.size > 5 * 1024 * 1024) { setError('Recording is too large. Try a shorter dictation.'); return; }
        setTranscriptionPending(true);
        const reader = new FileReader();
        reader.onerror = () => { if (generation === captureGeneration.current) { setTranscriptionPending(false); setError('Island could not prepare the recording.'); } };
        reader.onload = () => {
          if (generation !== captureGeneration.current || discardDictationRef.current) return;
          const data = String(reader.result || '');
          const encoded = data.slice(data.indexOf(',') + 1);
          if (!encoded || encoded.length > 7 * 1024 * 1024) { setTranscriptionPending(false); setError('Recording is too large to transcribe.'); return; }
          speechRequestIdRef.current = Date.now();
          setTranscriptionPending(true);
          send('speechTranscribe', { requestId: speechRequestIdRef.current, audio: `data:${blob.type};base64,${encoded}` });
        };
        reader.readAsDataURL(blob);
      };
      stream.getAudioTracks()[0]?.addEventListener('ended', () => { if (recorder.state !== 'inactive') recorder.stop(); }, { once: true });
      wakeVadRef.current = { noise: .004, aboveSince: 0, lastVoiceAt: 0, speechSeen: false, stopped: false };
      recorder.onstart = () => {
        if (generation !== captureGeneration.current || recorder.state !== 'recording') return;
        startingRef.current = false;
        setStarting(false);
        recordingStartedRef.current = Date.now();
        setRecordingSeconds(0);
        setDictating(true);
        startInputMeter(stream, autoSubmit);
        if (autoSubmit) send('wakeCaptureReady');
      };
      recorder.start(1000);
      if (holdCapture && (holdReleased.current || window.islandHoldReleased)) recorder.stop();
    } catch (reason) {
      if (generation !== captureGeneration.current) return;
      audioStreamRef.current?.getTracks().forEach(track => track.stop());
      audioStreamRef.current = null;
      stopInputMeter();
      setError(reason?.name === 'NotAllowedError' ? 'Allow microphone access in the Island WebView to use Dictate.' : reason?.message || 'Island could not start microphone recording.');
    } finally {
      if (generation === captureGeneration.current && audioRecorderRef.current?.state !== 'recording') { startingRef.current = false; setStarting(false); }
    }
  };
  useEffect(() => {
    if (compact && speechSettings?.configured) startDictation();
  }, []);
  useEffect(() => {
    if (!holdCapture) return;
    const release = () => { holdReleased.current = true; stopDictation(); };
    const key = e => { if (e.code === 'Space' || e.code === 'Enter' || e.key === ' ' || e.key === 'Enter') release(); };
    window.addEventListener('island:hold-release', release);
    window.addEventListener('keyup', key);
    window.addEventListener('blur', release);
    window.addEventListener('pointerup', release);
    window.addEventListener('pointercancel', release);
    return () => { window.removeEventListener('island:hold-release', release); window.removeEventListener('keyup', key); window.removeEventListener('blur', release); window.removeEventListener('pointerup', release); window.removeEventListener('pointercancel', release); };
  }, [holdCapture]);
  const cancelDictation = () => {
    captureGeneration.current += 1;
    speechRequestIdRef.current = 0;
    startingRef.current = false;
    setStarting(false);
    setTranscriptionPending(false);
    setDictating(false);
    stopInputMeter();
    discardDictationRef.current = true;
    audioChunksRef.current = [];
    setError('');
    const recorder = audioRecorderRef.current;
    if (recorder && recorder.state !== 'inactive') recorder.stop();
    else { audioStreamRef.current?.getTracks().forEach(track => track.stop()); audioStreamRef.current = null; setDictating(false); stopInputMeter(); }
    if (autoSubmit) onClose();
  };
  useEffect(() => {
    if (!dictating) return undefined;
    const timer = setInterval(() => {
      const seconds = Math.floor((Date.now() - recordingStartedRef.current) / 1000);
      setRecordingSeconds(seconds);
      if (seconds >= 180) { setError('Dictation reached the three minute limit and is being transcribed.'); stopDictation(); }
    }, 1000);
    return () => clearInterval(timer);
  }, [dictating]);

  const averageLevel = inputLevels.reduce((total, level) => total + level, 0) / Math.max(inputLevels.length, 1);
  const dictationStatus = starting ? 'Starting microphone' : dictating ? 'Listening' : transcriptionPending ? 'Transcribing' : transcript ? 'Ready to paste' : speechSettings?.configured ? 'Ready' : 'Needs setup';
  const timerLabel = String(Math.floor(recordingSeconds / 60)).padStart(2, '0') + ':' + String(recordingSeconds % 60).padStart(2, '0');
  const sentenceCount = transcript.match(/[.!?](?=\s|$)/g)?.length ?? 0;
  const canExpandTranscript = transcript.trim().length > 100 || sentenceCount > 1;
  if (minimal) return <div className="wake-listening-surface" role="status" aria-label={dictationStatus}>
    <div className={'wake-audio-eyes ' + (transcriptionPending ? 'is-processing' : '')} style={{ '--eye-level': averageLevel }} aria-hidden="true"><i/><i/></div>
    {error && <p role="alert">{error}</p>}
  </div>;
  if (compact && holdCapture) return <div className="ask-capture-surface" data-capture-state={dictating ? 'recording' : transcriptionPending ? 'processing' : starting ? 'starting' : 'idle'}>
    <span role="status">{error || (dictating ? 'Listening · release to ask' : transcriptionPending ? 'Transcribing…' : starting ? 'Starting microphone…' : 'Hold Space to try again')}</span>
    <DictateVisualizer inputLevels={inputLevels} dictating={dictating} transcriptionPending={transcriptionPending} averageLevel={averageLevel} compact />
  </div>;
  if (compact) return <div className="agent-voice-surface" aria-label="Dictating agent task">
    <div className="agent-voice-top"><span className="agent-voice-state"><Icon name="Mic" size={15} /><span key={dictationStatus} className="agent-voice-stage" aria-live="polite">{dictationStatus}</span></span><button type="button" onClick={onClose} aria-label="Close dictation"><Icon name="Cancel" size={14} /></button></div>
    <DictateVisualizer inputLevels={inputLevels} dictating={dictating} transcriptionPending={transcriptionPending} averageLevel={averageLevel} compact />
    <div className="agent-voice-bottom"><span>{error || (dictating ? autoSubmit ? `Pause briefly to send · ${timerLabel}` : timerLabel : transcriptionPending ? 'Transcribing…' : speechSettings?.configured ? 'Ready' : 'Set up speech in Settings')}</span>{(starting || dictating || transcriptionPending) && <button type="button" onClick={cancelDictation}>Cancel</button>}<button type="button" className="agent-voice-record" disabled={starting || transcriptionPending || !speechSettings?.configured} onClick={dictating ? stopDictation : startDictation}>{starting ? 'Starting…' : dictating ? 'Done' : 'Record'}</button></div>
  </div>;
  return <section className={'dictate-page ' + (transcriptExpanded && transcript ? 'is-transcript-expanded' : '')} aria-labelledby="dictate-title">
    <div className="dictate-page-head"><NestedPageHeader title="Dictate" icon="Mic" onBack={onClose} detail={dictationStatus} titleId="dictate-title" className="dictate-heading" />{dictating && <span className="dictate-timer">{timerLabel}</span>}</div>
    {!transcriptExpanded && <DictateVisualizer inputLevels={inputLevels} dictating={dictating} transcriptionPending={transcriptionPending} averageLevel={averageLevel} />}
    {transcriptExpanded && transcript ? <div className="dictate-transcript-full">
      <div className="dictate-transcript-full-head"><span>Transcript</span><button type="button" onClick={() => onTranscriptExpandedChange?.(false)}>Back to recording</button></div>
      <div className="dictate-transcript-full-body" aria-live="polite" aria-atomic="true"><p>{transcript}</p></div>
    </div> : <div className="dictate-transcript" aria-live="polite" aria-atomic="true">
      <p>{transcript || (dictating ? 'Listening…' : transcriptionPending ? 'Preparing your transcript…' : 'Your words will appear here')}</p>
      {transcript && canExpandTranscript && <button type="button" className="dictate-expand-transcript" onClick={() => onTranscriptExpandedChange?.(true)} aria-expanded="false">Read full transcript <Icon name="ChevronRight" size={13} /></button>}
    </div>}
    {error && <p className="dictate-error" role="status">{error}</p>}
    <div className="dictate-footer">
      {transcript && <div className="dictate-result-actions"><button type="button" onClick={() => send('clipboardWriteText', { value: transcript })}><Icon name="Copy" size={15} />Copy</button><button type="button" onClick={() => send('dictationPaste', { text: transcript })}><Icon name="Paste" size={15} />Paste</button></div>}
      {dictating ? <div className="dictate-record-split" role="group" aria-label="Dictation controls">
        <button type="button" className="dictate-record is-recording" disabled={transcriptionPending} aria-label="Stop dictation and transcribe" onClick={stopDictation}><span className="dictate-record-icon"><i /></span>Finish</button>
        <span className="dictate-record-divider" aria-hidden="true" />
        <button type="button" className="dictate-cancel" aria-label="Cancel dictation" title="Cancel dictation" onClick={cancelDictation}><Icon name="Cancel" size={15} /></button>
      </div> : <button type="button" className="dictate-record" disabled={starting || transcriptionPending || !speechSettings?.configured} title={speechSettings?.configured ? undefined : 'Configure speech in Island Settings'} onClick={startDictation}><span className="dictate-record-icon">{transcriptionPending ? <Icon name="Loading" size={16} /> : <Icon name="Mic" size={17} />}</span>{starting ? 'Starting…' : transcriptionPending ? 'Transcribing…' : 'Start dictation'}</button>}
    </div>
  </section>;
}

function ScreenResultCard({ result, colors, onDismiss }) {
  if (!result) return null;
  const copy = value => send('clipboardWriteText', { value });
  const openText = value => { send('textSet', { value }); send('page', { page: 'Text' }); };
  const copyColor = value => copy(value);
  const title = result.error ? 'Could not finish' : ({ capture: 'Screenshot saved', ocr: 'Screen text', qr: 'QR code', color: 'Color picked', record: 'Recording saved', dictate: 'Dictated text' }[result.mode] || 'Island result');
  const glyph = ({ capture: 'Capture', ocr: 'OCR', qr: 'Qr', color: 'ColorPicker', record: 'Record', dictate: 'Mic' }[result.mode] || 'Check');
  const shortName = result.path?.split(/[\\/]/).at(-1);
  const openFile = action => send('screenToolFile', { action, path: result.path });
  return <section className={'screen-result-card ' + (result.error ? 'is-error' : '')} aria-live="polite" aria-label={title}>
    {result.preview && <img className="screen-result-preview" src={result.preview} alt={`Preview of ${result.width} by ${result.height} pixel capture`} />}
    <div className="screen-result-icon"><Icon name={glyph} size={17} /></div>
    <div className="screen-result-body">
      <div className="screen-result-heading"><strong>{result.phase === 'working' ? 'Reading selected text…' : title}</strong><button type="button" className="screen-result-dismiss" onClick={onDismiss} aria-label="Dismiss result">×</button></div>
      {result.error ? <p>{result.error}</p> : result.phase === 'working' ? <p>Local text recognition is in progress.</p> : result.mode === 'capture' ? <>
        <p>{result.width} × {result.height} px{result.sizeBytes ? ` · ${(result.sizeBytes / 1048576).toFixed(1)} MB` : ''}<span> · </span>{shortName || 'Saved to Downloads'}</p>
        <div className="screen-result-actions"><button type="button" onClick={() => openFile('open')}>Open</button><button type="button" onClick={() => openFile('reveal')}>Show in folder</button></div>
      </> : result.mode === 'record' ? <>
        <p>{shortName || 'MP4'}{result.durationMs ? ` · ${Math.floor(result.durationMs / 1000)} sec` : ''}{result.sizeBytes ? ` · ${(result.sizeBytes / 1048576).toFixed(1)} MB` : ''}</p>
        <div className="screen-result-actions"><button type="button" onClick={() => openFile('open')}>Play</button><button type="button" onClick={() => openFile('reveal')}>Show in folder</button></div>
      </> : result.mode === 'color' ? <>
        <div className="screen-color-value"><i style={{ backgroundColor: result.hex }} /><code>{result.hex}</code><span>{result.rgb}</span></div>
        <div className="screen-result-actions"><button type="button" onClick={() => copyColor(result.hex)}><Icon name="Copy" size={13} /> HEX</button><button type="button" onClick={() => copyColor(result.rgb)}><Icon name="Copy" size={13} /> RGB</button>{colors.filter(color => color.hex !== result.hex).slice(0, 8).map(color => <button type="button" key={color.hex} className="screen-color-history" title={`Copy ${color.hex}`} aria-label={`Copy recent color ${color.hex}`} style={{ '--sample': color.hex }} onClick={() => copyColor(color.hex)} />)}</div>
      </> : result.mode === 'qr' ? <>
        <p className="screen-result-text">{result.text}</p>
        <div className="screen-result-actions"><button type="button" onClick={() => { copy(result.text); }}>Copy</button><button type="button" onClick={() => openText(result.text)}>Open in Text</button>{result.url && <button type="button" onClick={() => send('openExternalUrl', { url: result.text })}>Open link</button>}</div>
      </> : result.mode === 'ocr' || result.mode === 'dictate' ? <>
        <p className="screen-result-text">{result.text}</p>
        <div className="screen-result-actions"><button type="button" onClick={() => copy(result.text)}>Copy</button><button type="button" onClick={() => openText(result.text)}>Open in Text</button></div>
      </> : null}
    </div>
  </section>;
}

const commandActions = [
  ['capture', 'Capture area', 'Capture'], ['ocr', 'Screen text', 'OCR'], ['qr', 'Scan QR code', 'Qr'],
  ['color', 'Pick color', 'ColorPicker'], ['record', 'Record screen', 'Record'], ['dictate', 'Voice to text', 'Mic'],
  ['Clipboard', 'Clipboard', 'Clipboard'], ['Shelf', 'Shelf', 'Shelf'], ['Note', 'Notes', 'Note'],
  ['Text', 'Text tools', 'Text'], ['Weather', 'Weather', 'Weather'], ['Controls', 'Controls', 'Controls'], ['Agents', 'Agents', 'Agents'],
];

function CommandPage({ query, setQuery, state, onClose, onRoute }) {
  const inputRef = useRef(null);
  const [selected, setSelected] = useState(0);
  const needle = query.trim().toLocaleLowerCase();
  const visibleActions = commandActions.filter(([, label]) => !needle || label.toLocaleLowerCase().includes(needle)).slice(0, needle ? 5 : 6);
  const results = [
    ...(needle ? (state.commandApps || []).map(app => typeof app === 'string' ? { name: app } : app).filter(app => app.name.toLocaleLowerCase().includes(needle)) : []).map(app => ({ type: 'app', name: app.name, detail: 'Application', icon: 'Computer', iconData: app.icon })),
    ...(needle ? (state.commandFiles || []).filter(file => file.name.toLocaleLowerCase().includes(needle)).map(file => {
      const fallbackIcon = ({ folder: 'Folder', image: 'Image', video: 'Video', audio: 'Audio', archive: 'Archive', code: 'FileCode', text: 'FileText', pdf: 'Pdf', document: 'FileText', app: 'Computer' })[file.kind] || 'File';
      return { type: 'file', name: file.name, detail: file.path, icon: fallbackIcon, iconData: file.icon, path: file.path };
    }) : []),
    ...visibleActions.map(([action, name, icon]) => ({ type: 'action', action, name, detail: 'Island action', icon })),
  ].slice(0, 11);
  useEffect(() => { inputRef.current?.focus(); send('commandOpen'); send('commandAppSearch', { query: '' }); }, []);
  useEffect(() => {
    const timer = setTimeout(() => send('commandFileSearch', { query: query.trim() }), 180);
    return () => clearTimeout(timer);
  }, [query]);
  useEffect(() => {
    const timer = setTimeout(() => send('commandAppSearch', { query: query.trim() }), 100);
    return () => clearTimeout(timer);
  }, [query]);
  useEffect(() => {
    const onKey = event => { if (event.key === 'Escape') onClose(); };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onClose]);
  const submit = event => {
    event.preventDefault();
    const web = query.match(/^search\s+(.+)$/i);
    const ask = query.match(/^ask(?:\s+|:\s*)(.+)$/i);
    if (web) onRoute('web', web[1].trim());
    else if (ask) onRoute('ask', ask[1].trim());
    else if (results[selected] && (!needle || results[selected].name.toLocaleLowerCase().includes(needle))) runResult(results[selected]);
    else send('commandEvaluate', { query });
  };
  const runResult = result => {
    if (result.type === 'action') onRoute('action', result.action);
    else if (result.type === 'app') send('commandAppLaunch', { name: result.name });
    else if (result.type === 'file') send('commandFileOpen', { path: result.path });
  };
  const handleKeyDown = event => {
    if (event.key === 'ArrowDown' && results.length) { event.preventDefault(); setSelected(index => (index + 1) % results.length); }
    else if (event.key === 'ArrowUp' && results.length) { event.preventDefault(); setSelected(index => (index - 1 + results.length) % results.length); }
  };
  return <section className="command-page" aria-label="Command search">
    <NestedPageHeader title="Command" icon="Command" onBack={onClose} className="command-page-heading" />
    <form className="command-form" onSubmit={submit}><Icon name="Search" size={19} /><input ref={inputRef} value={query} onKeyDown={handleKeyDown} onChange={event => { setQuery(event.target.value); setSelected(0); send('commandClear'); }} placeholder="Search files, apps, actions, or calculate" aria-label="Search files, apps, and actions" maxLength={128} /><button type="submit" disabled={!query.trim()} aria-label="Run search" title="Run search"><Icon name="Enter" size={16} /></button></form>
    {(state.commandResult || state.commandError) && <div className={'command-answer ' + (state.commandError ? 'is-error' : '')} role="status">{state.commandResult || state.commandError}</div>}
    <div className="command-results" role="listbox" aria-label="Search results">{results.map((result, index) => <button type="button" role="option" aria-selected={selected === index} className={selected === index ? 'is-selected' : ''} key={`${result.type}:${result.path || result.name}`} onMouseEnter={() => setSelected(index)} onClick={() => runResult(result)}><span className="command-result-icon">{result.iconData ? <img src={result.iconData} alt="" /> : <Icon name={result.icon || 'File'} size={17} />}</span><span className="command-result-copy"><strong>{result.name}</strong><small>{result.detail}</small></span><span className="command-result-type">{result.type === 'app' ? 'App' : result.type === 'file' ? 'File' : 'Action'}</span></button>)}</div>
    {(state.commandFilesLoading || state.commandAppsLoading) && <div className="command-searching"><Icon name="Loading" size={13} /> Searching…</div>}
    {!results.length && needle && !state.commandFilesLoading && <div className="command-empty">No matching items. Try a file name or a calculation.</div>}
    <div className="command-page-footer"><span>↑↓ Navigate · Enter Open · Esc Home</span><span>Files & apps</span></div>
  </section>;
}

function App() {
  const [state, setState] = useState(initial);
  const appearanceMigrationDone = useRef(false);
  useLayoutEffect(() => {
    const generation = Number(state.compactRenderGeneration);
    if (state.expanded || !Number.isSafeInteger(generation) || generation < 1) return undefined;
    // Let WebView2 paint the committed compact tree once before native code
    // shrinks the child HWND. The second frame keeps the handoff out of the
    // same compositor turn that applies React's state update.
    let firstFrame = 0;
    let secondFrame = 0;
    firstFrame = requestAnimationFrame(() => {
      secondFrame = requestAnimationFrame(() => send('compactRenderReady', { generation }));
    });
    return () => {
      cancelAnimationFrame(firstFrame);
      cancelAnimationFrame(secondFrame);
    };
  }, [state.expanded, state.compactRenderGeneration]);
  useEffect(() => {
    if (state.expanded || !state.compactFaded) return;
    // Bounded recovery for a throttled WebView handoff. Rust still decides
    // when geometry is safe; React never reveals content on its own timer.
    const timer = setTimeout(() => send('compactResync', { generation: state.compactRenderGeneration }), 750);
    return () => clearTimeout(timer);
  }, [state.expanded, state.compactFaded, state.compactRenderGeneration]);
  const [selectedOpenCodeSession, setSelectedOpenCodeSession] = useState('');
  const [openCodeFolder, setOpenCodeFolder] = useState('');
  const [openCodePrompt, setOpenCodePrompt] = useState('');
  const [openCodeFiles, setOpenCodeFiles] = useState([]);
  const [openCodeMode, setOpenCodeMode] = useState('new');
  const [openCodeSessionsOpen, setOpenCodeSessionsOpen] = useState(false);
  const [openCodeProfilesOpen, setOpenCodeProfilesOpen] = useState(false);
  const [openCodeAgentPickerOpen, setOpenCodeAgentPickerOpen] = useState(false);
  const [openCodeSelectedAgent, setOpenCodeSelectedAgent] = useState('');
  const [agentMention, setAgentMention] = useState(null);
  const [agentMentionIndex, setAgentMentionIndex] = useState(0);
  const [openCodeSessionQuery, setOpenCodeSessionQuery] = useState('');
  const [openCodeSending, setOpenCodeSending] = useState(false);
  const [openCodeStopping, setOpenCodeStopping] = useState(false);
  const [openCodeModel, setOpenCodeModel] = useState('');
  const [openCodeModelsOpen, setOpenCodeModelsOpen] = useState(false);
  const [openCodeResponseOpen, setOpenCodeResponseOpen] = useState(false);
  const [openCodeModelQuery, setOpenCodeModelQuery] = useState('');
  const [replyingPermissionId, setReplyingPermissionId] = useState('');
  const openCodePromptRef = useRef(null);
  const pendingOpenCodeSend = useRef(false);
  const [shellPhase, setShellPhase] = useState('idle');
  const hasExpandedOnce = useRef(false);

  const displayPage = state.page;
  const accentGradient = accentGradients.some(([id]) => id === state.accentGradient) ? state.accentGradient : 'lime';
  const setAccentGradient = value => {
    if (!accentGradients.some(([id]) => id === value)) return;
    setState(previous => ({ ...previous, accentGradient: value }));
    try { localStorage.setItem('island.accent-gradient', value); } catch {}
    send('appearanceSet', { accentGradient: value });
  };
  const mediaCardStyle = ['backdrop', 'studio'].includes(state.mediaCardStyle) ? state.mediaCardStyle : 'standard';
  const setMediaCardStyle = value => {
    if (!['standard', 'backdrop', 'studio'].includes(value)) return;
    setState(previous => ({ ...previous, mediaCardStyle: value }));
    send('appearanceSet', { mediaCardStyle: value });
  };
  const [text, setText] = useState('');
  const [note, setNote] = useState('');
  const [city, setCity] = useState('');
  const [noteClearArmed, setNoteClearArmed] = useState(false);
  const [clipboardQuery, setClipboardQuery] = useState('');
  const [folderName, setFolderName] = useState('');
  const [showFolderInput, setShowFolderInput] = useState(false);
  const [folderEditing, setFolderEditing] = useState(false);
  const [folderDeleteArmed, setFolderDeleteArmed] = useState(false);
  const [showReminderCalendar, setShowReminderCalendar] = useState(false);
  const [noteView, setNoteView] = useState('notes');
  const [signOpen, setSignOpen] = useState(false);
  const signatureStrokesRef = useRef([]);
  const [commandOpen, setCommandOpen] = useState(false);
  const [commandQuery, setCommandQuery] = useState('');
  const [dictateOpen, setDictateOpen] = useState(false);
  const [holdCapture, setHoldCapture] = useState(false);

  const [dictateMode, setDictateMode] = useState('home');
  const [agentDictateAutoSubmit, setAgentDictateAutoSubmit] = useState(false);
  useEffect(() => { const dismiss = () => { setDictateOpen(false); setAgentDictateAutoSubmit(false); send('wakeWordResume'); }; window.addEventListener('island:voice-dismiss', dismiss); return () => window.removeEventListener('island:voice-dismiss', dismiss); }, []);
  const [dictateTranscriptExpanded, setDictateTranscriptExpanded] = useState(false);
  const [agentDictatedText, setAgentDictatedText] = useState('');
  const [dictatePortalTarget, setDictatePortalTarget] = useState(null);
  const homeDictateSlotRef = useRef(null);
  const agentDictateSlotRef = useRef(null);
  const [screenSelection, setScreenSelection] = useState(null);
  const [recordQuality, setRecordQuality] = useState(() => {
    try {
      const saved = window.localStorage.getItem('island.screen-record-quality.v1');
      return ['standard', 'high', 'maximum'].includes(saved) ? saved : 'high';
    } catch { return 'high'; }
  });
  const [settingsOpen, setSettingsOpen] = useState(false);
  useEffect(() => {
    if (state.wakeWord?.status === 'triggered') {
      setSettingsOpen(false);
      setSignOpen(false);
      setCommandOpen(false);
    }
  }, [state.wakeWord?.revision, state.wakeWord?.status]);
  const [settingsClosing, setSettingsClosing] = useState(false);
  const [settingsReturning, setSettingsReturning] = useState(false);
  const settingsCloseTimer = useRef(null);
  const settingsReturnTimer = useRef(null);
  // SettingsPanel leaves the tree in compact mode; keep its reading position here.
  const [settingsSection, setSettingsSection] = useState('general');
  const settingsScrollPositions = useRef(new Map());
  const closeSettingsWithMotion = () => {
    if (!settingsOpen || settingsClosing) return;
    setSettingsClosing(true);
    window.clearTimeout(settingsCloseTimer.current);
    settingsCloseTimer.current = window.setTimeout(() => {
      setSettingsOpen(false); setSettingsClosing(false); setSettingsReturning(true);
      window.clearTimeout(settingsReturnTimer.current);
      settingsReturnTimer.current = window.setTimeout(() => setSettingsReturning(false), 260);
    }, 170);
  };
  useEffect(() => () => { window.clearTimeout(settingsCloseTimer.current); window.clearTimeout(settingsReturnTimer.current); }, []);
  useEffect(() => {
    const target = document.createElement('div');
    target.className = 'dictate-portal';
    setDictatePortalTarget(target);
    return () => target.remove();
  }, []);
  useLayoutEffect(() => {
    if (!dictatePortalTarget) return;
    const slot = !state.expanded || settingsOpen || !dictateOpen ? null
      : dictateMode === 'agent' && ['Agents', 'Ask'].includes(displayPage) ? agentDictateSlotRef.current
        : dictateMode === 'home' && displayPage === 'Home' ? homeDictateSlotRef.current : null;
    if (slot) slot.appendChild(dictatePortalTarget);
    else dictatePortalTarget.remove();
  }, [dictatePortalTarget, state.expanded, settingsOpen, dictateOpen, dictateMode, displayPage]);
  useEffect(() => {
    if (!dictateOpen || dictateMode !== 'home') setDictateTranscriptExpanded(false);
  }, [dictateOpen, dictateMode]);
  useEffect(() => {
    if (displayPage !== 'Ask' || settingsOpen) return;
    const ownsSpace = e => e.code === 'Space' && !isEditor(e.target) && !document.querySelector('.ask-context-page');
    const down = e => {
      if (!ownsSpace(e) || e.altKey || e.ctrlKey || e.metaKey) return;
      e.preventDefault(); e.stopImmediatePropagation();
      if (document.activeElement?.matches('button')) document.activeElement.blur();
      const agent = state.agents?.[readAgentPreferences().defaultProvider];
      if (e.repeat || dictateOpen || !state.speechSettings?.configured || !agent?.connected || agent.sessions?.some(session => session.id === agent.selectedId && session.active)) return;
      if (['arming', 'listening'].includes(state.wakeWord?.status)) send('wakeWordSet', { enabled: false });
      window.islandHoldReleased = false; send('holdSpaceBegin'); setHoldCapture(true); setAgentDictateAutoSubmit(true); setDictateMode('agent'); setDictateOpen(true);
    };
    const release = e => {
      if (!ownsSpace(e)) return;
      e.preventDefault(); e.stopImmediatePropagation();
      window.islandHoldReleased = true; send('holdSpaceEnd'); window.dispatchEvent(new Event('island:hold-release'));
    };
    window.addEventListener('keydown', down, true); window.addEventListener('keyup', release, true);
    return () => { window.removeEventListener('keydown', down, true); window.removeEventListener('keyup', release, true); };
  }, [displayPage, settingsOpen, dictateOpen, state.speechSettings?.configured, state.agents]);
  const [homeActions, setHomeActionsState] = useState(loadHomeActions);
  const setHomeActions = value => {
    const next = normalizeHomeActions(typeof value === 'function' ? value(homeActions) : value);
    setHomeActionsState(next);
    try { localStorage.setItem('island.home-actions', JSON.stringify(next)); } catch {}
    send('homeActionsSet', { actions: next });
  };
  const [compactPriorityOrder, setCompactPriorityOrderState] = useState(loadCompactStatusOrder);
  const setCompactPriorityOrder = value => {
    const next = normalizeCompactStatusOrder(typeof value === 'function' ? value(compactPriorityOrder) : value);
    setCompactPriorityOrderState(next);
    try { localStorage.setItem('island.compact-status-order', JSON.stringify(next)); } catch {}
  };
  const [shelfDrag, setShelfDrag] = useState(null);
  const shelfPointerRef = useRef(null);
  const suppressShelfClickRef = useRef(false);
  const [batteryView, setBatteryView] = useState(() => {
    try { const value = JSON.parse(localStorage.getItem('island.compact.battery') || '{}'); return { mode: ['both', 'icon', 'percent'].includes(value.mode) ? value.mode : 'both', side: value.side === 'left' ? 'left' : 'right' }; }
    catch { return { mode: 'both', side: 'right' }; }
  });
  const [screenResult, setScreenResult] = useState(null);
  const [screenColors, setScreenColors] = useState(() => { try { return JSON.parse(localStorage.getItem('island.screen.colors') || '[]').slice(0, 8); } catch { return []; } });
  const [screenRecordingAt, setScreenRecordingAt] = useState(0);
  const [screenRecordingStopping, setScreenRecordingStopping] = useState(false);
  const [screenRecordingElapsed, setScreenRecordingElapsed] = useState(0);
  const [weatherEditing, setWeatherEditing] = useState(false);
  const [weatherDataTransition, setWeatherDataTransition] = useState(false);
  const [weatherClockNow, setWeatherClockNow] = useState(() => Date.now());
  const [rainPulseOn, setRainPulseOn] = useState(true);
  const [updatePulseOn, setUpdatePulseOn] = useState(true);
  const [updateRotation, setUpdateRotation] = useState(0);
  const weatherLoadingRef = useRef(!!state.weatherLoading);
  const weatherUpdatedAtRef = useRef(state.weatherUpdatedAt ?? null);
  const weatherTransitionTimerRef = useRef(null);
  const rainForecastRefreshForRef = useRef('');
  const [powerArmed, setPowerArmed] = useState(null);
  useEffect(() => {
    if (state.weatherLoading) {
      weatherLoadingRef.current = true;
      clearTimeout(weatherTransitionTimerRef.current);
      weatherTransitionTimerRef.current = null;
      setWeatherDataTransition(false);
      return;
    }
    if (!weatherLoadingRef.current) {
      weatherUpdatedAtRef.current = state.weatherUpdatedAt ?? null;
      return;
    }

    weatherLoadingRef.current = false;
    const updatedAt = state.weatherUpdatedAt ?? null;
    const receivedFreshData = updatedAt != null && updatedAt !== weatherUpdatedAtRef.current && !!state.weatherData;
    weatherUpdatedAtRef.current = updatedAt;
    if (!receivedFreshData || state.shellSettings?.reducedMotion) return;

    setWeatherDataTransition(true);
    clearTimeout(weatherTransitionTimerRef.current);
    weatherTransitionTimerRef.current = setTimeout(() => {
      weatherTransitionTimerRef.current = null;
      setWeatherDataTransition(false);
    }, 420);
  }, [state.weatherLoading, state.weatherUpdatedAt, state.weatherData, state.shellSettings?.reducedMotion]);
  useEffect(() => () => clearTimeout(weatherTransitionTimerRef.current), []);
  useEffect(() => {
    if (!state.weatherData?.hourly?.time?.length) return undefined;
    const timer = setInterval(() => setWeatherClockNow(Date.now()), 60_000);
    return () => clearInterval(timer);
  }, [!!state.weatherData?.hourly?.time?.length, state.weatherData?.timezone]);
  useEffect(() => {
    const cityKey = state.weatherCity?.trim().toLocaleLowerCase() ?? '';
    if (state.weatherData?.hourly?.time?.length) {
      rainForecastRefreshForRef.current = cityKey;
      return;
    }
    const compactVisible = !state.expanded;
    const notesVisible = displayPage === 'Note' && noteView === 'notes';
    if ((!compactVisible && !notesVisible) || !cityKey || state.weatherLoading
      || rainForecastRefreshForRef.current === cityKey) return;
    rainForecastRefreshForRef.current = cityKey;
    send('weatherRefresh');
  }, [displayPage, noteView, state.expanded, state.weatherCity, state.weatherData, state.weatherLoading]);
  useEffect(() => {
    if (!powerArmed) return undefined;
    const timer = setTimeout(() => setPowerArmed(null), 2400);
    return () => clearTimeout(timer);
  }, [powerArmed]);
  const [mediaClock, setMediaClock] = useState(0);
  const [mediaReceivedAt, setMediaReceivedAt] = useState(0);
  const [fileDragOver, setFileDragOver] = useState(false);
  const [seekDraftMs, setSeekDraftMs] = useState(null);
  const seekPendingRef = useRef(null);
  const seekTimerRef = useRef(null);
  const [volumeDraft, setVolumeDraft] = useState(null);
  const [outputPickerOpen, setOutputPickerOpen] = useState(false);
  const volumeDraftRef = useRef(null);
  const volumeDraggingRef = useRef(false);
  const volumeTimerRef = useRef(null);
  const volumeLastSentRef = useRef({ value: null, at: 0 });
  const navRef = useRef(null);
  const bodyRef = useRef(null);
  const layoutWideRef = useRef(null);
  const weatherCityRef = useRef('');
  const tabSpring = useRef({ x: 0, width: 0, vx: 0, vw: 0, frame: 0, last: 0, placed: false });
  useEffect(() => {
    try { localStorage.setItem('island.screen.colors', JSON.stringify(screenColors.slice(0, 8))); } catch {}
  }, [screenColors]);
  useEffect(() => { try { localStorage.setItem('island.compact.battery', JSON.stringify(batteryView)); } catch {} }, [batteryView]);
  useEffect(() => {
    if (!screenRecordingAt) { setScreenRecordingElapsed(0); return undefined; }
    const update = () => setScreenRecordingElapsed(Math.max(0, Math.floor((Date.now() - screenRecordingAt) / 1000)));
    update();
    const timer = setInterval(update, 500);
    return () => clearInterval(timer);
  }, [screenRecordingAt]);
  useEffect(() => {
    let phaseTimer;
    window.islandReceive = next => {
      if (next.page === 'Convert') setSettingsOpen(false);
      let update = next;
      if (!appearanceMigrationDone.current && typeof next.accentGradient === 'string') {
        appearanceMigrationDone.current = true;
        try {
          const legacy = localStorage.getItem('island.accent-gradient');
          if (next.accentGradient === 'lime' && accentGradients.some(([id]) => id === legacy)) {
            update = { ...next, accentGradient: legacy };
            send('appearanceSet', { accentGradient: legacy });
          } else {
            localStorage.setItem('island.accent-gradient', next.accentGradient);
          }
        } catch {}
      }
      setState(previous => {
        const shelfIcons = Object.prototype.hasOwnProperty.call(update, 'shelfIcons')
          ? update.shelfIcons
          : update.shelfRevision === previous.shelfRevision
            ? previous.shelfIcons ?? []
            : [];
        return { ...previous, ...update, media: mergeMedia(previous.media, update.media), shelfIcons };
      });
      if (Array.isArray(update.homeActions)) {
        const savedActions = normalizeHomeActions(update.homeActions);
        setHomeActionsState(savedActions);
        try { localStorage.setItem('island.home-actions', JSON.stringify(savedActions)); } catch {}
      }
      setText(next.text ?? '');
      setNote(next.note ?? '');
      if (next.weatherCity !== weatherCityRef.current) {
        weatherCityRef.current = next.weatherCity ?? '';
        setCity(next.weatherCity ?? '');
      }
      setMediaReceivedAt(performance.now());
    };
    window.islandReceiveMedia = incoming => {
      setState(previous => ({ ...previous, media: mergeMedia(previous.media, incoming) }));
      setMediaReceivedAt(performance.now());
    };
    window.islandShellPhase = phase => {
      clearTimeout(phaseTimer);
      // The first opening should reveal already-mounted content directly. A
      // fade from an empty frame reads as a loading flash on a cold launch.
      if (phase === 'enter' && !hasExpandedOnce.current) {
        hasExpandedOnce.current = true;
        setShellPhase('open');
        return;
      }
      if (phase === 'enter') hasExpandedOnce.current = true;
      setShellPhase(phase);
      if (phase === 'enter') phaseTimer = setTimeout(() => setShellPhase('open'), 140);
    };

    window.islandFolderPicked = folder => { setOpenCodeFolder(folder); setOpenCodeMode('new'); setOpenCodeSessionsOpen(false); };
    window.islandOpenSettings = () => { setSignOpen(false); setDictateOpen(false); setScreenSelection(null); setCommandOpen(false); setSettingsOpen(true); };
    window.islandOpenCommand = () => { setSignOpen(false); setSettingsOpen(false); setDictateOpen(false); setScreenSelection(null); setCommandQuery(''); setCommandOpen(true); send('commandClear'); };
    window.islandFilesPicked = paths => {
      setOpenCodeFiles(previous => [...new Set([...previous, ...paths])].slice(0, 8));
      setOpenCodeMode(mode => mode === 'profiles' ? 'new' : mode);
    };
    window.islandScreenToolResult = async result => {
      if (!result) return;
      if (result.mode === 'record' && result.recording === true) {
        setScreenRecordingAt(Date.now());
        setScreenRecordingStopping(false);
        setScreenResult(null);
        return;
      }
      if (result.mode === 'record' && result.recording === 'stopping') { setScreenRecordingStopping(true); return; }
      if (result.mode === 'record' && (result.path || result.error)) {
        setScreenRecordingAt(0);
        setScreenRecordingStopping(false);
      }
      if (result.working) { setScreenResult(null); return; }
      if (result.error) {
        if (['ocr', 'qr', 'color', 'capture', 'record'].includes(result.mode)) { setScreenResult(null); return; }
        setScreenResult({ mode: result.mode || 'capture', error: result.error });
      }
      else if (result.mode === 'ocr' && result.text) {
        setScreenResult(null);
      } else if (result.mode === 'capture') {
        setScreenResult(null);
      } else if (result.mode === 'color') {
        setScreenResult(null);
        setScreenColors(previous => [{ hex: result.hex, rgb: result.rgb }, ...previous.filter(color => color.hex !== result.hex)].slice(0, 8));
      } else if (result.mode === 'record') {
        setScreenResult(null);
      }
    };
    send('ready');
    return () => { clearTimeout(phaseTimer); delete window.islandReceive; delete window.islandReceiveMedia; delete window.islandShellPhase; delete window.islandFolderPicked; delete window.islandOpenSettings; delete window.islandOpenCommand; delete window.islandFilesPicked; delete window.islandScreenToolResult; };
  }, []);
  useEffect(() => {
    let fileDragDepth = 0;
    const hasFiles = event => Array.from(event.dataTransfer?.types ?? []).includes('Files');
    const onEnter = event => {
      if (!hasFiles(event)) return;
      fileDragDepth += 1;
      setFileDragOver(true);
    };
    const onLeave = event => {
      if (!hasFiles(event)) return;
      fileDragDepth = Math.max(0, fileDragDepth - 1);
      if (fileDragDepth === 0) setFileDragOver(false);
    };
    const onDrop = event => {
      if (!hasFiles(event)) return;
      fileDragDepth = 0;
      setFileDragOver(false);
    };
    window.addEventListener('dragenter', onEnter);
    window.addEventListener('dragleave', onLeave);
    window.addEventListener('drop', onDrop);
    return () => {
      window.removeEventListener('dragenter', onEnter);
      window.removeEventListener('dragleave', onLeave);
      window.removeEventListener('drop', onDrop);
    };
  }, []);
  useEffect(() => {
    if (state.outputVolume == null || volumeDraggingRef.current) return;
    const last = volumeLastSentRef.current;
    // Ignore a queued pre-drag snapshot until the newest slider write lands.
    if (last.value != null && performance.now() - last.at < 700 && last.value !== state.outputVolume) return;
    volumeDraftRef.current = state.outputVolume;
    setVolumeDraft(state.outputVolume);
  }, [state.outputVolume]);
  useEffect(() => {
    if (!outputPickerOpen) return undefined;
    const onPointerDown = event => {
      if (!event.target.closest?.('.device-picker')) setOutputPickerOpen(false);
    };
    const onKeyDown = event => { if (event.key === 'Escape') setOutputPickerOpen(false); };
    window.addEventListener('pointerdown', onPointerDown);
    window.addEventListener('keydown', onKeyDown);
    return () => {
      window.removeEventListener('pointerdown', onPointerDown);
      window.removeEventListener('keydown', onKeyDown);
    };
  }, [outputPickerOpen]);
  useEffect(() => () => { clearTimeout(volumeTimerRef.current); clearTimeout(seekTimerRef.current); }, []);
  useEffect(() => {
    if (!state.media?.playing || (state.expanded && displayPage !== 'Home')) return undefined;
    setMediaClock(performance.now());
    const timer = setInterval(() => setMediaClock(performance.now()), state.expanded ? 250 : 1000);
    return () => clearInterval(timer);
  }, [state.media?.playing, state.media?.title, state.media?.position_ms, state.expanded, displayPage]);
  const page = state.page;
  const nav = name => { if (name !== 'Note') setShowReminderCalendar(false); setSignOpen(false); setCommandOpen(false); setDictateOpen(false); setScreenSelection(null); setSettingsOpen(false); send('page', { page: name }); };
  const openScreenSelection = mode => { setSignOpen(false); setScreenResult(null); setCommandOpen(false); setSettingsOpen(false); setDictateOpen(false); setScreenSelection(mode); };
  const openDictation = () => { if (['arming', 'listening'].includes(state.wakeWord?.status)) send('wakeWordSet', { enabled: false, phrase: state.wakeWord?.phrase }); setAgentDictateAutoSubmit(false); setSignOpen(false); setScreenResult(null); setCommandOpen(false); setSettingsOpen(false); setScreenSelection(null); setDictateMode('home'); setDictateTranscriptExpanded(false); setDictateOpen(true); };
  const openSignature = () => { setScreenResult(null); setCommandOpen(false); setSettingsOpen(false); setDictateOpen(false); setScreenSelection(null); setSignOpen(true); };
  const beginScreenTool = scope => {
    if (!screenSelection) return;
    send('screenToolStart', { mode: screenSelection, scope, quality: screenSelection === 'record' ? recordQuality : undefined });
    setScreenSelection(null);
  };
  const openNote = view => {
    setNoteView(view);
    setShowReminderCalendar(view === 'reminders' && !state.noteReminderAt);
    nav('Note');
  };
  const closeReminderCalendar = () => {
    setShowReminderCalendar(false);
    const selector = noteView === 'notes' ? '.reminder-pill' : state.noteReminderAt ? '.reminder-card > div > button' : '.reminder-empty .utility-primary';
    requestAnimationFrame(() => bodyRef.current?.querySelector(selector)?.focus());
  };
  const startOpenCodeTask = () => {
    setSelectedOpenCodeSession('');
    setOpenCodeMode('new');
    setOpenCodeSessionsOpen(false);
    setOpenCodeProfilesOpen(false);
    setOpenCodeResponseOpen(false);
    send('opencodeSelect', { sessionId: '' });
    nav('Agents');
  };
  const runHomeAction = id => {
    switch (id) {
      case 'capture': case 'record': case 'ocr': case 'qr': openScreenSelection(id); break;
      case 'color': setScreenResult(null); send('screenToolStart', { mode: 'color' }); break;
      case 'command': setCommandQuery(''); send('commandClear'); setCommandOpen(true); break;
      case 'dictate': openDictation(); break;
      case 'sign': openSignature(); break;
      case 'remind': openNote('reminders'); break;
      case 'task': startOpenCodeTask(); break;
      case 'clipboard-page': nav('Clipboard'); break;
      case 'shelf-page': nav('Shelf'); break;
      case 'text-page': nav('Text'); break;
      case 'agents-page': nav('Agents'); break;
      case 'notes-page': openNote('notes'); break;
      case 'weather-page': nav('Weather'); break;
      case 'controls-page': nav('Controls'); break;
      case 'convert-page': if (state.modules?.convert !== false) nav('Convert'); break;
      default: break;
    }
  };
  const runContextAction = id => {
    send('compactOpen', { page: 'Home' });
    runHomeAction(id);
  };
  const routeCommand = (kind, value) => {
    setCommandOpen(false);
    if (kind === 'action') {
      if (['capture', 'ocr', 'qr', 'color', 'record'].includes(value)) {
        if (value === 'color') { setScreenResult(null); send('screenToolStart', { mode: value }); }
        else openScreenSelection(value);
      } else if (value === 'dictate') {
        openDictation();
      } else if (pages.includes(value)) send('page', { page: value });
      return;
    }
    if (kind === 'web') {
      if (value) send('openExternalUrl', { url: `https://www.google.com/search?q=${encodeURIComponent(value)}` });
      return;
    }
    if (kind === 'ask') {
      setOpenCodePrompt(value);
      startOpenCodeTask();
    }
  };
  const modules = state.modules ?? {};
  const contextTools = useMemo(() => {
    const kind = state.foregroundApp?.kind;
    if (!kind || state.foregroundApp?.executable?.toLowerCase() === 'explorer.exe') return [];
    const pageModule = { 'clipboard-page': 'clipboard', 'shelf-page': 'shelf', 'text-page': 'text', 'agents-page': 'agents', 'notes-page': 'note', 'weather-page': 'weather', 'controls-page': 'controls', 'convert-page': 'convert' };
    const ranks = {
      browser: ['capture', 'ocr', 'qr', 'color', 'dictate', 'command', 'notes-page', 'remind'],
      coding: ['task', 'agents-page', 'command', 'clipboard-page', 'capture', 'dictate', 'notes-page'],
      spreadsheet: ['convert-page', 'ocr', 'capture', 'clipboard-page', 'dictate', 'notes-page'],
      office: ['sign', 'convert-page', 'capture', 'ocr', 'remind', 'notes-page'],
      design: ['convert-page', 'capture', 'color', 'ocr', 'shelf-page'],
      media: ['capture', 'record', 'dictate', 'controls-page', 'notes-page'],
      other: ['command', 'dictate', 'remind', 'notes-page', 'weather-page'],
    }[kind] || [];
    const preferredOrder = new Map(ranks.map((id, index) => [id, index]));
    return homeActionCatalog
      .filter(action => action.contexts?.includes(kind))
      .filter(action => !pageModule[action.id] || modules[pageModule[action.id]] !== false)
      .sort((left, right) => (preferredOrder.get(left.id) ?? Number.MAX_SAFE_INTEGER) - (preferredOrder.get(right.id) ?? Number.MAX_SAFE_INTEGER))
      .slice(0, 5);
  }, [state.foregroundApp?.kind, state.foregroundApp?.executable, state.modules]);
  const visible = pages.filter(name => name === 'Home' || modules[name === 'Ask' ? 'agents' : name.toLowerCase()] !== false);
  useLayoutEffect(() => {
    // The profile editor now fits the standard notch width; don't widen the
    // native window when switching to it.
    const wide = false;
    if (!state.expanded) {
      if (layoutWideRef.current !== wide) { layoutWideRef.current = wide; send('layout', { wide }); }
      return;
    }
    // Settings uses a fixed native viewport. Do not run the intrinsic-height
    // measurement below: it temporarily changes the live page from flex to
    // auto-sized block layout, which can expand its nested scroller, clamp its
    // scrollTop to zero, and then shrink it again. The saved scroll position
    // cannot recover from that browser-side clamp.
    if (settingsOpen) {
      if (layoutWideRef.current !== wide) layoutWideRef.current = wide;
      send('layout', {
        wide,
        allowLarge: true,
        settingsOpen: true,
        page: displayPage,
        generation: state.compactRenderGeneration ?? 0,
      });
      return;
    }
    const content = bodyRef.current?.firstElementChild;
    if (!content) return;
    let last = 0;
    let lastWide = null;
    let lastAllowLarge = null;
    let lastSettingsOpen = null;
    const report = (force = false) => {
      const bar = document.querySelector('.bar');
      const body = bodyRef.current;
      const styles = getComputedStyle(body);
      // Measure intrinsic page content instead of the current WebView viewport.
      // Otherwise height:100% feeds the previous shell height back into native
      // layout and short pages cannot shrink after a tab or shape change.
      const pageRoot = content.firstElementChild;
      const preferredHeight = Number(pageRoot?.dataset.layoutHeight);
      let naturalHeight;
      if (Number.isFinite(preferredHeight) && preferredHeight > 0) {
        naturalHeight = Math.min(preferredHeight, 480);
      } else {
      const previousDisplay = content.style.display;
      const previousHeight = content.style.height;
      const previousMinHeight = content.style.minHeight;
      const previousFlex = pageRoot?.style.flex ?? '';
      const previousRootHeight = pageRoot?.style.height ?? '';
      content.style.display = 'block';
      content.style.height = 'auto';
      content.style.minHeight = '0';
      if (pageRoot) {
        pageRoot.style.flex = 'none';
        pageRoot.style.height = 'auto';
      }
      naturalHeight = content.scrollHeight;
      content.style.display = previousDisplay;
      content.style.height = previousHeight;
      content.style.minHeight = previousMinHeight;
      if (pageRoot) {
        pageRoot.style.flex = previousFlex;
        pageRoot.style.height = previousRootHeight;
      }
      }
      const height = Math.ceil((bar?.offsetHeight ?? 43) + naturalHeight
        + parseFloat(styles.paddingTop) + parseFloat(styles.paddingBottom));
      const allowLarge = settingsOpen || (displayPage === 'Home' && (!!screenResult || commandOpen || signOpen || (dictateOpen && dictateMode === 'home' && dictateTranscriptExpanded))) || displayPage === 'Agents' || displayPage === 'Ask' || displayPage === 'Convert';
      if (force || Math.abs(height - last) >= 4 || lastWide !== wide || lastAllowLarge !== allowLarge || lastSettingsOpen !== settingsOpen) { last = height; lastWide = wide; lastAllowLarge = allowLarge; lastSettingsOpen = settingsOpen; layoutWideRef.current = wide; send('layout', { height, wide, allowLarge, settingsOpen, page: displayPage, generation: state.compactRenderGeneration ?? 0 }); }
    };
    let frame = 0;
    let forceNext = false;
    const schedule = (force = false) => {
      forceNext ||= force;
      if (frame) return;
      frame = requestAnimationFrame(() => { frame = 0; const forced = forceNext; forceNext = false; report(forced); });
    };
    const observer = new ResizeObserver(() => schedule());
    observer.observe(content);
    if (content.firstElementChild) observer.observe(content.firstElementChild);
    const requestLayout = () => schedule(true);
    window.islandRequestLayout = requestLayout;
    schedule(true);
    return () => { observer.disconnect(); cancelAnimationFrame(frame); if (window.islandRequestLayout === requestLayout) delete window.islandRequestLayout; };
  // Notes and Reminders can have different intrinsic heights while the
  // flex-sized workspace keeps the same observed outer size. Re-run the
  // existing measurement when its internal tab changes instead of trusting
  // ResizeObserver to notice a change hidden by the fixed viewport.
  }, [displayPage, noteView, showReminderCalendar, signOpen, state.expanded, state.compactRenderGeneration, state.agents, state.weatherData, state.noteFolders?.length, showFolderInput, folderEditing, dictateOpen, dictateMode, dictateTranscriptExpanded, screenSelection, screenResult, commandOpen, settingsOpen]);
  const clips = state.clipboard ?? [];
  const homeClips = (state.homeClipboard ?? clips).map(item => ({ item, index: clips.indexOf(item) })).filter(({ index }) => index >= 0);
  const filteredClips = clips.map((item, index) => ({ item, index })).filter(({ item }) => item.toLocaleLowerCase().includes(clipboardQuery.toLocaleLowerCase()));
  const shelf = state.shelf ?? [];
  const shelfIndexAtPoint = (x, y) => {
    const item = document.elementFromPoint(x, y)?.closest?.('[data-shelf-index]');
    const index = Number(item?.dataset.shelfIndex);
    return Number.isInteger(index) && index >= 0 ? index : null;
  };
  const beginShelfPointer = (event, index) => {
    if (event.button !== 0 || !event.isPrimary || event.target.closest?.('.shelf-remove-button')) return;
    shelfPointerRef.current = { pointerId: event.pointerId, from: index, to: index, x: event.clientX, y: event.clientY, dragging: false };
    try { event.currentTarget.setPointerCapture(event.pointerId); } catch {}
  };
  const moveShelfPointer = event => {
    const drag = shelfPointerRef.current;
    if (!drag || drag.pointerId !== event.pointerId) return;
    if (!drag.dragging && Math.hypot(event.clientX - drag.x, event.clientY - drag.y) < 7) return;
    drag.dragging = true;
    const next = shelfIndexAtPoint(event.clientX, event.clientY);
    if (next != null && next !== drag.to) {
      drag.to = next;
      setShelfDrag({ from: drag.from, to: next });
    }
    event.preventDefault();
  };
  const finishShelfPointer = event => {
    const drag = shelfPointerRef.current;
    if (!drag || drag.pointerId !== event.pointerId) return;
    if (!drag.dragging && Math.hypot(event.clientX - drag.x, event.clientY - drag.y) >= 7) {
      drag.dragging = true;
      drag.to = shelfIndexAtPoint(event.clientX, event.clientY) ?? drag.from;
    }
    if (drag.dragging) {
      event.preventDefault();
      suppressShelfClickRef.current = true;
      window.setTimeout(() => { suppressShelfClickRef.current = false; }, 350);
      if (drag.to !== drag.from) send('shelfMove', { from: drag.from, to: drag.to });
    }
    shelfPointerRef.current = null;
    setShelfDrag(null);
    try { if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId); } catch {}
  };
  const cancelShelfPointer = event => {
    if (shelfPointerRef.current?.pointerId !== event.pointerId) return;
    shelfPointerRef.current = null;
    setShelfDrag(null);
  };
  const activityLines = (state.activity ?? '').split('\n').filter(Boolean);
  const openCode = state.opencode ?? {};
  const openCodeSessions = openCode.sessions ?? [];
  const openCodeAgents = openCode.agents ?? [];
  const visibleOpenCodeAgents = openCodeAgents.filter(agent => !agent.hidden);
  const primaryOpenCodeAgents = visibleOpenCodeAgents.filter(agent => agent.mode !== 'subagent');
  const mentionableOpenCodeAgents = visibleOpenCodeAgents.filter(agent => agent.mode === 'subagent' || agent.mode === 'all');
  const selectedSession = openCodeMode === 'session'
    ? openCodeSessions.find(session => session.id === selectedOpenCodeSession)
      ?? (selectedOpenCodeSession ? { id: selectedOpenCodeSession, title: 'OpenCode session', directory: '', active: false, model: '', agent: '' } : null)
    : null;
  const selectedAgent = openCodeAgents.find(agent => agent.id === openCodeSelectedAgent) || openCodeAgents.find(agent => agent.id === selectedSession?.agent) || null;
  useEffect(() => {
    if (selectedSession) setOpenCodeSelectedAgent(selectedSession.agent || '');
    else if (openCodeMode === 'new') setOpenCodeSelectedAgent('');
  }, [selectedOpenCodeSession, openCodeMode, selectedSession?.agent]);
  const openCodeResponse = openCode.response?.sessionId === selectedOpenCodeSession ? openCode.response : null;
  const showOpenCodeResponse = displayPage === 'Agents' && openCodeResponseOpen && !!selectedSession;
  const openCodeSourcesList = useMemo(() => showOpenCodeResponse ? openCodeReplySources(openCodeResponse?.text || '') : [], [showOpenCodeResponse, openCodeResponse?.text]);
  const openCodeReplyBlocks = useMemo(() => showOpenCodeResponse ? renderOpenCodeReply(openCodeResponse?.text || '') : [], [showOpenCodeResponse, openCodeResponse?.text]);
  const openCodeModelDirectory = selectedSession?.directory || (openCodeMode === 'new' ? openCodeFolder : '');
  const openCodeModelsReady = openCode.connected && openCode.modelDirectory === openCodeModelDirectory;
  const selectedModelKey = selectedSession?.model || openCodeModel;
  const availableModels = openCodeModelsReady ? (openCode.models ?? []) : [];
  const shownModels = availableModels.filter(model => `${model.name} ${model.provider} ${model.key}`.toLocaleLowerCase().includes(openCodeModelQuery.toLocaleLowerCase()));
  const selectedModelName = availableModels.find(model => model.key === selectedModelKey)?.name || selectedModelKey || (openCode.defaultModel ? `Default · ${openCode.defaultModel}` : 'Default model');
  const openCodeMentionMatches = agentMention ? mentionableOpenCodeAgents.filter(agent => agent.id.toLocaleLowerCase().startsWith(agentMention.query.toLocaleLowerCase())).slice(0, 6) : [];
  const sessionMatches = openCodeSessions.filter(session => `${session.title} ${session.directory}`.toLocaleLowerCase().includes(openCodeSessionQuery.toLocaleLowerCase()));
  const permissionSessions = new Map(openCodeSessions.map(session => [session.id, session.title]));
  const sessionPermissions = selectedSession ? (openCode.permissions ?? []).filter(permission => permission.sessionId === selectedSession.id) : [];
  const otherPermissions = (openCode.permissions ?? []).filter(permission => permission.sessionId !== selectedSession?.id);
  useEffect(() => {
    if (!openCode.connected) return;
    send('opencodeCatalog', { directory: openCodeModelDirectory });
    setOpenCodeModelsOpen(false);
    setOpenCodeModel('');
  }, [openCode.connected, openCodeModelDirectory]);
  useEffect(() => {
    if (!openCode.operationRevision || !pendingOpenCodeSend.current || !['start', 'prompt'].includes(openCode.operationKind)) return;
    pendingOpenCodeSend.current = false;
    setOpenCodeSending(false);
    if (openCode.operationOk) {
      setOpenCodePrompt('');
      setOpenCodeFiles([]);
      setOpenCodeResponseOpen(true);
      setOpenCodeModelsOpen(false);
    }
  }, [openCode.operationRevision]);
  useEffect(() => {
    if (openCode.operationRevision && openCode.operationKind === 'abort') setOpenCodeStopping(false);
  }, [openCode.operationRevision]);
  useEffect(() => {
    if (openCode.operationRevision && openCode.operationKind === 'approval') setReplyingPermissionId('');
  }, [openCode.operationRevision]);
  useEffect(() => {
    if (!openCode.lastCreatedId || !openCode.operationOk) return;
    setSelectedOpenCodeSession(openCode.lastCreatedId);
    setOpenCodeMode('session');
    send('opencodeSelect', { sessionId: openCode.lastCreatedId });
  }, [openCode.lastCreatedId]);
  const submitOpenCode = event => {
    event.preventDefault();
    const text = openCodePrompt.trim();
    if ((!text && !openCodeFiles.length) || !openCode.connected || openCodeSending) return;
    pendingOpenCodeSend.current = true;
    setOpenCodeSending(true);
    setAgentMention(null);
    const agent = openCodeSelectedAgent || selectedSession?.agent || '';
    if (selectedSession) send('opencodePrompt', { sessionId: selectedSession.id, text, agent, files: openCodeFiles });
    else send('opencodeStart', { directory: openCodeFolder, text, model: openCodeModelsReady ? openCodeModel : '', agent, files: openCodeFiles });
  };
  const updateOpenCodePrompt = event => {
    const value = event.target.value;
    const caret = event.target.selectionStart ?? value.length;
    setOpenCodePrompt(value);
    const match = value.slice(0, caret).match(/(?:^|[^a-zA-Z0-9_-])@([a-zA-Z0-9_-]*)$/);
    if (match) setAgentMention({ query: match[1], start: caret - match[1].length - 1, end: caret });
    else setAgentMention(null);
    setAgentMentionIndex(0);
  };
  const insertOpenCodeMention = agent => {
    if (!agentMention) return;
    const mention = `@${agent.id} `;
    const next = `${openCodePrompt.slice(0, agentMention.start)}${mention}${openCodePrompt.slice(agentMention.end)}`;
    const caret = agentMention.start + mention.length;
    setOpenCodePrompt(next);
    setAgentMention(null);
    requestAnimationFrame(() => { openCodePromptRef.current?.focus(); openCodePromptRef.current?.setSelectionRange(caret, caret); });
  };
  const chooseOpenCodeModel = key => {
    if (selectedSession) {
      const model = key || openCode.defaultModel;
      if (model) send('opencodeSetModel', { sessionId: selectedSession.id, model });
    } else {
      setOpenCodeModel(key);
    }
    setOpenCodeModelsOpen(false);
  };
  const startOpenCodeFollowup = () => {
    setOpenCodeResponseOpen(false);
    requestAnimationFrame(() => openCodePromptRef.current?.focus());
  };
  const replyOpenCodePermission = (permission, allow) => {
    setReplyingPermissionId(permission.id);
    send('opencodeReply', { sessionId: permission.sessionId, requestId: permission.id, allow });
  };
  const stopOpenCodeRun = () => {
    if (!selectedSession?.id || openCodeStopping) return;
    setOpenCodeStopping(true);
    send('opencodeAbort', { sessionId: selectedSession.id });
  };
  const privacyLines = activityLines.filter(line => /^(Microphone|Camera)\s*[·]/.test(line));
  const forecast = state.weatherData;
  const current = forecast?.current;
  const daily = forecast?.daily;
  const weekLow = Math.min(...(daily?.temperature_2m_min ?? [0]));
  const weekHigh = Math.max(...(daily?.temperature_2m_max ?? [1]));
  const mediaDurationMs = state.media?.duration_ms ?? 0;
  const outputVolume = volumeDraft ?? state.outputVolume ?? 0;
  const outputDeviceLabel = state.selectedOutputDeviceId
    ? (state.outputDevices ?? []).find(device => device.id === state.selectedOutputDeviceId)?.name ?? 'Audio output'
    : 'System default';
  const mediaPositionMs = Math.min(mediaDurationMs || Number.MAX_SAFE_INTEGER,
    (state.media?.position_ms ?? 0) + (state.media?.playing && mediaReceivedAt ? Math.max(0, mediaClock - mediaReceivedAt) : 0));
  const mediaSeekTo = positionMs => {
    if (!mediaDurationMs || !state.media?.can_seek) return;
    const nextPosition = Math.max(0, Math.min(mediaDurationMs, Math.round(positionMs)));
    if (seekPendingRef.current === nextPosition && seekTimerRef.current) return;
    seekPendingRef.current = nextPosition;
    setSeekDraftMs(nextPosition);
    clearTimeout(seekTimerRef.current);
    seekTimerRef.current = setTimeout(() => {
      seekPendingRef.current = null;
      setSeekDraftMs(null);
      seekTimerRef.current = null;
    }, 2200);
    send('mediaSeek', { positionMs: nextPosition });
  };
  useEffect(() => {
    const target = seekPendingRef.current;
    const observed = state.media?.position_ms;
    if (target == null || observed == null || Math.abs(observed - target) > 1800) return;
    clearTimeout(seekTimerRef.current);
    seekTimerRef.current = null;
    seekPendingRef.current = null;
    setSeekDraftMs(null);
  }, [state.media?.position_ms]);
  const flushVolume = () => {
    clearTimeout(volumeTimerRef.current);
    volumeTimerRef.current = null;
    const value = volumeDraftRef.current;
    if (value == null) return;
    volumeLastSentRef.current = { value, at: performance.now() };
    send('outputVolume', { value });
  };
  const changeVolume = (value, immediate = false) => {
    const next = Math.max(0, Math.min(100, Math.round(value)));
    volumeDraftRef.current = next;
    setVolumeDraft(next);
    if (immediate) { flushVolume(); return; }
    if (volumeTimerRef.current == null) volumeTimerRef.current = setTimeout(flushVolume, 16);
  };
  useLayoutEffect(() => {
    const nav = navRef.current;
    const indicator = nav?.querySelector('.tab-indicator');
    const selected = [...(nav?.querySelectorAll('.tab') ?? [])].find(button => button.dataset.page === page);
    if (!nav || !indicator || !selected) return;
    const spring = tabSpring.current;
    const targetX = selected.offsetLeft;
    const targetWidth = selected.offsetWidth;
    const paint = () => {
      indicator.style.transform = `translate3d(${spring.x}px, 0, 0)`;
      indicator.style.width = `${spring.width}px`;
    };
    cancelAnimationFrame(spring.frame);
    if (!spring.placed || spring.node !== indicator || state.shellSettings?.reducedMotion || window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
      spring.node = indicator;
      Object.assign(spring, { x: targetX, width: targetWidth, vx: 0, vw: 0, placed: true });
      paint();
      return;
    }
    spring.last = performance.now();
    const tick = now => {
      const elapsed = Math.min((now - spring.last) / 1000, .04);
      spring.last = now;
      const steps = Math.max(1, Math.ceil(elapsed * 120));
      const dt = elapsed / steps;
      for (let i = 0; i < steps; i++) {
        spring.vx += (-500 * (spring.x - targetX) - 48 * spring.vx) * dt;
        spring.vw += (-500 * (spring.width - targetWidth) - 48 * spring.vw) * dt;
        spring.x += spring.vx * dt;
        spring.width += spring.vw * dt;
      }
      if (Math.abs(spring.x - targetX) < .15 && Math.abs(spring.width - targetWidth) < .15
          && Math.abs(spring.vx) < .5 && Math.abs(spring.vw) < .5) {
        Object.assign(spring, { x: targetX, width: targetWidth, vx: 0, vw: 0, frame: 0 });
        paint();
      } else {
        paint();
        spring.frame = requestAnimationFrame(tick);
      }
    };
    spring.frame = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(spring.frame);
  }, [page, visible.join('|'), state.expanded, settingsOpen, state.shellSettings?.reducedMotion]);
  const rainCandidate = upcomingRain(state.weatherData, weatherClockNow);
  const updateCandidates = [
    state.opencode?.updateAvailable && !state.opencode?.installing && { id: 'opencode', label: 'OpenCode', version: state.opencode.latestVersion, busy: !!(state.opencode.activeCount || state.agents?.opencode?.activeCount) },
    state.agents?.codex?.updateAvailable && !state.agents.codex.installing && { id: 'codex', label: 'Codex ACP', version: state.agents.codex.latestVersion, busy: !!state.agents.codex.activeCount },
    state.conversion?.updateAvailable && !state.conversion.engineBusy && { id: 'ffmpeg', label: 'FFmpeg', version: state.conversion.latestVersion, busy: !!state.conversion.busy },
  ].filter(Boolean);
  const actionableUpdates = updateCandidates.filter(item => !item.busy);
  const visibleUpdates = actionableUpdates.length ? actionableUpdates : updateCandidates;
  const updateCandidate = visibleUpdates[updateRotation % visibleUpdates.length];
  const updateKey = updateCandidates.map(item => `${item.id}:${item.version}`).join('|');
  useEffect(() => {
    if (!updateKey || state.expanded) return undefined;
    setUpdatePulseOn(true);
    let timer;
    const hide = () => { setUpdatePulseOn(false); timer = setTimeout(show, 300_000); };
    const show = () => { setUpdateRotation(index => index + 1); setUpdatePulseOn(true); timer = setTimeout(hide, 6_000); };
    timer = setTimeout(hide, 6_000);
    return () => clearTimeout(timer);
  }, [updateKey, state.expanded]);
  useEffect(() => {
    if (!rainCandidate || state.expanded) return undefined;
    setRainPulseOn(true);
    let timer;
    const hide = () => { setRainPulseOn(false); timer = setTimeout(show, 180_000); };
    const show = () => { setRainPulseOn(true); timer = setTimeout(hide, 4_500); };
    timer = setTimeout(hide, 4_500);
    return () => clearTimeout(timer);
  }, [rainCandidate?.time, rainCandidate?.active, state.expanded]);
  const compactStatus = resolveCompactStatus({
    recording: !!screenRecordingAt,
    recordingElapsed: screenRecordingElapsed,
    recordingStopping: screenRecordingStopping,
    feedbacks: state.compactFeedbacks || [],
    media: state.media?.playing ? state.media : null,
    activeRuns: state.activeRuns,
    activeSession: (() => { const runs = state.agentRuns || []; const run = runs.find(item => item.provider === readAgentPreferences().defaultProvider) || runs[0]; const session = state.agents?.[run?.provider]?.sessions?.find(item => item.id === run?.sessionId); return run ? { title: run.label, provider: run.provider, sessionId: run.sessionId, childCount: session?.childCount || 0 } : null; })(),
    agentApproval: state.agentApproval,
    rain: rainPulseOn ? rainCandidate : null,
    update: updatePulseOn ? updateCandidate : null,
  }, compactPriorityOrder);
  useEffect(() => {
    send('compactStatusVisible', {
      id: compactStatus.id,
      feedbackId: compactStatus.payload?.feedback?.id ?? null,
      visible: !state.expanded && !state.compactFaded,
    });
  }, [compactStatus.id, compactStatus.payload?.feedback?.id, state.expanded, state.compactFaded]);
  return <div data-reduced-motion={!!state.shellSettings?.reducedMotion} className={'island accent-' + accentGradient + ' ' + (state.dark ? 'dark' : 'light') + (state.expanded ? ' expanded' : ' compact') + ((state.shellSettings?.shape || 'notch') === 'capsule' ? ' capsule-shape' : ' notch-shape') + ' ' + shellPhase + (state.compactFaded ? ' compact-faded' : '') + (fileDragOver ? ' file-dragging' : '') + (state.wakeWord?.surface ? ' voice-surface-open' : '') + (settingsOpen ? ' settings-open' : '') + (settingsClosing ? ' settings-closing' : '') + (settingsReturning ? ' settings-returning' : '')}
    onFocusCapture={event => { if (isEditor(event.target)) send('textFocus'); }}
    onBlurCapture={event => { if (isEditor(event.target)) queueMicrotask(() => { if (!isEditor(document.activeElement)) send('textBlur'); }); }}>
    {!state.expanded ? <CompactTransition state={state} compactStatus={compactStatus} batteryView={batteryView} screenRecordingAt={screenRecordingAt} screenRecordingElapsed={screenRecordingElapsed} screenRecordingStopping={screenRecordingStopping} contextTools={contextTools} onRunContextTool={runContextAction} /> : <>
    <header className={'bar ' + (settingsOpen ? 'settings-bar' : '')}>
      {settingsOpen ? <div className="settings-topbar"><button type="button" aria-label="Back to app" title="Back to app" onClick={closeSettingsWithMotion}><Icon name="PreviousMonth" size={17} /></button><strong>Settings</strong><span>{({ general: 'General', compact: 'Status', priority: 'Priority', behavior: 'Motion', tools: 'Tools', 'quick-actions': 'Quick actions', agents: 'Agents', dictation: 'Dictation', voice: 'Voice', ask: 'Ask', conversion: 'Conversion' })[settingsSection]}</span></div> : <nav ref={navRef} aria-label="Island pages"><span className="tab-indicator" aria-hidden="true" />{visible.map(name => <button key={name} data-page={name} className={'tab ' + (page === name ? 'active' : '')} title={name} aria-label={name} aria-current={page === name ? 'page' : undefined} onClick={() => nav(name)}><Icon name={name} /><span className="tab-label">{name}</span></button>)}</nav>}
      <button className={'icon-button ' + (state.pinned ? 'pinned' : '')} title={state.pinned ? 'Unpin island' : 'Keep island open'} aria-label={state.pinned ? 'Unpin island' : 'Keep island open'} aria-pressed={!!state.pinned} onClick={() => send('pinToggle')}>
        <Icon name="Pin" />
      </button>
      {!settingsOpen && <button className={'icon-button ' + (settingsOpen ? 'pinned' : '')} title="Island settings" aria-label="Island settings" aria-pressed={settingsOpen} onClick={() => { setSignOpen(false); setDictateOpen(false); setScreenSelection(null); setCommandOpen(false); setSettingsOpen(open => !open); }}><Icon name="Settings" /></button>}
    </header>
    {state.dropActive && <div className={'convert-native-drop intent-' + state.dropIntent} role="group" aria-label="Choose a drop destination" aria-live="polite"><div className={state.dropIntent === 1 ? 'active' : state.dropIntent === 3 ? 'is-disabled' : ''}><Icon name="File" size={23} /><strong>Convert</strong><small>{state.dropIntent === 3 ? 'Files only' : 'Resize or change format'}</small></div><div className={state.dropIntent === 2 ? 'active' : ''}><Icon name="Shelf" size={23} /><strong>Shelf</strong><small>Save for later</small></div></div>}
    <main className="body" ref={bodyRef}><div className="page-content entering" key={displayPage}><PageBoundary key={`${displayPage}:${settingsOpen ? settingsSection : 'page'}:${commandOpen}:${signOpen}`}>
      {settingsOpen ? <SettingsPanel state={state} batteryView={batteryView} setBatteryView={setBatteryView} accentGradient={accentGradient} setAccentGradient={setAccentGradient} mediaCardStyle={mediaCardStyle} setMediaCardStyle={setMediaCardStyle} homeActions={homeActions} setHomeActions={setHomeActions} compactPriorityOrder={compactPriorityOrder} setCompactPriorityOrder={setCompactPriorityOrder} section={settingsSection} setSection={setSettingsSection} sectionScrollPositions={settingsScrollPositions} /> : <>
      {displayPage === 'Home' && (commandOpen ? <CommandPage query={commandQuery} setQuery={setCommandQuery} state={state} onClose={() => setCommandOpen(false)} onRoute={routeCommand} /> : signOpen ? <div className="home-layout sign-home-layout"><SignaturePanel onBack={() => setSignOpen(false)} strokesRef={signatureStrokesRef} /></div> : <div className="home-layout" data-layout-height={dictateOpen && dictateMode === 'home' && dictateTranscriptExpanded ? 380 : undefined}>
        <div className={'home-grid ' + (!screenSelection && !dictateOpen && state.media ? 'has-media ' : '') + (screenSelection ? 'is-tool-selecting ' : '') + (dictateOpen ? 'is-dictating' : '')}>
          {dictateOpen ? <div className="home-dictate-slot" ref={homeDictateSlotRef} /> : screenSelection ? <ScreenSelectionSetup mode={screenSelection} onBack={() => setScreenSelection(null)} onSelect={beginScreenTool} recordQuality={recordQuality} onRecordQualityChange={quality => { setRecordQuality(quality); try { window.localStorage.setItem('island.screen-record-quality.v1', quality); } catch {} }} /> : <>
          <section className={'home-primary ' + (state.media ? `has-player media-card-${mediaCardStyle}-mode ` : '') + (state.media?.artwork && mediaCardStyle === 'backdrop' ? 'media-card-has-artwork ' : '') + (!state.media && !state.activeRuns && !homeClips.length ? 'is-welcome' : '')} aria-label={state.media ? 'Now playing' : state.activeRuns ? 'Agent activity' : homeClips.length ? 'Recent clipboard' : 'Welcome'}>
            {state.media && mediaCardStyle === 'backdrop' && state.media.artwork && <div className="media-backdrop" aria-hidden="true"><img src={state.media.artwork} alt="" draggable="false" /><span /></div>}
            {(state.media || state.activeRuns || homeClips.length > 0) && <div className="primary-heading"><span className="eyebrow">{state.media ? state.media.playing ? 'Now playing' : 'Paused' : state.activeRuns ? 'Agents running' : 'Recent clipboard'}</span>{state.media ? <>{(state.media.sessions ?? []).length > 1 ? <MediaSessionSwitcher sessions={state.media.sessions} selectedId={state.media.session_id} onSelect={id => send('mediaSelect', { id })} reducedMotion={!!state.shellSettings?.reducedMotion} /> : <span className="primary-count">{mediaSourceName(state.media.source)}</span>}</> : <span className="primary-count">{state.activeRuns ? `${state.activeRuns} agents` : homeClips.length}</span>}</div>}
            {state.media ? <div className={'media-content ' + (mediaCardStyle === 'backdrop' ? 'media-content-backdrop' : '')}>
              <div className={'media-art ' + (state.media.artwork ? 'has-art' : '')}>
                {state.media.artwork ? <img src={state.media.artwork} alt="Album artwork" /> : <Icon name="Play" size={24} />}
              </div>
              <div className="media-main">
                <div className="media-title"><strong title={state.media.title || mediaSourceName(state.media.source)}>{state.media.title || mediaSourceName(state.media.source)}</strong>{state.media.artist && <span title={state.media.artist}>{state.media.artist}</span>}</div>
                <div className="media-progress" aria-label="Playback timeline">
                  <input className="media-seek" type="range" min="0" max={mediaDurationMs || 1} step="500" aria-label="Seek in current media" disabled={!state.media.can_seek} value={seekDraftMs ?? mediaPositionMs} style={{ '--range-progress': `${mediaDurationMs ? Math.min(100, (seekDraftMs ?? mediaPositionMs) / mediaDurationMs * 100) : 0}%` }} onChange={event => setSeekDraftMs(Number(event.currentTarget.value))} onPointerUp={event => mediaSeekTo(Number(event.currentTarget.value))} onPointerCancel={event => mediaSeekTo(Number(event.currentTarget.value))} onKeyUp={event => mediaSeekTo(Number(event.currentTarget.value))} onBlur={event => { if (seekDraftMs != null) mediaSeekTo(Number(event.currentTarget.value)); }} />
                  <div className="media-times"><small>{mediaTime(seekDraftMs ?? mediaPositionMs)}</small><small>{mediaTime(mediaDurationMs || null)}</small></div>
                </div>
                <div className="media-controls">
                  <span className="media-control-side">{state.media.can_seek && <button type="button" aria-label="Back 10 seconds" title="Back 10 seconds" onClick={() => mediaSeekTo(mediaPositionMs - 10000)}><Icon name="Back10" size={18} /></button>}</span>
                  <span className="media-control-center">
                    {state.media.can_previous && <button type="button" aria-label="Previous track" title="Previous track" onClick={() => send('mediaPrevious')}><Icon name="Previous" size={18} /></button>}
                    {state.media.can_toggle && <button type="button" className="media-play" aria-label={state.media.playing ? 'Pause' : 'Play'} title={state.media.playing ? 'Pause' : 'Play'} onClick={() => send('mediaToggle')}><Icon name={state.media.playing ? 'Pause' : 'Play'} size={18} /></button>}
                    {state.media.can_next && <button type="button" aria-label="Next track" title="Next track" onClick={() => send('mediaNext')}><Icon name="Next" size={18} /></button>}
                  </span>
                  <span className="media-control-side">{state.media.can_seek && <button type="button" aria-label="Forward 10 seconds" title="Forward 10 seconds" onClick={() => mediaSeekTo(mediaPositionMs + 10000)}><Icon name="Forward10" size={18} /></button>}</span>
                </div>
              </div>
            </div>
              : state.activeRuns ? <><div className="home-agent-status"><div><strong className="primary-title">{state.status || 'Agent activity'}</strong><p className="primary-detail">{state.activity || 'Waiting for the next provider update'}</p></div></div><button className="primary-action" onClick={() => nav('Agents')}>View activity</button></>
              : homeClips.length ? <div className="recent-clips">{homeClips.slice(0, 3).map(({ item, index }) => <button key={index} className="recent-clip" title="Copy to clipboard" onClick={() => send('clipboardCopy', { index })}><span>{preview(item)}</span><small>Copy</small></button>)}</div>
                : <div className="home-welcome"><strong className="primary-title">Hello, what’s up?</strong><form className="home-launcher" onSubmit={event => { event.preventDefault(); send('commandClear'); send('commandAppSearch', { query: commandQuery }); setCommandOpen(true); }}><Icon name="Search" size={19} /><input aria-label="Search files, apps, and actions" placeholder="Find a file, app, or action" value={commandQuery} onChange={event => setCommandQuery(event.target.value)} /><button type="submit" aria-label="Open Command"><Icon name="Command" size={17} /></button></form></div>}
          </section>
          <section className="actions" aria-label="Quick actions">
            {homeActions.map((id, index) => {
              const action = homeActionCatalog.find(candidate => candidate.id === id);
              return action
                ? <button type="button" key={`slot-${index}`} onClick={() => runHomeAction(id)} title={action.detail}><Icon name={action.icon} /><span>{action.label}</span></button>
                : <div key={`slot-${index}`} className="actions-empty-slot" aria-label={`Empty quick action slot ${index + 1}`} />;
            })}
          </section>
          </>}
        </div>
        {!dictateOpen && !screenSelection && !signOpen && screenResult && <ScreenResultCard result={screenResult} colors={screenColors} onDismiss={() => setScreenResult(null)} />}
        {!(dictateOpen && dictateMode === 'home' && dictateTranscriptExpanded) && <div className="output-row">
          <Icon name="Volume" size={17} />
          <input className="volume-range" type="range" min="0" max="100" aria-label="Output device volume" value={outputVolume} disabled={state.outputVolume == null} style={{ '--range-progress': `${outputVolume}%` }} onChange={event => changeVolume(Number(event.target.value))} onPointerDown={event => { volumeDraggingRef.current = true; event.currentTarget.setPointerCapture(event.pointerId); const rect = event.currentTarget.getBoundingClientRect(); changeVolume((event.clientX - rect.left) / rect.width * 100); }} onPointerMove={event => { if (!volumeDraggingRef.current) return; const rect = event.currentTarget.getBoundingClientRect(); changeVolume((event.clientX - rect.left) / rect.width * 100); }} onPointerUp={event => { if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId); volumeDraggingRef.current = false; const rect = event.currentTarget.getBoundingClientRect(); changeVolume((event.clientX - rect.left) / rect.width * 100, true); }} onPointerCancel={() => { volumeDraggingRef.current = false; flushVolume(); }} onKeyDown={() => { volumeDraggingRef.current = true; }} onKeyUp={() => { volumeDraggingRef.current = false; flushVolume(); }} onBlur={() => { volumeDraggingRef.current = false; flushVolume(); }} />
          <span>{state.outputVolume == null ? '—' : `${outputVolume}%`}</span>
          <div className="device-picker">
            <button className="device-picker-trigger" type="button" aria-label="Select output device" aria-haspopup="listbox" aria-expanded={outputPickerOpen} title="Select the audio device whose volume Island controls" onClick={() => setOutputPickerOpen(open => !open)}>
              <Icon name="Speaker" size={15} /><span>{outputDeviceLabel}</span><i aria-hidden="true" />
            </button>
            {outputPickerOpen && <div className="device-menu" role="listbox" aria-label="Audio output device">
              <button type="button" role="option" aria-selected={!state.selectedOutputDeviceId} onClick={() => { send('outputDevice', { id: '' }); setOutputPickerOpen(false); }}><span>System default</span>{!state.selectedOutputDeviceId && <small>Selected</small>}</button>
              {(state.outputDevices ?? []).map(device => <button type="button" role="option" aria-selected={state.selectedOutputDeviceId === device.id} key={device.id} onClick={() => { send('outputDevice', { id: device.id }); setOutputPickerOpen(false); }}><span>{device.name}</span><small>{device.id === state.defaultOutputDeviceId ? 'Default' : state.selectedOutputDeviceId === device.id ? 'Selected' : ''}</small></button>)}
              {!(state.outputDevices ?? []).length && <p>No audio outputs found</p>}
            </div>}
          </div>
        </div>}
        <div className={'home-strip ' + (fileDragOver ? 'file-dragover' : '')} aria-label="Shelf dock"><button className="strip-heading" onClick={() => nav('Shelf')} title="Open Shelf"><Icon name="Shelf" size={18} /><strong>Shelf</strong><small>{shelf.length}</small></button><div className="strip-files">{fileDragOver ? <span className="strip-drop-message">Release to add to Shelf</span> : shelf.length ? shelf.slice(0, 4).map((item, index) => <button className="strip-file" key={index} title="Open saved item" aria-label={`Open shelf item ${index + 1}`} onClick={() => send('shelfOpen', { index })}><ShelfVisual kind={state.shelfKinds?.[index]} visual={state.shelfIcons?.[index]} compact /></button>) : <span className="strip-empty">Drag files or shortcuts onto the island</span>}</div></div>
      </div>)}
      {displayPage === 'Clipboard' && <section className="stack">
        <div className="list-toolbar clipboard-toolbar"><div className="clipboard-search"><Icon name="Search" size={16} /><input type="search" aria-label="Search clipboard history" placeholder="Search clipboard" value={clipboardQuery} onChange={event => setClipboardQuery(event.target.value)} /></div><span className="clipboard-count">{filteredClips.length}</span>{clips.length > 0 && <button className="subtle-action" onClick={() => send('clipboardClear')}>Clear</button>}</div>
        <div className="scroll-list clipboard-list">{filteredClips.length ? filteredClips.map(({ item, index }) => {
          const hexMatch = item.match(/^#([\da-f]{6})$/i);
          const rgbMatch = item.match(/^rgb\(\s*(\d{1,3})\s*,\s*(\d{1,3})\s*,\s*(\d{1,3})\s*\)$/i);
          const channels = hexMatch
            ? [1, 3, 5].map(offset => parseInt(item.slice(offset, offset + 2), 16))
            : rgbMatch ? rgbMatch.slice(1).map(Number) : null;
          const isColor = !!channels && channels.every(channel => channel >= 0 && channel <= 255);
          const colorHex = isColor ? `#${channels.map(channel => channel.toString(16).padStart(2, '0')).join('').toUpperCase()}` : '';
          const colorRgb = isColor ? `rgb(${channels.join(', ')})` : '';
          return <article className={'clip-card ' + (isColor ? 'is-color-clip' : '')} key={index}>
            <span className="clip-card-glyph">{isColor ? <i className="clipboard-color-swatch" style={{ '--clip-color': colorHex }} /> : <Icon name="Text" size={20} />}</span>
            <div className="clip-card-content"><small>{isColor ? 'COLOR' : 'TEXT'}</small><strong>{isColor ? colorHex : preview(item)}</strong>
              <div className="clip-card-actions">{isColor ? <>
                <button type="button" onClick={() => send('clipboardWriteText', { value: colorHex })}><Icon name="Copy" size={13} />HEX</button>
                <button type="button" onClick={() => send('clipboardWriteText', { value: colorRgb })}><Icon name="Copy" size={13} />RGB <span>{channels.join(', ')}</span></button>
              </> : <button type="button" onClick={() => send('clipboardCopy', { index })}><Icon name="Copy" size={13} />Copy text</button>}</div>
            </div>
          </article>;
        }) : <div className="empty-state"><Icon name="Clipboard" size={26} /><strong>{clips.length ? 'No match' : 'Your clipboard is clear'}</strong><span>{clips.length ? 'Try a different search.' : state.clipboardCapturing ? 'Text you copy will appear here automatically.' : 'Turn on clipboard capture in Settings.'}</span></div>}</div>
      </section>}
      {displayPage === 'Convert' && <ConversionWorkspace state={state} send={send} Icon={Icon} onSettings={() => { setSettingsSection('conversion'); setSettingsOpen(true); }} />}
      {displayPage === 'Shelf' && <section className="stack">
        <div className="section-head"><span className="eyebrow">FILES AT HAND · {shelf.length}</span><button className="subtle-action" onClick={() => nav('Convert')}>Convert files</button>{shelf.length > 0 && <button className="subtle-action" onClick={() => send('shelfClear')}>Clear shelf</button>}</div>
        <div className={'shelf-grid scroll-list ' + (shelf.length ? '' : 'is-empty')}>{shelf.length ? shelf.map((item, index) => <div className={'shelf-item ' + (shelfDrag?.from === index ? 'is-dragging ' : '') + (shelfDrag?.from !== shelfDrag?.to && shelfDrag?.to === index ? 'is-drop-target' : '')} key={`${item}-${index}`} data-shelf-index={index} tabIndex={0} role="group" aria-keyshortcuts="Alt+ArrowLeft Alt+ArrowRight Alt+ArrowUp Alt+ArrowDown" aria-label={`Shelf item ${index + 1}; drag to reorder, or use Alt plus an arrow key`}
          onKeyDown={event => { if (!event.altKey || !['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown'].includes(event.key)) return; event.preventDefault(); const to = Math.max(0, Math.min(shelf.length - 1, index + (['ArrowLeft', 'ArrowUp'].includes(event.key) ? -1 : 1))); if (to !== index) send('shelfMove', { from: index, to }); }}>
          <button className="shelf-file" type="button" draggable="false" title="Drag to reorder · click to open" aria-label={`Open ${item || `shelf item ${index + 1}`}; drag to reorder`} onPointerDown={event => beginShelfPointer(event, index)} onPointerMove={moveShelfPointer} onPointerUp={finishShelfPointer} onPointerCancel={cancelShelfPointer} onClick={event => { if (suppressShelfClickRef.current) { event.preventDefault(); event.stopPropagation(); suppressShelfClickRef.current = false; return; } send('shelfOpen', { index }); }}><ShelfVisual kind={state.shelfKinds?.[index]} visual={state.shelfIcons?.[index]} /></button>
          <button type="button" draggable="false" className="shelf-remove-button" title="Remove from Shelf" aria-label={`Remove ${item || `shelf item ${index + 1}`} from Shelf`} onClick={event => { event.stopPropagation(); send('shelfRemove', { index }); }}><span aria-hidden="true">×</span></button>
        </div>) : <div className={'empty-state shelf-empty ' + (fileDragOver ? 'file-dragover' : '')}><Icon name="Shelf" size={27} /><strong>{fileDragOver ? 'Drop here' : 'Drop files onto the island'}</strong><span>They stay on this shelf until you clear it.</span></div>}</div>
      </section>}
      {displayPage === 'Text' && <section className="editor-layout">
        <div className="editor-column"><div className="section-head"><span className="eyebrow">SCRATCHPAD</span><small>{text.length} chars · {text.trim() ? text.trim().split(/\s+/).length : 0} words · {text ? text.split('\n').length : 0} lines</small></div><textarea aria-label="Text scratchpad" value={text} onChange={event => { setText(event.target.value); send('textSet', { value: event.target.value }); }} placeholder="Type, paste, or bring in clipboard text…" /></div>
        <div className="tool-grid"><span className="tool-heading">TOOLS</span>{[['Paste text','textReadClipboard'],['Copy text','textCopy'],['Uppercase','textUpper'],['Lowercase','textLower'],['Trim space','textTrim'],['Clear','textClear']].map(([label, kind]) => <button key={kind} onClick={() => send(kind)}>{label}</button>)}{futureAction('Title case')}{futureAction('JSON format')}{futureAction('Base64')}{futureAction('Make QR')}</div>
      </section>}
      {['Agents', 'Ask'].includes(displayPage) && <AgentsWorkspace voiceSurface={!!state.wakeWord?.surface} askMode={displayPage === 'Ask'} state={state} Icon={Icon} send={send} onDictate={options => { const autoSubmit = options?.autoSubmit === true; setHoldCapture(options?.hold === true); if (options?.hold) window.islandHoldReleased = false; if ((!autoSubmit || options?.hold) && ['arming', 'listening'].includes(state.wakeWord?.status)) send('wakeWordSet', { enabled: false, phrase: state.wakeWord?.phrase }); setAgentDictateAutoSubmit(autoSubmit); setSettingsOpen(false); setDictateMode('agent'); setDictateOpen(true); }} dictateActive={dictateOpen && dictateMode === 'agent'} dictateSlotRef={agentDictateSlotRef} dictatedText={agentDictatedText} dictatedAutoSubmit={agentDictateAutoSubmit} onDictatedTextConsumed={() => { setAgentDictatedText(''); setAgentDictateAutoSubmit(false); }} renderAnswer={renderOpenCodeReply} openCodeProfiles={<OpenCodeAgentManager agents={visibleOpenCodeAgents} models={openCode.models || []} notice={openCode.notice} error={openCode.agentError} onRefresh={() => send('opencodeAgentsRefresh')} onSave={profile => send('opencodeSaveAgent', profile)} onDelete={id => send('opencodeDeleteAgent', { id })} />} />}
      {displayPage === 'Note' && <section className="note-workspace">
        <div className="note-mode-switch" role="tablist" aria-label="Notes and reminders"><button type="button" role="tab" aria-selected={noteView === 'notes'} className={noteView === 'notes' ? 'active' : ''} onClick={() => { setNoteView('notes'); setShowReminderCalendar(false); }}>Notes</button><button type="button" role="tab" aria-selected={noteView === 'reminders'} className={noteView === 'reminders' ? 'active' : ''} onClick={() => { setNoteView('reminders'); setShowReminderCalendar(false); }}>Reminders{state.noteReminderAt && <i className={state.noteReminderDue ? 'is-due' : ''} />}</button></div>
        {showReminderCalendar ? <ReminderCalendar initialAt={state.noteReminderAt} backLabel={noteView === 'notes' ? 'Notes' : 'Reminders'} onClose={closeReminderCalendar} onSet={at => { send('noteReminderSet', { at }); setShowReminderCalendar(false); }} /> : noteView === 'notes' ? <>
          <div className="note-status"><span>Saved automatically</span><small>{note.length} characters</small></div>
          <div className="folder-row">{(state.noteFolders ?? ['Notes']).map((folder, index) => <button key={index} className={'folder-chip ' + (state.noteFolderSelected === index ? 'active' : '')} onClick={() => { send('noteFolderSelect', { index }); setFolderEditing(false); setFolderDeleteArmed(false); }}>{folder}</button>)}<button className="add-folder" aria-label="Add folder" title="Add folder" onClick={() => { setShowFolderInput(value => !value); setFolderEditing(false); }}><Icon name="FolderAdd" size={16} /></button><button className="folder-manage" aria-label="Rename selected folder" title="Rename folder" onClick={() => { setFolderName(state.noteFolders?.[state.noteFolderSelected] ?? 'Notes'); setFolderEditing(true); setShowFolderInput(false); }}><Icon name="Edit" size={15} /></button>{(state.noteFolders?.length ?? 1) > 1 && <button className="folder-manage" aria-label={folderDeleteArmed ? 'Confirm delete folder' : 'Delete selected folder'} title="Delete folder" onClick={() => { if (folderDeleteArmed) { send('noteFolderDelete', { index: state.noteFolderSelected }); setFolderDeleteArmed(false); } else setFolderDeleteArmed(true); }}>{folderDeleteArmed ? 'Delete?' : <Icon name="Trash" size={15} />}</button>}</div>
          {(showFolderInput || folderEditing) && <form className="new-folder" onSubmit={event => { event.preventDefault(); if (folderName.trim()) { send(folderEditing ? 'noteFolderRename' : 'noteFolderAdd', { index: state.noteFolderSelected, name: folderName.trim() }); setFolderName(''); setShowFolderInput(false); setFolderEditing(false); } }}><input autoFocus maxLength={48} aria-label={folderEditing ? 'Rename folder' : 'New folder name'} placeholder="Folder name" value={folderName} onChange={event => setFolderName(event.target.value)} /><button type="submit">{folderEditing ? 'Save' : 'Add'}</button></form>}
          <textarea className="note-editor" aria-label="Local note" value={note} onChange={event => { setNote(event.target.value); send('noteSet', { value: event.target.value }); }} placeholder="Keep a thought close…" />
          <div className="note-bottom"><button className={'reminder-pill ' + (state.noteReminderDue ? 'due' : '')} onClick={() => setShowReminderCalendar(true)}><Icon name="Calendar" size={17} /><span>{state.noteReminderAt ? new Date(state.noteReminderAt * 1000).toLocaleString([], { dateStyle: 'medium', timeStyle: 'short' }) : 'Add reminder'}</span></button>{state.noteReminderAt && <button className="note-tool" title="Remove reminder" aria-label="Remove reminder" onClick={() => send('noteReminderClear')}>×</button>}<span className="note-bottom-spacer" /><button className="note-tool" title="Copy note" aria-label="Copy note" onClick={() => send('noteCopy')}><Icon name="Copy" size={16} /></button><button className="note-tool" title={noteClearArmed ? 'Confirm clear note' : 'Clear note'} aria-label={noteClearArmed ? 'Confirm clear note' : 'Clear note'} onClick={() => { if (noteClearArmed) { send('noteClear'); setNoteClearArmed(false); } else setNoteClearArmed(true); }}>{noteClearArmed ? 'Clear?' : <Icon name="Trash" size={16} />}</button></div>
        </> : <div className="reminders-view">
          <div className="reminders-heading"><div><span className="eyebrow">NOTEBOOK REMINDER</span><strong>{state.noteReminderDue ? 'Reminder is due' : state.noteReminderAt ? 'Scheduled reminder' : 'Nothing scheduled'}</strong></div><Icon name="Reminder" size={20} /></div>
          {state.noteReminderAt ? <div className={'reminder-card ' + (state.noteReminderDue ? 'is-due' : '')}><span className="reminder-date"><Icon name="Calendar" size={18} /><strong>{new Date(state.noteReminderAt * 1000).toLocaleString([], { dateStyle: 'full', timeStyle: 'short' })}</strong></span><p>{note.trim() || 'Reminder for your notes'}</p><div><button type="button" onClick={() => setShowReminderCalendar(true)}>Change time</button><button type="button" onClick={() => send('noteReminderClear')}>Clear reminder</button></div></div> : <div className="reminder-empty"><Icon name="Calendar" size={25} /><strong>No reminder yet</strong><span>Choose a time for this note.</span><button className="utility-primary" type="button" onClick={() => setShowReminderCalendar(true)}>Schedule reminder</button></div>}
          {state.noteReminderAt && <button className="reminders-edit-note" type="button" onClick={() => { setShowReminderCalendar(false); setNoteView('notes'); }}><Icon name="Edit" size={15} />Edit the linked note</button>}
        </div>}
      </section>}
      {displayPage === 'Weather' && <section className="wx-page" aria-busy={!!state.weatherLoading}>
        <div className="wx-heading"><div className="wx-heading-place"><Icon name="Location" size={16} /><strong>{forecast?.location || state.weatherCity || 'Weather'}</strong>{forecast?.country && <span>{forecast.country}</span>}</div>{state.weatherCity && <button type="button" className="wx-change" onClick={() => setWeatherEditing(value => !value)} aria-label="Change weather location" aria-expanded={weatherEditing}><Icon name="Location" size={15} /><span>Change location</span></button>}</div>
        {(!state.weatherCity || weatherEditing) && <form className="wx-search" onSubmit={event => { event.preventDefault(); if (city.trim()) { send('weatherCity', { value: city.trim() }); setWeatherEditing(false); } }}><Icon name="Location" size={17} /><input autoFocus aria-label="City for weather" value={city} onChange={event => setCity(event.target.value)} placeholder="City or town" /><button type="submit" disabled={!city.trim()}>Use location</button></form>}
        {state.weatherError && current && <p className="wx-error" role="status">{state.weatherError} Showing the last forecast.</p>}
        {current ? <div className={'wx-layout ' + (weatherDataTransition ? 'is-updating' : '')}>
          <div className={'wx-current ' + (current.is_day === 0 ? 'night' : 'day')}>
            <div className="wx-current-main"><div className="wx-current-reading">
              <div className="wx-temperature"><strong>{Math.round(current.temperature_2m)}°</strong></div>
              <div className="wx-condition"><strong>{weatherName(current.weather_code)}</strong><span>High {Math.round(daily?.temperature_2m_max?.[0] ?? 0)}° · Low {Math.round(daily?.temperature_2m_min?.[0] ?? 0)}°</span></div>
            </div><div className="wx-symbol" aria-hidden="true">{weatherGlyph(current.weather_code, 76, current.is_day !== 0)}</div></div>
            <div className="wx-metrics"><div><small>Wind</small><strong>{Math.round(current.wind_speed_10m)} <em>{forecast.windUnit}</em></strong></div><div><small>Humidity</small><strong>{Math.round(current.relative_humidity_2m)}%</strong></div><div><small>Rain</small><strong>{daily?.precipitation_sum?.[0] ?? 0} <em>{forecast.rainUnit}</em></strong></div></div>
            <div className="wx-sun"><span>Sunrise {localTime(daily?.sunrise?.[0]) || '—'}</span><span>Sunset {localTime(daily?.sunset?.[0]) || '—'}</span></div>
          </div>
          <div className="wx-detail">
            <div className="wx-daily"><span className="wx-label">7-day forecast</span><div className="wx-days">{daily?.time?.slice(0, 7).map((date, index) => { const low = Math.round(daily.temperature_2m_min?.[index] ?? 0); const high = Math.round(daily.temperature_2m_max?.[index] ?? 0); const span = Math.max(1, weekHigh - weekLow); return <div className="wx-day" key={date}><span>{index === 0 ? 'Today' : localDay(date)}</span>{weatherGlyph(daily.weather_code?.[index], 18)}<small>{low}°</small><div className="wx-temp-track"><i style={{ left: Math.max(0, (low - weekLow) / span * 100) + '%', right: Math.max(0, (weekHigh - high) / span * 100) + '%' }} /></div><strong>{high}°</strong><small className="wx-day-rain">{daily.precipitation_sum?.[index] ? daily.precipitation_sum[index] + ' ' + forecast.rainUnit : ''}</small></div>; })}</div></div>
          </div>
        </div> : <div className="wx-empty">{weatherGlyph(null, 42)}<strong>{state.weatherLoading ? 'Finding your forecast' : state.weatherCity ? 'Forecast unavailable' : 'Choose a location'}</strong><span>{state.weatherCity ? state.weatherError || 'The forecast will appear here.' : 'Set a city to see local conditions.'}</span></div>}
        {current && <div className="wx-footer"><span role="status" aria-live="polite">{state.weatherLoading ? 'Updating forecast…' : state.weatherUpdatedAt ? 'Updated ' + new Date(state.weatherUpdatedAt * 1000).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' }) : 'Forecast'}</span><span>Open-Meteo</span></div>}
      </section>}
      {displayPage === 'Controls' && <section className="controls-page">
        <div className="controls-heading"><span>Quick controls</span>{state.radioBusy && <small>Checking radios…</small>}</div>
        {privacyLines.length > 0 && <div className="privacy-activity" aria-label="Active camera and microphone use">{privacyLines.map(line => <span className={line.startsWith('Camera') ? 'is-camera' : 'is-mic'} key={line}><i />{line}</span>)}</div>}
        <div className="control-grid">
          <button type="button" className={'control-tile ' + (state.radios?.wifi ? 'control-active' : '')} disabled={state.radioBusy} aria-pressed={state.radios?.wifi == null ? undefined : !!state.radios.wifi} onClick={() => send(state.radios?.wifi == null ? 'radioSettings' : 'radioToggle', { radio: 'wifi' })} title={state.radios?.wifi == null ? 'Open Wi-Fi settings' : 'Toggle Wi-Fi'}><span className="control-icon"><Icon name="Wifi" size={20} /></span><strong>Wi-Fi</strong><small>{state.radios?.wifi == null ? 'Open settings' : state.radios.wifi ? 'On' : 'Off'}</small></button>
          <button type="button" className={'control-tile ' + (state.radios?.bluetooth ? 'control-active' : '')} disabled={state.radioBusy} aria-pressed={state.radios?.bluetooth == null ? undefined : !!state.radios.bluetooth} onClick={() => send(state.radios?.bluetooth == null ? 'radioSettings' : 'radioToggle', { radio: 'bluetooth' })} title={state.radios?.bluetooth == null ? 'Open Bluetooth settings' : 'Toggle Bluetooth'}><span className="control-icon"><Icon name="Bluetooth" size={20} /></span><strong>Bluetooth</strong><small>{state.radios?.bluetooth == null ? 'Open settings' : state.radios.bluetooth ? 'On' : 'Off'}</small></button>
          <button type="button" className={'control-tile ' + (state.dark ? 'control-active' : '')} aria-pressed={!!state.dark} onClick={() => send('themeToggle')}><span className="control-icon"><Icon name="Moon" size={20} /></span><strong>Dark theme</strong><small>{state.dark ? 'On' : 'Off'}</small></button>
          <div className={'control-tile battery-tile ' + (state.batteryPercent != null && state.batteryPercent <= 20 ? 'is-low ' : '') + (state.batteryCharging || state.batteryPlugged ? 'is-connected' : '')} aria-label={state.batteryPercent == null ? 'Battery status unavailable' : `Battery ${state.batteryPercent} percent${state.batteryCharging ? ', charging' : state.batteryPlugged ? ', plugged in' : ''}`}>
            <span className="control-battery-symbol"><BatteryGlyph percent={state.batteryPercent} charging={state.batteryCharging} /></span>
            <strong>Battery</strong><small>{state.batteryPercent == null ? 'Not reported' : `${state.batteryPercent}%${state.batteryCharging ? ' · Charging' : state.batteryPlugged ? ' · Plugged in' : ''}`}</small>
          </div>
        </div>
        {state.radios?.message && <p className="control-message" role="status">{state.radios.message} <button onClick={() => send('radioSettings', { radio: /wi-fi/i.test(state.radios.message) ? 'wifi' : 'bluetooth' })}>Windows settings</button></p>}
        <div className="power-section"><span className="wx-label">Power</span><div className="power-actions">{[['lock', 'Lock', 'Lock'], ['sleep', 'Sleep', 'Moon'], ['restart', 'Restart', 'Refresh'], ['shutdown', 'Shut down', 'Power']].map(([action, label, icon]) => {
          const needsConfirm = action === 'restart' || action === 'shutdown';
          const armed = powerArmed === action;
          return <button key={action} className={armed ? 'power-confirm' : ''} aria-label={armed ? `Confirm ${label.toLowerCase()}` : label} aria-pressed={needsConfirm ? armed : undefined} title={armed ? 'Click again to confirm' : label} onClick={() => {
            if (!needsConfirm) { setPowerArmed(null); send('powerAction', { action }); return; }
            if (armed) { send('powerAction', { action }); setPowerArmed(null); }
            else setPowerArmed(action);
          }}><Icon name={icon} size={17} /><span>{label}</span></button>;
        })}</div></div>
        {state.powerNotice && <p className="control-message" role="status">{state.powerNotice}{state.powerNotice.includes('scheduled') && <button onClick={() => send('powerCancel')}>Cancel shutdown</button>}</p>}
      </section>}
      </>}
    </PageBoundary></div></main>
    </>}
    {dictateOpen && dictatePortalTarget && createPortal(<DictatePanel captureSensitivity={readAgentPreferences().transcriptionSensitivity} pauseMs={readAgentPreferences().speechPauseMs} noiseSuppression={readAgentPreferences().speechNoiseSuppression} minimal={!!state.wakeWord?.surface && !holdCapture} holdCapture={holdCapture} autoSubmit={agentDictateAutoSubmit && !holdCapture} compact={dictateMode === 'agent'} speechSettings={state.speechSettings} onClose={() => { if (dictateMode === 'agent' && agentDictateAutoSubmit) send('wakeWordResume', { phrase: state.wakeWord?.phrase || 'Hey Island' }); setAgentDictateAutoSubmit(false); setDictateOpen(false); }} onInsert={dictateMode === 'agent' ? value => { setAgentDictatedText(value); setDictateOpen(false); } : undefined} transcriptExpanded={dictateTranscriptExpanded} onTranscriptExpandedChange={setDictateTranscriptExpanded} />, dictatePortalTarget)}
  </div>;
}

createRoot(document.getElementById('root')).render(<App />);
