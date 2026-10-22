import React, { useEffect, useId, useMemo, useRef, useState } from 'react';

// A selection replaces the options surface; it never grows a menu outside it.
export function AgentOptions({ options, values, onChange, Icon, selectedId, onSelectId }) {
  const [query, setQuery] = useState('');
  const [limit, setLimit] = useState(40);
  const [cursor, setCursor] = useState(0);
  const listId = useId();
  const list = useRef(null);
  const triggers = useRef(new Map());
  const option = options.find(item => item.id === selectedId);
  const choices = useMemo(() => (option?.options || []).flatMap(group => Array.isArray(group.options)
    ? group.options.map(item => ({ ...item, group: group.name || group.id || '' })) : [group]), [option]);
  const matches = useMemo(() => {
    const terms = query.trim().toLocaleLowerCase().split(/\s+/).filter(Boolean);
    return choices.filter(item => terms.every(term => `${item.name || ''} ${item.value} ${item.group || ''}`.toLocaleLowerCase().includes(term)));
  }, [choices, query]);
  const visible = matches.slice(0, limit);
  const close = () => {
    const previous = selectedId;
    onSelectId('');
    requestAnimationFrame(() => triggers.current.get(previous)?.focus());
  };
  useEffect(() => { setQuery(''); setLimit(40); setCursor(0); }, [selectedId]);
  useEffect(() => { setLimit(40); setCursor(0); if (list.current) list.current.scrollTop = 0; }, [query]);
  useEffect(() => {
    const scroller = list.current;
    const choice = scroller?.querySelector(`[data-choice-index="${cursor}"]`);
    if (!choice) return;
    const bounds = scroller.getBoundingClientRect();
    const item = choice.getBoundingClientRect();
    if (item.top < bounds.top) scroller.scrollTop -= bounds.top - item.top;
    else if (item.bottom > bounds.bottom) scroller.scrollTop += item.bottom - bounds.bottom;
  }, [cursor]);
  const choose = item => { if (!item || !option) return; onChange(option.id, item.value); close(); };

  if (option?.type === 'select') return <section className="agent-option-browser" aria-label={`Choose ${option.name || option.id}`} onKeyDown={event => {
    if (event.nativeEvent.isComposing) return;
    if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); close(); }
  }}>
    <header className="agent-option-browser-head"><button type="button" className="agent-browser-back" onClick={close} aria-label="Back to task options">Options</button><strong>{option.name || option.id}</strong><small>{matches.length} {matches.length === 1 ? 'choice' : 'choices'}</small></header>
    <label className="agent-search"><Icon name="Search" size={16} /><input autoFocus value={query} role="combobox" aria-expanded="true" aria-controls={listId} aria-autocomplete="list" aria-activedescendant={visible[cursor] ? `${listId}-${cursor}` : undefined} aria-label={`Search ${option.name || option.id}`} placeholder={/model/i.test(option.id + option.name) ? 'Search by name or provider' : 'Search choices'} onChange={event => setQuery(event.target.value)} onKeyDown={event => {
      if (event.nativeEvent.isComposing) return;
      if (event.key === 'ArrowDown' || event.key === 'ArrowUp') { event.preventDefault(); setCursor(index => Math.max(0, Math.min(visible.length - 1, index + (event.key === 'ArrowDown' ? 1 : -1)))); }
      if (event.key === 'Enter') { event.preventDefault(); choose(visible[cursor]); }
    }} /></label>
    <div id={listId} className="agent-option-browser-list" ref={list} role="listbox" aria-label={option.name || option.id}>
      {visible.map((item, index) => <button type="button" key={item.value} id={`${listId}-${index}`} role="option" aria-selected={String(values[option.id] ?? option.currentValue) === item.value} data-choice-index={index} className={cursor === index ? 'is-cursor' : ''} onClick={() => choose(item)} onKeyDown={event => {
        if (event.key === 'ArrowDown' || event.key === 'ArrowUp') { event.preventDefault(); const next = Math.max(0, Math.min(visible.length - 1, index + (event.key === 'ArrowDown' ? 1 : -1))); setCursor(next); list.current?.querySelector(`[data-choice-index="${next}"]`)?.focus(); }
      }}><span className="agent-option-symbol"><Icon name="Agents" size={16} /></span><span><strong>{item.name || item.value}</strong>{(item.group || item.description) && <small>{item.group || item.description}</small>}</span>{String(values[option.id] ?? option.currentValue) === item.value && <Icon name="Check" size={16} />}</button>)}
      {!matches.length && <div className="agent-list-empty">No matching choices</div>}
      {matches.length > limit && <button type="button" className="agent-browser-more" onClick={() => setLimit(value => value + 40)}>Show more · {matches.length - limit} remaining</button>}
    </div>
  </section>;

  return <div className="agent-option-overview">{options.map(item => {
    const current = values[item.id] ?? item.currentValue;
    if (item.type === 'boolean') return <div key={item.id} className="settings-line"><span>{item.name || item.id}</span><button type="button" role="switch" aria-label={item.name || item.id} aria-checked={String(current) === 'true'} className={'settings-switch ' + (String(current) === 'true' ? 'on' : '')} onClick={() => onChange(item.id, String(String(current) !== 'true'))}><i /></button></div>;
    const items = (item.options || []).flatMap(group => group.options || [group]);
    const chosen = items.find(choice => choice.value === current);
    return <button type="button" className="agent-option-row" key={item.id} ref={node => { if (node) triggers.current.set(item.id, node); else triggers.current.delete(item.id); }} onClick={() => onSelectId(item.id)}><span className="agent-option-symbol"><Icon name={/model/i.test(item.id + item.name) ? 'Agents' : 'Settings'} size={16} /></span><span><small>{item.name || item.id}</small><strong>{chosen?.name || current || 'Choose'}</strong></span></button>;
  })}</div>;
}
