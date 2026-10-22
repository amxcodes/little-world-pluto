import React, { useRef, useState } from 'react';

// A single open editor keeps dense settings within the available surface.
export function SettingsDisclosure({ id, title, icon, summary, active, onOpen, children, Icon }) {
  const open = active === id;
  return <section className={'settings-disclosure ' + (open ? 'is-open' : '')}>
    <button type="button" className="settings-disclosure-trigger" aria-expanded={open} aria-controls={`settings-editor-${id}`} onClick={() => onOpen(open ? '' : id)}>
      <span className="settings-disclosure-icon"><Icon name={icon} size={18} /></span>
      <strong>{title}</strong><span className="settings-disclosure-summary">{summary}</span>
      <Icon name={open ? 'Up' : 'Down'} size={14} />
    </button>
    {open && <div id={`settings-editor-${id}`} className="settings-disclosure-body">{children}</div>}
  </section>;
}

export function HomeActionSettings({ actions, onChange, catalog, Icon }) {
  const [slot, setSlot] = useState(0);
  const [query, setQuery] = useState('');
  const choose = id => {
    const next = [...actions];
    const previous = next[slot];
    const source = id ? next.indexOf(id) : -1;
    next[slot] = id;
    // Selecting an existing action swaps positions instead of silently deleting it.
    if (source >= 0 && source !== slot) next[source] = previous;
    onChange(next);
  };
  return <div className="settings-action-editor">
    <section className="settings-action-preview">
      <div className="settings-editor-caption"><strong>Home layout</strong><span>Choose a slot</span></div>
      <div className="settings-slot-grid">
        {actions.map((id, index) => {
          const action = catalog.find(item => item.id === id);
          return <button type="button" key={index} aria-pressed={slot === index} className={slot === index ? 'selected' : ''} aria-label={`Slot ${index + 1}: ${action?.label || 'Empty'}`} onClick={() => setSlot(index)}>
            <Icon name={action?.icon || 'Add'} size={18} /><span>{action?.label || 'Empty'}</span><small>{index + 1}</small>
          </button>;
        })}
      </div>
      <p className="settings-note">Assigned actions swap places. Empty slots keep their space.</p>
    </section>
    <section className="settings-action-library">
      <div className="settings-editor-caption"><strong>Slot {slot + 1}</strong><button type="button" disabled={!actions[slot]} onClick={() => choose(null)}>Clear</button></div>
      <label className="settings-library-search"><Icon name="Search" size={16} /><input aria-label="Find a Home action" placeholder="Find an action" value={query} onChange={event => setQuery(event.target.value)} /></label>
      <div className="settings-action-catalog">
        {catalog.filter(item => `${item.label} ${item.detail}`.toLowerCase().includes(query.toLowerCase())).map(item => <button type="button" key={item.id} aria-pressed={actions[slot] === item.id} className={actions[slot] === item.id ? 'selected' : ''} onClick={() => choose(item.id)}>
          <Icon name={item.icon} size={16} /><span>{item.label}</span>{actions[slot] === item.id ? <Icon name="Check" size={14} /> : actions.includes(item.id) ? <small>Swap</small> : null}
        </button>)}
        {!catalog.some(item => `${item.label} ${item.detail}`.toLowerCase().includes(query.toLowerCase())) && <p className="settings-note">No matching actions.</p>}
      </div>
    </section>
  </div>;
}

export function PrioritySettings({ order, onChange, modules, lanes, Icon }) {
  const [lane, setLane] = useState(lanes[0].id);
  const [dragging, setDragging] = useState(null);
  const gesture = useRef(null);
  const ids = order.filter(id => modules.find(item => item.id === id)?.lane === lane);
  const move = (id, target) => {
    if (id === target || !ids.includes(id) || !ids.includes(target)) return;
    const reordered = [...ids];
    reordered.splice(reordered.indexOf(id), 1);
    reordered.splice(ids.indexOf(target), 0, id);
    let index = 0;
    onChange(order.map(item => ids.includes(item) ? reordered[index++] : item));
  };
  const end = event => {
    const drag = gesture.current;
    gesture.current = null;
    setDragging(null);
    if (!drag) return;
    const target = document.elementFromPoint(event.clientX, event.clientY)?.closest('[data-priority-id]')?.dataset.priorityId;
    if (target) move(drag.id, target);
  };
  return <div className="settings-priority-editor">
    <div className="settings-protected-strip"><Icon name="Lock" size={16} /><strong>Always available</strong><span>Recording · Screen tools · Approvals</span></div>
    <div className="settings-lane-tabs" role="group" aria-label="Priority group">{lanes.map(item => <button key={item.id} type="button" aria-pressed={lane === item.id} className={lane === item.id ? 'selected' : ''} onClick={() => setLane(item.id)}>{item.id === 'attention' ? 'Attention' : 'Background'}</button>)}</div>
    <div className="settings-editor-caption"><span>First shown → last shown</span><span>Drag to reorder</span></div>
    <ol className="settings-priority-items">
      {ids.map((id, index) => {
        const item = modules.find(module => module.id === id);
        return <li key={id} data-priority-id={id} className={dragging === id ? 'is-dragging' : ''}>
          <button type="button" className="settings-drag-handle" aria-label={`Move ${item.label}. Use arrow keys to reorder.`}
            onKeyDown={event => { if (event.key === 'ArrowUp' || event.key === 'ArrowDown') { event.preventDefault(); move(id, ids[index + (event.key === 'ArrowUp' ? -1 : 1)]); } }}
            onPointerDown={event => { if (event.button !== 0) return; gesture.current = { id }; setDragging(id); event.currentTarget.setPointerCapture(event.pointerId); }}
            onPointerUp={end} onPointerCancel={() => { gesture.current = null; setDragging(null); }}><span aria-hidden="true">⠿</span></button>
          <span className="settings-priority-number">{index + 1}</span><Icon name={item.icon} size={18} />
          <div><strong>{item.label}</strong><small>{item.detail}</small></div>
        </li>;
      })}
    </ol>
    <p className="settings-note">Reminders return until noticed. Brief updates expire; privacy indicators stay visible.</p>
  </div>;
}
