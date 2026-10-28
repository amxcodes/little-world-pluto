import React, { useEffect, useId, useMemo, useRef, useState } from 'react';
import { SplitRange } from './split-range.jsx';

const size = bytes => bytes >= 1e6 ? `${(bytes / 1e6).toFixed(1)} MB` : `${Math.ceil(bytes / 1000)} KB`;
const formatNames = { jpg: 'JPEG', png: 'PNG', webp: 'WebP', bmp: 'Bitmap', tiff: 'TIFF', gif: 'GIF', mp4: 'MP4', mov: 'MOV', mkv: 'MKV', webm: 'WebM', avi: 'AVI', wmv: 'WMV', mp3: 'MP3', m4a: 'M4A', aac: 'AAC', wav: 'WAV', flac: 'FLAC', ogg: 'OGG', opus: 'Opus', aiff: 'AIFF', wma: 'WMA', srt: 'SRT', vtt: 'WebVTT', txt: 'Plain text' };
const formatDetails = { jpg: 'Compact photos · no transparency', png: 'Lossless · preserves transparency', webp: 'Lossless · modern web images', bmp: 'Uncompressed bitmap', tiff: 'High-fidelity image', gif: 'Still image export', mp4: 'H.264 video · AAC audio', mov: 'H.264 video · AAC audio', mkv: 'H.264 video · AAC audio', webm: 'VP9 video · Opus audio', avi: 'MPEG-4 video · MP3 audio', wmv: 'Windows Media video · audio', mp3: 'Widely compatible audio', m4a: 'AAC audio · M4A container', aac: 'AAC audio · broad device support', wav: 'Uncompressed audio', flac: 'Lossless audio', ogg: 'Vorbis audio', opus: 'Opus audio', aiff: 'Uncompressed AIFF audio', wma: 'Windows Media audio', srt: 'SubRip subtitle text', vtt: 'Web Video Text Tracks', txt: 'Subtitle dialogue only' };
const labels = { ready: 'Ready', inspecting: 'Inspecting', queued: 'Queued', running: 'Converting', complete: 'Saved', failed: 'Needs attention', unsupported: 'Unavailable', cancelled: 'Cancelled' };
const defaultsKey = 'island.conversion.defaults.v1';
export const readConversionDefaults = () => {
  try {
    const saved = JSON.parse(localStorage.getItem(defaultsKey) || '{}');
    return { format: ['png', 'jpg', 'webp', 'bmp', 'tiff', 'gif'].includes(saved.format) ? saved.format : 'png',
      quality: Number.isInteger(saved.quality) && saved.quality >= 35 && saved.quality <= 100 ? saved.quality : 88,
      limit: Number.isFinite(saved.limit) && saved.limit >= 0.01 && saved.limit <= (saved.unit === 'KB' || !saved.unit ? 100000000 : 100000) ? saved.limit : 2,
      // Older builds stored the same value in KB without a unit. Preserve it.
      unit: ['KB', 'MB'].includes(saved.unit) ? saved.unit : Number.isFinite(saved.limit) ? 'KB' : 'MB' };
  } catch { return { format: 'png', quality: 88, limit: 2, unit: 'MB' }; }
};
export const saveConversionDefaults = value => localStorage.setItem(defaultsKey, JSON.stringify(value));

function FileFigure({ format, kind, output, stack, progress, compact = false }) {
  const id = useId().replaceAll(':', '');
  return <svg className={`convert-file-figure ${compact ? 'is-compact' : ''} ${output ? 'is-output' : ''}`} viewBox="0 0 108 126" aria-hidden="true">
    <defs>
      <linearGradient id={id} x1="0" y1="0" x2="1" y2="1">
        <stop offset="0" stopColor={output ? 'var(--accent-a)' : 'var(--card-hover)'} />
        <stop offset="1" stopColor={output ? 'var(--accent-b)' : 'var(--card)'} />
      </linearGradient>
    </defs>
    {stack && <><path d="M25 14h49l19 19v79H25z" fill="var(--card)" stroke="var(--line)" transform="rotate(9 55 65)" /><path d="M19 12h49l19 19v79H19z" fill="var(--card-hover)" stroke="var(--line)" transform="rotate(-6 55 65)" /></>}
    <path d="M17 12h49l24 24v76H17z" fill="var(--bg)" opacity=".4" transform="translate(3 4)" />
    <path d="M17 10h49l24 24v76H17z" fill={`url(#${id})`} stroke={output ? 'var(--accent)' : 'var(--line)'} />
    {output && progress != null && <path className="convert-figure-progress" d="M17 110V10h49l24 24v76z" fill="none" stroke="var(--accent-ink)" strokeWidth="2.5" pathLength="1" strokeDasharray="1" strokeDashoffset={1 - progress} />}
    <path d="M66 10v24h24" fill="none" stroke={output ? 'var(--accent-ink)' : 'var(--muted)'} opacity=".35" />
    <g stroke={output ? 'var(--accent-ink)' : 'var(--muted)'} fill="none" strokeWidth="1.8" strokeLinejoin="round" opacity=".85">
      {kind === 'image' ? <><rect x="34" y="48" width="38" height="27" rx="3" /><path d="m35 70 11-10 9 7 7-7 9 9" /><circle cx="62" cy="56" r="3" /></>
        : kind === 'video' ? <><rect x="34" y="47" width="38" height="29" rx="3" /><path d="m49 54 12 8-12 8zM39 48v28M67 48v28" /></>
        : kind === 'audio' ? <path d="M35 58v10m7-17v24m7-31v38m8-29v20m7-25v30m7-20v10" />
        : <path d="M34 51h38M34 60h38M34 69h24" />}
    </g>
    <text x="53" y="96" textAnchor="middle" fill={output ? 'var(--accent-ink)' : 'var(--text)'} fontFamily="inherit" fontSize="10" fontWeight="650">{(format || 'FILE').slice(0, 7).toUpperCase()}</text>
  </svg>;
}

export function ConversionWorkspace({ state, send, onSettings, Icon }) {
  const data = state.conversion || {};
  const items = data.items || [];
  const [kind, setKind] = useState('image');
  const [format, setFormat] = useState(() => readConversionDefaults().format);
  const [mode, setMode] = useState('convert');
  const [browser, setBrowser] = useState(false);
  const [width, setWidth] = useState('');
  const [height, setHeight] = useState('');
  const [limit, setLimit] = useState(() => readConversionDefaults().limit);
  const [unit, setUnit] = useState(() => readConversionDefaults().unit);
  const [smaller, setSmaller] = useState(false);
  const [quality, setQuality] = useState(() => readConversionDefaults().quality);
  const [excluded, setExcluded] = useState([]);
  const dropArrivalId = Number(state.dropHandoffId) || 0;
  const seenItems = useRef(new Map());
  const itemIds = JSON.stringify(items.map(item => item.id));
  const arrivalOrder = useMemo(() => {
    let next = 0;
    return new Map(items.map(item => [item.id, seenItems.current.get(item.id) ?? next++]));
  }, [itemIds]);
  useEffect(() => { seenItems.current = arrivalOrder; }, [arrivalOrder]);
  const seenReadyItems = useRef(new Set());
  const kinds = useMemo(() => [...new Set(items.filter(i => ['image', 'video', 'audio', 'subtitle'].includes(i.kind)).map(i => i.kind))], [items]);
  useEffect(() => { if (kinds.length && !kinds.includes(kind)) setKind(kinds[0]); }, [kinds.join('|'), kind]);
  useEffect(() => {
    const currentIds = new Set(items.map(item => item.id));
    for (const id of seenReadyItems.current) if (!currentIds.has(id)) seenReadyItems.current.delete(id);
    const newlyInspected = items.filter(item => item.status !== 'inspecting'
      && ['image', 'video', 'audio', 'subtitle'].includes(item.kind)
      && !seenReadyItems.current.has(item.id));
    for (const item of newlyInspected) seenReadyItems.current.add(item.id);
    const latest = newlyInspected.at(-1);
    if (latest) setKind(latest.kind);
  }, [items]);
  useEffect(() => { if ((kind === 'audio' || kind === 'subtitle') && mode === 'resize' || kind === 'subtitle' && mode === 'fit') setMode('convert'); }, [kind, mode]);
  const group = items.filter(i => i.kind === kind && i.formats.length && i.status !== 'complete');
  const selected = group.filter(i => !excluded.includes(i.id));
  const formats = selected.length ? selected[0].formats.filter(f => selected.every(i => i.formats.includes(f))) : [];
  useEffect(() => { if (formats.length && !formats.includes(format)) setFormat(formats[0]); }, [formats.join('|'), format]);
  const active = items.find(i => i.status === 'running') || selected[0] || items.at(-1);
  const complete = items.filter(i => i.status === 'complete').length;
  const failed = items.filter(i => i.status === 'failed').length;
  const eligible = selected.filter(i => i.formats.includes(format));
  const busy = !!data.busy;
  const valid = mode !== 'fit' || (Number.isFinite(+limit) && +limit >= (unit === 'MB' ? 0.01 : 1) && +limit * (unit === 'MB' ? 1e6 : 1e3) <= 1e11);
  const start = () => send('conversionStart', { ids: eligible.map(i => i.id), profile: {
    format, quality, width: mode === 'resize' ? Math.max(0, +width || 0) : 0,
    height: mode === 'resize' ? Math.max(0, +height || 0) : 0,
    maxBytes: mode === 'fit' ? Math.round(+limit * (unit === 'MB' ? 1e6 : 1e3)) : 0, allowSmaller: mode === 'fit' && smaller,
  } });
  return <section className={`conversion-workspace ${dropArrivalId ? 'is-drop-arriving' : ''}`} data-layout-height="390" aria-label="File conversion">
    <header className="convert-header">
      <div className="convert-title"><span className="convert-title-icon"><Icon name="File" size={18} /></span><strong>Files</strong></div>
      <span className="convert-count" role="status">{busy ? data.inspecting ? 'Inspecting' : `${complete} / ${items.length} saved` : `${items.length} ${items.length === 1 ? 'file' : 'files'}`}</span>
      <button className="convert-add" disabled={busy || items.length >= 32} onClick={() => send('conversionPick')}><Icon name="Add" size={15} /> Add files</button>
    </header>
    <div className="convert-body">
      <div className="convert-source">
        <div className={`convert-transformation ${busy && !data.inspecting ? 'is-working' : ''}`}>
          <div key={dropArrivalId}><FileFigure format={active?.name?.split('.').at(-1)} kind={active?.kind} stack={items.length > 1} /><span>Original</span></div>
          <span className={`convert-connector ${busy ? 'is-active' : ''}`} aria-hidden="true">{[0, 1, 2, 3].map(index => <i key={index} style={{ '--block-index': index }} />)}</span>
          <div><FileFigure format={format} kind={active?.kind} output progress={active?.status === 'running' ? active.progress : null} /><span>{formatNames[format] || 'Output'}</span></div>
        </div>
        <div className="convert-queue" aria-label="Batch files">
          {!items.length && <div className="convert-empty"><strong>Your files, ready for a new format.</strong><span>Drop files on Convert, or add them here.</span><button onClick={() => send('conversionPick')}>Choose files</button></div>}
    {items.map(item => <div className={`convert-queue-row is-${item.status}${arrivalOrder.has(item.id) ? ' is-arriving' : ''}`} key={item.id} style={{ '--arrival-delay': `${Math.min(arrivalOrder.get(item.id) || 0, 8) * 34}ms` }}>
            <input type="checkbox" aria-label={`Include ${item.name}`} disabled={busy || !item.formats.length || item.status === 'complete'}
              checked={item.kind === kind && !excluded.includes(item.id) && !!item.formats.length && item.status !== 'complete'}
              onChange={() => { if (kind !== item.kind) { setKind(item.kind); setExcluded(ids => ids.filter(id => id !== item.id)); }
                else setExcluded(ids => ids.includes(item.id) ? ids.filter(id => id !== item.id) : [...ids, item.id]); }} />
            <FileFigure format={item.name?.split('.').at(-1)} kind={item.kind} compact />
            <div className="convert-row-content">
              <strong title={item.name}>{item.name}</strong>
              <div className="convert-row-badges">
                {item.bytes > 0 && <span className="convert-size-badge">{item.status === 'complete' ? `${size(item.bytes)} → ${size(item.outputBytes)}` : size(item.bytes)}</span>}
                {item.width > 0 && item.status !== 'complete' && <span className="convert-detail-badge">{item.width} × {item.height}</span>}
                <span className={`convert-state-badge is-${item.status}`}>{labels[item.status]}</span>
              </div>
              {item.message && <small title={item.message}>{item.message}</small>}
              {item.status === 'running' && <div className="convert-progress" role="progressbar" aria-label={`Converting ${item.name}`} aria-valuemin={0} aria-valuemax={100} aria-valuenow={item.progress == null ? undefined : Math.round(item.progress * 100)}>
                {item.progress != null ? <i style={{ transform: `scaleX(${item.progress})` }} /> : <span>Processing</span>}
              </div>}
            </div>
            {item.status === 'complete' ? <div className="convert-result-actions"><button title="Open output" onClick={() => send('conversionOpen', { id: item.id })}>Open</button><button title="Add output to Shelf" onClick={() => send('conversionShelf', { id: item.id })}>Shelf</button></div>
              : <button className="convert-remove" disabled={busy} aria-label={`Remove ${item.name}`} onClick={() => send('conversionRemove', { id: item.id })}>×</button>}
          </div>)}
        </div>
      </div>
      <div className="convert-inspector">
        <div className="convert-mode-rail" role="group" aria-label="Conversion action">
          {[["convert", "Refresh", "Format"], ...(["image", "video"].includes(kind) ? [["resize", "Image", "Resize"]] : []), ...(kind !== 'subtitle' ? [["fit", "File", "Fit size"]] : [])].map(([value, icon, label]) =>
            <button key={value} title={label} aria-label={label} aria-pressed={mode === value} disabled={busy} onClick={() => { setMode(value); setBrowser(false); }}><Icon name={icon} size={19} /><span>{label}</span></button>)}
          {kind !== 'subtitle' && <button className="convert-engine-rail" title="FFmpeg settings" aria-label="FFmpeg settings" onClick={onSettings}><Icon name="Settings" size={17} /><span>Engine</span></button>}
        </div>
        <div className="convert-panel">
          {kinds.length > 1 && <div className="convert-kind-picker" role="group" aria-label="File category">{kinds.map(k => <button key={k} disabled={busy} aria-pressed={kind === k} onClick={() => { setKind(k); setBrowser(false); }}><Icon name={k === 'image' ? 'Image' : k === 'video' ? 'Video' : k === 'audio' ? 'Audio' : 'File'} size={14} />{k === 'image' ? 'Images' : k === 'video' ? 'Video' : k === 'audio' ? 'Audio' : 'Subtitles'}</button>)}</div>}
          {browser ? <>
            <div className="convert-panel-heading"><button className="convert-panel-back" onClick={() => setBrowser(false)} aria-label="Back to options"><Icon name="PreviousMonth" size={15} /></button><strong>Output format</strong></div>
            <div className="convert-format-grid">{formats.length ? formats.map(f => <button key={f} aria-pressed={f === format} title={formatDetails[f]} onClick={() => { setFormat(f); setBrowser(false); }}><strong>{formatNames[f] || f.toUpperCase()}</strong><small>{formatDetails[f]}</small></button>) : <div className="convert-format-empty">No compatible output formats are available for these files.</div>}</div>
          </> : <>
            <div className="convert-panel-heading"><strong>{mode === 'fit' ? 'Fit to size' : mode === 'resize' ? 'Resize' : 'Change format'}</strong><small>{mode === 'fit' ? 'Set a maximum output size per file' : mode === 'resize' ? 'Preserve proportions while resizing' : 'Choose a format for the selected files'}</small></div>
            <button className="convert-option" disabled={busy || !formats.length} onClick={() => setBrowser(true)}><span><small>Output</small><strong>{formats.length ? formatNames[format] || format.toUpperCase() : items.some(item => item.kind === kind && item.status !== 'complete') ? 'Unavailable' : 'Add a file'}</strong></span><Icon name="ChevronRight" size={16} /></button>
            {mode === 'resize' && <div className="convert-dimensions">
              <label><span>Width</span><input type="text" inputMode="numeric" pattern="[0-9]*" placeholder="Auto" value={width} disabled={busy} onChange={e => setWidth(e.target.value.replace(/[^0-9]/g, '').slice(0, 5))} /></label>
              <span className="convert-dimension-times" aria-hidden="true">×</span>
              <label><span>Height</span><input type="text" inputMode="numeric" pattern="[0-9]*" placeholder="Auto" value={height} disabled={busy} onChange={e => setHeight(e.target.value.replace(/[^0-9]/g, '').slice(0, 5))} /></label>
            </div>}
            {mode === 'fit' && <div className="convert-size-controls">
              <div className="convert-size-presets">{[[500, 'KB'], [2, 'MB'], [10, 'MB']].map(([n, u]) => <button key={`${n}${u}`} disabled={busy} aria-pressed={+limit === n && unit === u} onClick={() => { setLimit(n); setUnit(u); }}>{n} {u}</button>)}</div>
              <div className="convert-size-input"><label htmlFor="convert-limit-value">Maximum per file</label><div><input id="convert-limit-value" type="text" inputMode="decimal" value={limit} disabled={busy} onChange={e => setLimit(e.target.value.replace(/[^0-9.]/g, '').replace(/(\..*)\./g, '$1').slice(0, 10))} /><button aria-label="Change size unit" disabled={busy} onClick={() => { const current = Number.isFinite(+limit) && +limit > 0 ? +limit : (unit === 'MB' ? 2 : 2000); setLimit(unit === 'MB' ? Math.round(current * 1000) : Math.round(current / 10) / 100); setUnit(unit === 'MB' ? 'KB' : 'MB'); }}>{unit}</button></div></div>
              {['image', 'video'].includes(kind) && <button type="button" role="switch" aria-checked={smaller} className="convert-switch-row" disabled={busy} onClick={() => setSmaller(value => !value)}><span>Allow smaller dimensions</span><i aria-hidden="true"><b /></i></button>}
            </div>}
            {kind === 'image' && format === 'jpg' && <div className="convert-quality"><label htmlFor="convert-quality">JPEG quality <span>{quality}%</span></label><SplitRange id="convert-quality" label="QUALITY" min={35} max={100} value={quality} disabled={busy} onChange={e => setQuality(+e.target.value)} /></div>}
          </>}
        </div>
      </div>
    </div>
    {data.notice && <p className="convert-notice" role="status">{data.notice}</p>}
    <footer className="convert-footer">
      <button className="convert-destination" title={data.destination || 'Save alongside each original'} disabled={busy} onClick={() => send('conversionDestination')}>
        <span className="convert-destination-icon"><Icon name="FolderAdd" size={16} /></span>
        <span className="convert-destination-copy"><small>Save location</small><strong>{data.destination ? data.destination.split(/[\\/]/).filter(Boolean).at(-1) : 'Original folder'}</strong></span>
        <Icon name="ChevronRight" size={15} />
      </button>
      {busy ? <button className="convert-cancel" onClick={() => send('conversionCancel')}>Cancel batch</button> :
        <button className="convert-primary" disabled={!eligible.length || !valid} onClick={start}>{failed && eligible.some(i => i.status === 'failed') ? 'Retry selected' : `Convert${eligible.length > 1 ? ` ${eligible.length} files` : ''}`}</button>}
    </footer>
  </section>;
}
