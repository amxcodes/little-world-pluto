import React, { useEffect, useMemo, useRef, useState } from 'react';
import { AgentPresentationView } from './agent-presentation.jsx';
import { AgentOptions } from './agent-options.jsx';
import { WakeEyes } from './wake-eyes.jsx';

export const defaultAskInstructions = 'Write a short, natural spoken answer that the user can read aloud from a small teleprompter. Lead directly with the answer. Usually use 2–5 short sentences, with one sentence per line and a blank line between ideas. Use familiar words and natural contractions. Avoid em dashes, headings, bullet lists, markdown decoration, emojis, tables, diagrams, and introductory filler. Keep the wording human and conversational, not like an essay. Give more detail only when the question requires it. Do not invent personal experiences or facts about the user. State uncertainty plainly when needed.';

const defaults = {
  defaultProvider: 'opencode', providerSetupComplete: false, activityDetail: 'current', showChildCount: true,
  systemPrompt: 'For this compact workspace, lead with the result and keep routine updates concise. Include fuller detail when it helps complete the task. Be explicit about uncertainty and failures; never claim an action succeeded without confirmation. Follow the user\'s request, project instructions, and the provider\'s safety and approval rules.',
  answerView: 'preview', showComposerDuringRun: true, hybridWhiteboard: false,
  askInstructions: defaultAskInstructions,
  wakeSensitivity: 'balanced',
  transcriptionSensitivity: 'balanced',
  speechPauseMs: 1500, speechNoiseSuppression: true,
  provider: { opencode: { directory: '', config: {} }, codex: { directory: '', config: {} } },
};

// The expanded page unmounts whenever the island compacts. Keep only transient
// composer state in memory so reopening the island does not erase unfinished work.
const workspaceMemory = {
  drafts: { opencode: '', codex: '' },
  files: { opencode: [], codex: [] },
  view: 'task',
  query: '',
  pendingRevision: {},
  submitted: {},
  provider: '',
  lastWakeRevision: 0,
  wakeTarget: null,
  wakeSessions: {},
  wakeTranscript: null,
};

export function readAgentPreferences() {
  try {
    const native = window.islandAgentPreferenceBootstrap;
    const saved = native && typeof native === 'object' && !Array.isArray(native)
      ? native : JSON.parse(localStorage.getItem('island.agent-preferences') || '{}');
    const hasExistingPreferences = Object.keys(saved).length > 0;
    return {
      ...defaults,
      ...saved,
      systemPrompt: typeof saved.systemPrompt === 'string' ? saved.systemPrompt.slice(0, 6000) : defaults.systemPrompt,
      hybridWhiteboard: saved.hybridWhiteboard === true,
      askInstructions: typeof saved.askInstructions === 'string' ? saved.askInstructions.slice(0, 6000) : defaultAskInstructions,
      wakeSensitivity: ['low', 'balanced', 'high'].includes(saved.wakeSensitivity) ? saved.wakeSensitivity : 'balanced',
      transcriptionSensitivity: ['low', 'balanced', 'high'].includes(saved.transcriptionSensitivity) ? saved.transcriptionSensitivity : 'balanced',
      speechPauseMs: [900, 1500, 2200].includes(saved.speechPauseMs) ? saved.speechPauseMs : 1500,
      speechNoiseSuppression: saved.speechNoiseSuppression !== false,
      // Older builds wrote the default false flag alongside unrelated settings.
      // Any existing preferences mean the user has already used the app; only
      // a brand-new profile should see provider onboarding.
      providerSetupComplete: saved.providerSetupComplete === true || hasExistingPreferences,
      provider: {
        ...defaults.provider,
        ...saved.provider,
        opencode: { ...defaults.provider.opencode, ...saved.provider?.opencode },
        codex: { ...defaults.provider.codex, ...saved.provider?.codex },
      },
    };
  } catch { return defaults; }
}

export function saveAgentPreferences(next) {
  try { localStorage.setItem('island.agent-preferences', JSON.stringify(next)); } catch {}
  window.islandAgentPreferenceBootstrap = next;
  try { window.ipc?.postMessage(JSON.stringify({ kind: 'agentPreferencesSave', preferences: next })); } catch {}
  window.dispatchEvent(new CustomEvent('island-agent-preferences', { detail: next }));
}

function useAgentPreferences() {
  const [preferences, setPreferences] = useState(readAgentPreferences);
  useEffect(() => {
    const update = event => setPreferences(event.detail || readAgentPreferences());
    window.addEventListener('island-agent-preferences', update);
    return () => window.removeEventListener('island-agent-preferences', update);
  }, []);
  const change = patch => {
    if (typeof patch.defaultProvider === 'string') workspaceMemory.provider = patch.defaultProvider;
    const next = { ...preferences, ...patch };
    setPreferences(next);
    saveAgentPreferences(next);
  };
  return [preferences, change];
}

const plainVoiceText = text => String(text || '').replace(/```island-ui[^\n]*\n[\s\S]*?(?:```|$)/gi, '').replace(/\*\*/g, '').replace(/`/g, '');
const providerName = id => id === 'codex' ? 'Codex' : 'OpenCode';
const sendAgent = (send, provider, action, data = {}) => send('agentAction', { provider, action, ...data });
const cleanOptions = options => (options || []).filter(option => option?.id && (option.type === 'boolean' || (option.type === 'select' && Array.isArray(option.options))));
const optionChoices = option => option.options.flatMap(item => Array.isArray(item.options) ? item.options : [item]).filter(item => typeof item.value === 'string');
const agentOperationLabels = { start: 'start the task', prompt: 'send the message', load: 'load the session', auth: 'sign in', logout: 'sign out', prepare: 'load task options', configure: 'update task options', fork: 'fork the session', session: 'open the session', question: 'send the answer', close: 'close the session', delete: 'remove the session' };
const agentStepState = status => {
  const value = String(status || '').toLocaleLowerCase();
  if (['in_progress', 'running', 'started'].includes(value)) return 'running';
  if (['completed', 'complete', 'succeeded', 'success'].includes(value)) return 'done';
  if (['failed', 'error'].includes(value)) return 'failed';
  if (['denied', 'rejected'].includes(value)) return 'denied';
  if (['cancelled', 'canceled'].includes(value)) return 'cancelled';
  return 'pending';
};
const agentStepLabel = state => ({ running: 'Running', done: 'Done', failed: 'Failed', denied: 'Denied', cancelled: 'Cancelled', pending: 'Queued' })[state];
const emptyMcpDraft = () => ({ name: '', originalName: '', transport: 'stdio', enabled: true, command: '', argsText: '', env: [], url: '', headers: [] });

function ChoiceControl({ label, value, options, onChange, searchable, compact = false }) {
  const [query, setQuery] = useState('');
  const [expanded, setExpanded] = useState(false);
  const needsSearch = searchable ?? options.length > 8;
  const visibleOptions = needsSearch
    ? options.filter(option => String(option.label ?? option.value).toLocaleLowerCase().includes(query.toLocaleLowerCase()))
    : options;
  const selected = String(value ?? '');
  const selectedOption = options.find(option => String(option.value) === selected);
  return <div className={'agent-choice-control ' + (compact ? 'compact ' : '') + (options.length > 4 ? 'has-many' : '')}>
    {label && <span className="agent-choice-label">{label}</span>}
    {needsSearch ? <>
      <button type="button" className="agent-choice-current" aria-expanded={expanded} onClick={() => { setExpanded(value => !value); setQuery(''); }}><span>{selectedOption?.label || selected || `Choose ${label?.toLocaleLowerCase() || 'option'}`}</span><i aria-hidden="true" /></button>
      {expanded && <div className="agent-choice-picker">
        <input autoFocus className="agent-choice-search" aria-label={`Find ${label || 'option'}`} placeholder={`Search ${label?.toLocaleLowerCase() || 'options'}…`} value={query} onChange={event => setQuery(event.target.value)} />
        <div className="agent-choice-options" role="listbox" aria-label={label || 'Choose an option'}>
          {visibleOptions.map(option => <button type="button" key={String(option.value)} role="option" className="agent-choice-option" aria-selected={selected === String(option.value)} title={option.label || String(option.value)} onClick={() => { onChange(option.value); setExpanded(false); setQuery(''); }}>{option.label || String(option.value)}</button>)}
          {!visibleOptions.length && <small className="agent-choice-empty">No matching options</small>}
        </div>
      </div>}
    </> : <div className="agent-choice-options" role="group" aria-label={label || 'Choose an option'}>
      {visibleOptions.map(option => <button type="button" key={String(option.value)} className="agent-choice-option" aria-pressed={selected === String(option.value)} title={option.label || String(option.value)} onClick={() => onChange(option.value)}>{option.label || String(option.value)}</button>)}
      {!visibleOptions.length && <small className="agent-choice-empty">No matching options</small>}
    </div>}
  </div>;
}

const attachmentIcon = path => {
  const extension = String(path).split('.').at(-1)?.toLocaleLowerCase();
  if (['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp', 'svg'].includes(extension)) return 'Image';
  if (['mp4', 'mov', 'mkv', 'webm', 'avi'].includes(extension)) return 'Video';
  if (['mp3', 'wav', 'flac', 'm4a', 'ogg'].includes(extension)) return 'Audio';
  if (extension === 'pdf') return 'Pdf';
  if (['zip', '7z', 'rar', 'tar', 'gz'].includes(extension)) return 'Archive';
  if (['js', 'jsx', 'ts', 'tsx', 'rs', 'py', 'json', 'toml', 'html', 'css', 'sh'].includes(extension)) return 'FileCode';
  return 'FileText';
};

function AgentLibrarySettings({ Icon, send, section }) {
  const [mcpServers, setMcpServers] = useState([]);
  const [skills, setSkills] = useState([]);
  const [mcpDraft, setMcpDraft] = useState(null);
  const [skillDraft, setSkillDraft] = useState(null);
  const [mcpNotice, setMcpNotice] = useState(null);
  const [skillNotice, setSkillNotice] = useState(null);
  const [armedMcpDelete, setArmedMcpDelete] = useState(false);
  const [armedSkillDelete, setArmedSkillDelete] = useState(false);
  const pendingSkill = useRef(null);
  useEffect(() => {
    const receive = result => {
      if (Array.isArray(result?.mcpServers)) setMcpServers(result.mcpServers);
      if (Array.isArray(result?.skills)) setSkills(result.skills);
      if (result?.action === 'refresh') {
        setMcpNotice(result?.mcpError ? { message: result.mcpError, severity: 'error' } : null);
        setSkillNotice(result?.skillsError ? { message: result.skillsError, severity: 'error' } : null);
      }
      if (result?.action === 'skillRead' && result?.skill?.content && pendingSkill.current?.name === result.skill.name) {
        const content = result.skill.content;
        const body = content.replace(/^---\r?\n[\s\S]*?\r?\n---\r?\n?/, '').trim();
        const name = result.skill.name;
        const description = pendingSkill.current?.description || '';
        pendingSkill.current = null;
        setSkillDraft({ name, originalName: name, description, instructions: body });
      }
      if (result?.action === 'mcpSave' && result.ok) setMcpDraft(null);
      if (result?.action === 'skillSave' && result.ok) setSkillDraft(null);
      if (result?.action === 'mcpDelete' && result.ok) { setMcpDraft(null); setArmedMcpDelete(false); }
      if (result?.action === 'skillDelete' && result.ok) { setSkillDraft(null); setArmedSkillDelete(false); }
      if (['mcpSave', 'mcpDelete'].includes(result?.action)) setMcpNotice({ message: result?.error || (result?.ok ? (result.action === 'mcpDelete' ? 'Server removed' : 'Server saved') : 'Could not save MCP server.'), severity: result?.ok ? 'success' : 'error' });
      if (['skillSave', 'skillDelete'].includes(result?.action)) setSkillNotice({ message: result?.error || (result?.ok ? (result.action === 'skillDelete' ? 'Skill removed' : 'Skill saved') : 'Could not save skill.'), severity: result?.ok ? 'success' : 'error' });
      if (result?.action === 'skillRead' && result?.error) setSkillNotice({ message: result.error, severity: 'error' });
    };
    window.islandAgentLibraryResult = receive;
    send('agentLibrary', { action: 'refresh' });
    return () => { if (window.islandAgentLibraryResult === receive) delete window.islandAgentLibraryResult; };
  }, [send]);

  const saveMcp = () => {
    if (!mcpDraft) return;
    send('agentLibrary', { action: 'mcpSave', server: {
      name: mcpDraft.name, originalName: mcpDraft.originalName, transport: mcpDraft.transport, enabled: mcpDraft.enabled,
      command: mcpDraft.command, args: mcpDraft.argsText.split(/\r?\n/).map(item => item.trim()).filter(Boolean),
      env: mcpDraft.env.map(({ name, value }) => ({ name, value })), url: mcpDraft.url,
      headers: mcpDraft.headers.map(({ name, value }) => ({ name, value })),
    } });
  };
  const pairEditor = (label, key, rows) => <div className="agent-library-pairs">
    <div className="agent-library-pairs-title"><span>{label}</span><button type="button" className="settings-inline-action" onClick={() => setMcpDraft(current => ({ ...current, [key]: [...current[key], { name: '', value: '' }] }))}>Add</button></div>
    {rows.map((row, index) => <div className="agent-library-pair" key={`${key}-${index}`}>
      <input className="agent-library-input" aria-label={`${label} name`} value={row.name} maxLength={128} placeholder={key === 'env' ? 'API_KEY' : 'Authorization'} onChange={event => setMcpDraft(current => ({ ...current, [key]: current[key].map((item, i) => i === index ? { ...item, name: event.target.value } : item) }))} />
      <input className="agent-library-input" aria-label={`${label} value`} type="password" autoComplete="new-password" value={row.value} maxLength={8192} placeholder={row.stored ? 'Saved · leave empty to keep' : 'Value'} onChange={event => setMcpDraft(current => ({ ...current, [key]: current[key].map((item, i) => i === index ? { ...item, value: event.target.value } : item) }))} />
      <button type="button" className="settings-inline-action" aria-label={`Remove ${label}`} onClick={() => setMcpDraft(current => ({ ...current, [key]: current[key].filter((_, i) => i !== index) }))}>×</button>
    </div>)}
  </div>;

  return <>
    <section hidden={section !== 'mcp'} className="settings-card settings-card-wide agent-library-card">
      <div className="settings-card-heading"><Icon name="Cloud" size={17} /><div><strong>MCP tools</strong><small>Shared secure connections for OpenCode and Codex</small></div></div>
      {!mcpDraft && <div className="agent-library-grid">{!mcpServers.length && <small className="agent-library-empty">No MCP servers configured</small>}{mcpServers.map(server => <button type="button" className={'agent-library-tile ' + (mcpDraft?.originalName === server.name ? 'selected' : '')} key={server.name} aria-pressed={mcpDraft?.originalName === server.name} onClick={() => { setArmedMcpDelete(false); setMcpDraft({ ...emptyMcpDraft(), ...server, originalName: server.name, argsText: (server.args || []).join('\n'), env: (server.environment || []).map(item => ({ name: item.name, value: '', stored: item.stored })), headers: (server.headers || []).map(item => ({ name: item.name, value: '', stored: item.stored })) }); }}>
        <span className="agent-library-item-mark"><Icon name={server.transport === 'http' ? 'Cloud' : 'Terminal'} size={16} /></span>
        <span className="agent-library-tile-copy"><strong>{server.name}</strong><small>{!server.enabled ? 'Paused' : server.transport === 'http' ? 'Remote · HTTP' : 'Local · stdio'}</small></span>
      </button>)}<button type="button" className="agent-library-tile agent-library-add" onClick={() => { setArmedMcpDelete(false); setMcpDraft(emptyMcpDraft()); }}><span className="agent-library-item-mark"><Icon name="Add" size={16} /></span><span className="agent-library-tile-copy"><strong>Add server</strong><small>MCP connection</small></span></button></div>}
      {mcpDraft && <>
      <div className="agent-library-editor">
        <div className="agent-library-form-grid">
          <label><span>Name</span><input className="agent-library-input" value={mcpDraft.name} maxLength={64} placeholder="filesystem" disabled={!!mcpDraft.originalName} onChange={event => setMcpDraft({ ...mcpDraft, name: event.target.value.toLowerCase() })} /></label>
          <label className="agent-library-connection"><span>Connection</span><div className="settings-segments" role="group" aria-label="Connection type">{[{ value: 'stdio', label: 'Local · stdio' }, { value: 'http', label: 'Remote · HTTP' }].map(option => <button type="button" key={option.value} className={mcpDraft.transport === option.value ? 'selected' : ''} aria-pressed={mcpDraft.transport === option.value} onClick={() => setMcpDraft(current => ({ ...current, transport: option.value }))}>{option.label}</button>)}</div></label>
        </div>
        {mcpDraft.transport === 'stdio' ? <>
          <label><span>Executable</span><input className="agent-library-input" value={mcpDraft.command} maxLength={2048} placeholder="node.exe or full path" onChange={event => setMcpDraft({ ...mcpDraft, command: event.target.value })} /><small>For script packages, add the script path as the first argument.</small></label>
          <label><span>Arguments · one per line</span><textarea className="agent-library-input agent-library-small-area" value={mcpDraft.argsText} maxLength={16000} placeholder="-y\n@modelcontextprotocol/server-filesystem" onChange={event => setMcpDraft({ ...mcpDraft, argsText: event.target.value })} /></label>
          {pairEditor('Environment', 'env', mcpDraft.env)}
        </> : <>
          <label><span>Endpoint</span><input className="agent-library-input" value={mcpDraft.url} maxLength={4096} placeholder="https://example.com/mcp" onChange={event => setMcpDraft({ ...mcpDraft, url: event.target.value })} /></label>
          {pairEditor('Headers', 'headers', mcpDraft.headers)}
          <small>HTTP servers connect only when the selected ACP agent advertises HTTP transport.</small>
        </>}
        <div className="settings-line"><span>Enabled for new and resumed sessions</span><button type="button" role="switch" aria-label={`Enable ${mcpDraft.name || 'MCP server'}`} aria-checked={mcpDraft.enabled} className={'settings-switch ' + (mcpDraft.enabled ? 'on' : '')} onClick={() => setMcpDraft({ ...mcpDraft, enabled: !mcpDraft.enabled })}><i /></button></div>
      </div>
      <div className="agent-library-actions">{mcpDraft.originalName && <button type="button" className={'settings-inline-action agent-library-remove ' + (armedMcpDelete ? 'is-armed' : '')} onClick={() => { if (armedMcpDelete) send('agentLibrary', { action: 'mcpDelete', name: mcpDraft.originalName }); else setArmedMcpDelete(true); }}>{armedMcpDelete ? 'Confirm remove' : 'Remove server'}</button>}<span /> <button type="button" className="settings-inline-action" onClick={() => { setMcpDraft(null); setArmedMcpDelete(false); }}>Cancel</button><button type="button" className="settings-inline-action agent-library-save" onClick={saveMcp}>Save server</button></div>
      </>}
      <p hidden={!!mcpDraft} className="settings-note">Credentials are encrypted with Windows DPAPI.</p>
      {mcpNotice && <AgentFeedback notice={mcpNotice.message} severity={mcpNotice.severity} Icon={Icon} />}
    </section>

    <section hidden={section !== 'skills'} className="settings-card settings-card-wide agent-library-card">
      <div className="settings-card-heading"><Icon name="Note" size={17} /><div><strong>Shared skills</strong><small>One global library discovered by OpenCode and Codex</small></div></div>
      {!skillDraft && <div className="agent-library-grid">{!skills.length && <small className="agent-library-empty">No shared skills yet</small>}{skills.map(skill => <button type="button" className={'agent-library-tile ' + (skillDraft?.originalName === skill.name ? 'selected' : '')} key={skill.name} aria-pressed={skillDraft?.originalName === skill.name} disabled={!skill.managed} title={skill.description || skill.name} onClick={() => { if (!skill.managed) return; setArmedSkillDelete(false); pendingSkill.current = { name: skill.name, description: skill.description || '' }; send('agentLibrary', { action: 'skillRead', name: skill.name }); }}>
        <span className="agent-library-item-mark"><Icon name="Note" size={16} /></span>
        <span className="agent-library-tile-copy"><strong>{skill.name}</strong><small>{skill.managed ? 'Island skill' : 'Existing skill'}</small></span>
      </button>)}<button type="button" className="agent-library-tile agent-library-add" onClick={() => { setArmedSkillDelete(false); setSkillDraft({ name: '', originalName: '', description: '', instructions: '' }); }}><span className="agent-library-item-mark"><Icon name="Add" size={16} /></span><span className="agent-library-tile-copy"><strong>Create skill</strong><small>Shared workflow</small></span></button></div>}
      {skillDraft && <>
      <div className="agent-library-editor agent-skill-editor">
        <div className="agent-library-form-grid">
          <label><span>Name</span><input className="agent-library-input" value={skillDraft.name} maxLength={64} placeholder="code-review" disabled={!!skillDraft.originalName} onChange={event => setSkillDraft({ ...skillDraft, name: event.target.value.toLowerCase() })} /></label>
          <label><span>Description</span><input className="agent-library-input" value={skillDraft.description} maxLength={1024} placeholder="When should this workflow be used?" onChange={event => setSkillDraft({ ...skillDraft, description: event.target.value })} /></label>
        </div>
        <label><span>Instructions · Markdown</span><textarea className="agent-library-input agent-library-instructions" value={skillDraft.instructions} maxLength={126000} placeholder="Write the reusable workflow instructions…" onChange={event => setSkillDraft({ ...skillDraft, instructions: event.target.value })} /></label>
      </div>
      <div className="agent-library-actions">{skillDraft.originalName && <button type="button" className={'settings-inline-action agent-library-remove ' + (armedSkillDelete ? 'is-armed' : '')} onClick={() => { if (armedSkillDelete) send('agentLibrary', { action: 'skillDelete', name: skillDraft.originalName }); else setArmedSkillDelete(true); }}>{armedSkillDelete ? 'Confirm remove' : 'Remove skill'}</button>}<span /> <button type="button" className="settings-inline-action" onClick={() => { setSkillDraft(null); setArmedSkillDelete(false); }}>Cancel</button><button type="button" className="settings-inline-action agent-library-save" onClick={() => send('agentLibrary', { action: 'skillSave', ...skillDraft })}>Save skill</button></div>
      </>}
      <p hidden={!!skillDraft} className="settings-note">Shared skills are available to both Codex and OpenCode.</p>
      {skillNotice && <AgentFeedback notice={skillNotice.message} severity={skillNotice.severity} Icon={Icon} />}
    </section>
  </>;
}

function AgentSwitch({ label, value, onChange }) {
  return <div className="settings-line"><span>{label}</span><button type="button" role="switch" aria-label={label} aria-checked={!!value} className={'settings-switch ' + (value ? 'on' : '')} onClick={onChange}><i /></button></div>;
}

function AgentFeedback({ error, notice, severity = 'info', operationKind, authRequired, authMethods = [], provider, send, authenticating = false, embedded = false, Icon }) {
  if (error) {
    const message = String(error).trim();
    const operation = agentOperationLabels[operationKind];
    const title = authRequired ? 'Sign in to continue' : operation ? `Couldn't ${operation}` : 'Agent request failed';
    const methods = authMethods.filter(method => method?.id);
    return <section className={'agent-feedback is-error' + (embedded ? ' is-embedded' : '')} role="alert" aria-live="assertive">
      <Icon name="Cancel" size={15} />
      <div className="agent-feedback-body">
        <div className="agent-feedback-heading"><strong>{title}</strong><small>{providerName(provider)}</small></div>
        <p>{message}</p>
        {message.length > 90 && <details className="agent-feedback-details"><summary>Technical details</summary><pre>{message}</pre></details>}
        {!!methods.length && <div className="agent-feedback-actions">{methods.map(method => <button type="button" key={method.id} disabled={authenticating} onClick={() => sendAgent(send, provider, 'login', { methodId: method.id })}>{authenticating ? 'Signing in…' : method.name || 'Sign in'}</button>)}</div>}
      </div>
    </section>;
  }
  if (!notice) return null;
  const tone = ['success', 'warning', 'error'].includes(severity) ? severity : 'info';
  const message = String(notice).trim();
  return <div className={`agent-feedback is-${tone}${embedded ? ' is-embedded' : ''}`} role="status" aria-live="polite">
    <Icon name={tone === 'success' ? 'Check' : tone === 'warning' ? 'Pending' : tone === 'error' ? 'Cancel' : 'Link'} size={15} />
    <div className="agent-feedback-body"><p>{message}</p>{message.length > 90 && <details className="agent-feedback-details"><summary>More details</summary><pre>{message}</pre></details>}</div>
  </div>;
}

function permissionFields(detail) {
  try {
    const value = JSON.parse(detail);
    if (!value || typeof value !== 'object' || Array.isArray(value)) return null;
    return Object.entries(value).map(([key, item]) => [key, typeof item === 'string' ? item : JSON.stringify(item)]);
  } catch { return null; }
}

function ApprovalCard({ permission, provider, send, Icon, compact = false }) {
  const fields = permissionFields(permission.detail || '');
  const hasFields = !!fields?.length;
  const showFullDetail = !!permission.detail && (hasFields || permission.detail.length > 120);
  return <section className={'agent-approval ' + (compact ? 'compact' : '')} aria-label="Agent approval required">
    <div className="agent-approval-title"><Icon name="Lock" size={15} /><strong>Approval needed</strong><small>{providerName(provider)}</small></div>
    <p>{permission.action || 'Agent action'}</p>
    {hasFields ? <dl className="agent-approval-fields">{fields.slice(0, 4).map(([key, value]) => <div key={key}><dt>{key}</dt><dd title={value}>{value}</dd></div>)}</dl> : permission.detail && <p className="agent-approval-detail">{permission.detail}</p>}
    {showFullDetail && <details className="agent-approval-more"><summary>{fields ? 'Full request' : 'More details'}</summary><pre>{fields ? JSON.stringify(JSON.parse(permission.detail), null, 2) : permission.detail}</pre></details>}
    <div className="agent-approval-actions">{permission.options.map(option => {
      const kind = String(option.kind || '').toLocaleLowerCase();
      const description = typeof option.meta?.description === 'string' ? option.meta.description : '';
      const decisionClass = kind.includes('allow') || kind.includes('accept')
        ? (kind.includes('always') || kind.includes('session') ? 'allow-persistent' : 'allow')
        : kind.includes('reject') || kind.includes('deny') ? 'deny' : '';
      const label = option.name || option.kind || 'Choose';
      return <button type="button" key={option.optionId} title={description} aria-label={description ? `${label}: ${description}` : label} className={decisionClass} onClick={() => sendAgent(send, provider, 'reply', { sessionId: permission.sessionId, requestId: permission.id, optionId: option.optionId })}>{label}</button>;
    })}</div>
  </section>;
}

function QuestionCard({ question, provider, send, Icon }) {
  const fields = Object.entries(question.schema?.properties || {});
  const [answers, setAnswers] = useState(() => Object.fromEntries(fields.filter(([, rule]) => rule.default !== undefined).map(([key, rule]) => [key, rule.default])));
  if (question.mode === 'url') return <section className="agent-question" aria-label="Agent sign-in request">
    <div className="agent-approval-title"><Icon name="Lock" size={15} /><strong>{providerName(provider)} needs browser access</strong></div>
    <p>{question.message || 'Continue in your browser.'}</p><code className="agent-question-url">{question.url}</code>
    <div className="agent-approval-actions"><button type="button" onClick={() => sendAgent(send, provider, 'answer', { sessionId: question.sessionId, requestId: question.id, choice: 'decline' })}>Decline</button><button type="button" className="allow" onClick={() => { send('openExternalUrl', { url: question.url }); sendAgent(send, provider, 'answer', { sessionId: question.sessionId, requestId: question.id, choice: 'accept' }); }}>Open and continue</button></div>
  </section>;
  const update = (key, value) => setAnswers(current => ({ ...current, [key]: value }));
  const choices = rule => (rule.oneOf || rule.enum || []).map(item => typeof item === 'object' ? { value: item.const, label: item.title || String(item.const) } : { value: item, label: String(item) });
  const answer = (choice, content = {}) => sendAgent(send, provider, 'answer', { sessionId: question.sessionId, requestId: question.id, choice, content });
  const required = new Set(question.schema?.required || []);
  const complete = fields.every(([key]) => !required.has(key) || (answers[key] !== undefined && answers[key] !== ''));
  return <section className="agent-question" aria-label="Agent question">
    <div className="agent-approval-title"><Icon name="Bot" size={15} /><strong>{providerName(provider)} asks</strong></div>
    <p>{question.message}</p>
    <div className="agent-question-fields">{fields.map(([key, rule]) => {
      const items = choices(rule.type === 'array' ? rule.items || {} : rule);
      return <div className="agent-question-field" key={key}><span>{rule.title || key}{required.has(key) ? ' *' : ''}</span>
        {rule.type === 'array' && items.length ? <span className="agent-question-choices">{items.map(item => <label key={String(item.value)}><input type="checkbox" checked={(answers[key] || []).includes(item.value)} onChange={event => update(key, event.target.checked ? [...(answers[key] || []), item.value] : (answers[key] || []).filter(value => value !== item.value))} />{item.label}</label>)}</span>
          : rule.type === 'boolean' ? <input type="checkbox" aria-label={rule.title || key} checked={!!answers[key]} onChange={event => update(key, event.target.checked)} />
            : items.length ? <ChoiceControl label={rule.title || key} value={answers[key] ?? ''} options={[{ value: '', label: 'Choose…' }, ...items]} onChange={value => update(key, value)} />
              : ['string', 'integer', 'number'].includes(rule.type) ? <input aria-label={rule.title || key} type={rule.type === 'string' ? 'text' : 'number'} value={answers[key] ?? ''} onChange={event => update(key, rule.type === 'string' || event.target.value === '' ? event.target.value : Number(event.target.value))} maxLength={4000} />
                : <small>This field needs the provider's own interface.</small>}
      </div>;
    })}</div>
    <div className="agent-approval-actions"><button type="button" onClick={() => answer('decline')}>Decline</button><button type="button" className="allow" disabled={!complete} onClick={() => answer('accept', answers)}>Send answer</button></div>
  </section>;
}

function AgentStepContent({ content = [] }) {
  const blocks = content.slice(0, 6);
  if (!blocks.length) return null;
  return <div className="agent-step-content">{blocks.map((block, index) => {
    if (block.type === 'image' && /^image\/(png|jpeg|gif|webp)$/.test(block.mimeType || '') && block.data) return <img key={index} src={`data:${block.mimeType};base64,${block.data}`} alt="Agent attachment" />;
    if (block.type === 'resource_link') return <small key={index}>{block.title || block.name || 'Attached resource'}</small>;
    if (block.type === 'diff' && (block.oldText !== undefined || block.newText !== undefined)) return <div className="agent-step-diff" key={index}><small>{block.path || 'File change'}</small><pre><del>{block.oldText ?? '(new file)'}</del><ins>{block.newText ?? '(deleted)'}</ins></pre></div>;
    const text = block.type === 'terminal_output' ? block.text : block.type === 'resource' ? block.resource?.text : block.type === 'diff' ? block.text : block.type === 'text' ? block.text : '';
    return text ? <pre key={index}>{String(text).slice(0, 6000)}</pre> : null;
  })}</div>;
}

export function AgentSettings({ state, Icon, send, onDetailChange }) {
  const [preferences, setPreferences] = useAgentPreferences();
  const [activeSection, setActiveSection] = useState('overview');
  const settingsSections = [
    { id: 'providers', title: 'Providers', detail: 'Connection, sign-in, and updates', icon: 'Agents' },
    { id: 'defaults', title: 'Defaults', detail: 'Provider and project folders', icon: 'FolderAdd' },
    { id: 'mcp', title: 'MCP tools', detail: 'Shared server connections', icon: 'Cloud' },
    { id: 'skills', title: 'Shared skills', detail: 'Create and manage workflows', icon: 'Note' },
    { id: 'prompt', title: 'Shared system prompt', detail: 'Instructions for every task', icon: 'Agents' },
    { id: 'presentation', title: 'Presentation', detail: 'Activity and answer display', icon: 'Controls' },
  ];
  const currentSection = settingsSections.find(item => item.id === activeSection);
  const openSection = id => { setActiveSection(id); onDetailChange?.(id !== 'overview'); };
  const providers = state.agents || {};
  useEffect(() => { for (const id of ['opencode', 'codex']) if (!providers[id]?.connected && !providers[id]?.installing) sendAgent(send, id, 'connect'); }, []);
  const folderTarget = useRef('');
  useEffect(() => {
    const previous = window.islandFolderPicked;
    window.islandFolderPicked = folder => {
      const id = folderTarget.current;
      if (!id) return;
      folderTarget.current = '';
      const saved = readAgentPreferences();
      saveAgentPreferences({ ...saved, provider: { ...saved.provider, [id]: { ...saved.provider[id], directory: folder } } });
    };
    return () => { window.islandFolderPicked = previous; };
  }, []);
  const pickFolder = id => { folderTarget.current = id; send('opencodePickFolder'); };
  return <div className={'settings-grid agent-settings ' + (activeSection === 'overview' ? 'is-overview' : 'is-detail')}>
    {activeSection === 'overview' ? <section className="settings-card settings-card-wide agent-settings-browser">
      <div className="settings-card-heading"><Icon name="Agents" size={17} /><div><strong>Agents</strong></div></div>
      <div className="agent-settings-menu" aria-label="Agent settings sections">{settingsSections.map(item => <button type="button" key={item.id} className="agent-option-row agent-settings-section-link" onClick={() => openSection(item.id)}>
        <span className="agent-option-symbol"><Icon name={item.icon} size={16} /></span><span><strong>{item.title}</strong><small>{item.detail}</small></span><Icon name="ChevronRight" size={15} />
      </button>)}</div>
    </section> : <header className="agent-settings-browser-head">
      <button type="button" className="agent-browser-back" onClick={() => openSection('overview')} aria-label="Back to Agent settings">Settings</button>
      <strong>{currentSection?.title}</strong>
    </header>}
    <section hidden={activeSection !== 'providers'} className="settings-card settings-card-wide"><div className="settings-card-heading"><Icon name="Agents" size={17} /><div><strong>Providers</strong></div></div>
      <div className="agent-settings-providers">{['opencode', 'codex'].map(id => {
        const agent = providers[id] || {};
        const opencode = state.opencode || {};
        const activeCount = (agent.activeCount || 0) + (id === 'opencode' ? opencode.activeCount || 0 : 0);
        const checking = id === 'codex' ? agent.checkingUpdate : opencode.checkingUpdate;
        const installing = agent.installing || (id === 'opencode' && opencode.installing);
        const updateAvailable = id === 'codex' ? agent.updateAvailable : opencode.updateAvailable;
        const latestVersion = id === 'codex' ? agent.latestVersion : opencode.latestVersion;
        const installedVersion = id === 'codex' ? agent.adapterVersion : opencode.installedVersion || agent.version;
        return <div className="agent-settings-provider" key={id}>
          <span className="agent-settings-provider-mark"><Icon name={id === 'codex' ? 'Agents' : 'Terminal'} size={17} /></span>
          <div className="agent-settings-provider-info">
            <strong>{providerName(id)}</strong>
            <small className={agent.error ? 'agent-settings-provider-state has-error' : 'agent-settings-provider-state'} title={agent.error || agent.executable || ''}>{agent.error ? (agent.authRequired ? 'Sign-in needed' : 'Needs attention') : agent.connected ? `Connected · ACP v${agent.protocolVersion || 1}` : agent.missing ? 'Install the ACP provider to connect' : 'Connect to see status'}</small>
          </div>
          <div className="agent-settings-provider-meta">
            <small title={agent.executable || ''}>{id === 'codex' ? `ACP adapter ${installedVersion || 'version not detected'}` : `OpenCode CLI ${installedVersion || 'version not detected'}`}</small>
            {(latestVersion || checking) && <small>{checking ? 'Checking stable releases…' : updateAvailable ? `Update ${latestVersion} available` : `Stable ${latestVersion}`}</small>}
            {agent.connected && agent.version && id === 'codex' && agent.version !== installedVersion && <small>Runtime {agent.version}</small>}
          </div>
          <div className="agent-settings-provider-actions">
            {(agent.authMethods || []).map(method => <button type="button" className="settings-inline-action" key={method.id} disabled={agent.authenticating || installing} onClick={() => sendAgent(send, id, 'login', { methodId: method.id })}>{agent.authenticating ? 'Signing in…' : method.name || 'Sign in'}</button>)}
            {agent.capabilities?.auth?.logout != null && <button type="button" className="settings-inline-action" disabled={activeCount > 0 || installing} onClick={() => sendAgent(send, id, 'logout')}>Sign out</button>}
            {(agent.missing || updateAvailable) && <button type="button" className="settings-inline-action" disabled={installing || activeCount > 0} onClick={() => id === 'opencode' && updateAvailable && !agent.missing ? send('opencodeUpdate') : sendAgent(send, id, 'install')}>{installing ? 'Updating…' : agent.missing ? 'Install' : 'Update'}</button>}
            <button type="button" className="settings-inline-action" disabled={checking || installing} onClick={() => sendAgent(send, id, 'checkUpdate')}>{checking ? 'Checking…' : 'Check updates'}</button>
            <button type="button" className="settings-inline-action" disabled={agent.authenticating || installing || activeCount > 0} onClick={() => sendAgent(send, id, 'connect')}>Reconnect</button>
          </div>
        </div>;
      })}</div>
      <p className="settings-note">Sign-in stays with the selected provider. Updates use each provider’s stable channel.</p>
    </section>
    <AgentLibrarySettings Icon={Icon} send={send} section={activeSection} />
    <section hidden={activeSection !== 'defaults'} className="settings-card settings-card-wide"><div className="settings-card-heading"><Icon name="Settings" size={17} /><div><strong>Defaults</strong></div></div>
      <div className="settings-choice"><span>Default provider</span><div className="settings-segments">{['opencode', 'codex'].map(id => <button type="button" key={id} className={preferences.defaultProvider === id ? 'selected' : ''} onClick={() => setPreferences({ defaultProvider: id, providerSetupComplete: true })}>{providerName(id)}</button>)}</div></div>
      {['opencode', 'codex'].map(id => <div className="agent-default-folder" key={id}><span>{providerName(id)} project</span><button type="button" onClick={() => pickFolder(id)} title={preferences.provider[id]?.directory || ''}><Icon name="FolderAdd" size={14} />{preferences.provider[id]?.directory?.split(/[\\/]/).at(-1) || 'Choose folder'}</button></div>)}
    </section>
    <section hidden={activeSection !== 'prompt'} className="settings-card settings-card-wide agent-system-prompt-card"><div className="settings-card-heading"><Icon name="Agents" size={17} /><div><strong>Shared system prompt</strong><small>Applies across Codex and OpenCode</small></div></div>
      <label className="agent-system-prompt-field"><span>Instructions for every task</span><textarea className="agent-library-input agent-system-prompt" value={preferences.systemPrompt || ''} maxLength={6000} placeholder="Set response style, project context, and working preferences…" onChange={event => setPreferences({ systemPrompt: event.target.value })} /></label>
      <p className="settings-note">Added to each new task and follow-up. Provider rules and project instructions remain in effect.</p>
    </section>
    <section hidden={activeSection !== 'presentation'} className="settings-card settings-card-wide"><div className="settings-card-heading"><Icon name="Controls" size={17} /><div><strong>Presentation</strong></div></div>
      <div className="settings-choice"><span>Activity</span><div className="settings-segments">{[['current', 'Current'], ['all', 'All']].map(([id, label]) => <button type="button" key={id} className={preferences.activityDetail === id ? 'selected' : ''} onClick={() => setPreferences({ activityDetail: id })}>{label}</button>)}</div></div>
      <div className="settings-choice"><span>Answer</span><div className="settings-segments">{[['preview', 'Preview'], ['full', 'Full']].map(([id, label]) => <button type="button" key={id} className={preferences.answerView === id ? 'selected' : ''} onClick={() => setPreferences({ answerView: id })}>{label}</button>)}</div></div>
      <AgentSwitch label="Show child-agent count" value={!!preferences.showChildCount} onChange={() => setPreferences({ showChildCount: !preferences.showChildCount })} />
      <AgentSwitch label="Keep follow-up visible while running" value={!!preferences.showComposerDuringRun} onChange={() => setPreferences({ showComposerDuringRun: !preferences.showComposerDuringRun })} />
      <AgentSwitch label="Whiteboard explainers" value={!!preferences.hybridWhiteboard} onChange={() => setPreferences({ hybridWhiteboard: !preferences.hybridWhiteboard })} />
      <p className="settings-note">Simple diagrams with shapes, connected arrows and sketches. Detailed human figures are not supported.</p>
    </section>
  </div>;
}

export function AgentsWorkspace({ state, Icon, send, renderAnswer, openCodeProfiles, onDictate, dictateActive, dictateSlotRef, dictatedText, dictatedAutoSubmit = false, onDictatedTextConsumed, voiceSurface = false, askMode = false }) {
  const [preferences, setPreferences] = useAgentPreferences();
  const [askContext, setAskContext] = useState(() => {
    try { return localStorage.getItem('island.ask.context') || ''; } catch { return ''; }
  });
  const [contextOpen, setContextOpen] = useState(false);
  const [pendingVoice, setPendingVoice] = useState(() => workspaceMemory.wakeTranscript);
  const [voiceAnswerRevision, setVoiceAnswerRevision] = useState(0);
  const voiceLoadRequest = useRef('');
  const [provider, setProvider] = useState(() => window.islandAgentFocus?.provider || workspaceMemory.provider || readAgentPreferences().defaultProvider);
  const lastDefaultProvider = useRef(preferences.defaultProvider);
  const [setupStep, setSetupStep] = useState('provider');
  const [focusSessionId, setFocusSessionId] = useState(() => window.islandAgentFocus?.sessionId || '');
  const [view, setView] = useState(() => workspaceMemory.view);
  const [drafts, setDrafts] = useState(() => workspaceMemory.drafts);
  const [files, setFiles] = useState(() => workspaceMemory.files);
  const [directory, setDirectory] = useState(() => ({ opencode: readAgentPreferences().provider.opencode?.directory || '', codex: readAgentPreferences().provider.codex?.directory || '' }));
  const [additionalDirectories, setAdditionalDirectories] = useState(() => ({ opencode: readAgentPreferences().provider.opencode?.additionalDirectories || [], codex: readAgentPreferences().provider.codex?.additionalDirectories || [] }));
  const [config, setConfig] = useState(() => ({ opencode: readAgentPreferences().provider.opencode?.config || {}, codex: readAgentPreferences().provider.codex?.config || {} }));
  const [query, setQuery] = useState(() => workspaceMemory.query);
  const [library, setLibrary] = useState({ skills: [], mcpServers: [] });
  const [projectEntries, setProjectEntries] = useState({ entries: [], relative: '' });
  const [mention, setMention] = useState(null);
  const [mentionIndex, setMentionIndex] = useState(0);
  const [optionId, setOptionId] = useState('');
  const [sending, setSending] = useState({ opencode: false, codex: false });
  const [detailsOpen, setDetailsOpen] = useState(false);
  const [answerExpanded, setAnswerExpanded] = useState(false);
  const [armedDelete, setArmedDelete] = useState('');

  const pendingRevision = useRef(workspaceMemory.pendingRevision);
  const submitted = useRef(workspaceMemory.submitted);
  const autoOpenedFailures = useRef(new Set());
  const providerRef = useRef(provider);
  const composerInputRef = useRef(null);
  const mentionRequest = useRef('');


  const folderPurpose = useRef('project');
  const preparedFor = useRef('');
  providerRef.current = provider;
  useEffect(() => {
    const text = dictatedText?.trim();
    if (!text) return;
    if (dictatedAutoSubmit) {
      const pending = { text, mode: askMode ? 'ask' : voiceSurface ? 'wake' : 'agent', revision: state.wakeWord?.revision, ...((!askMode && workspaceMemory.wakeTarget) || { provider, sessionId: state.agents?.[provider]?.selectedId || '' }) };
      workspaceMemory.wakeTranscript = pending;
      setPendingVoice(pending);
      voiceLoadRequest.current = '';
      setView('task');
      onDictatedTextConsumed?.();
      return;
    }
    setDrafts(current => {
      const existing = current[provider] || '';
      const next = `${existing}${existing && !/\s$/.test(existing) ? ' ' : ''}${text}`;
      workspaceMemory.drafts[provider] = next;
      return { ...current, [provider]: next };
    });
    setView('task');
    onDictatedTextConsumed?.();
  }, [dictatedText, dictatedAutoSubmit]);
  const agent = state.agents?.[provider] || {};
  const wakeWord = state.wakeWord || {};
  const wakeCapturePending = voiceSurface && wakeWord.status === 'triggered'
    && !!wakeWord.revision && wakeWord.revision !== workspaceMemory.lastWakeRevision;
  useEffect(() => {
    if (!wakeWord.revision || wakeWord.revision === workspaceMemory.lastWakeRevision || wakeWord.status !== 'triggered') return;
    workspaceMemory.lastWakeRevision = wakeWord.revision;
    workspaceMemory.wakeAwaitingAnswer = null;
    workspaceMemory.wakeTranscript = null;
    setPendingVoice(null);
    const savedWake = workspaceMemory.wakeSessions?.[provider];
    workspaceMemory.wakeTarget = { provider, sessionId: state.agents?.[provider]?.sessions?.some(item => item.id === savedWake) ? savedWake : '' };
    setView('task');
    setAnswerExpanded(false);
    setDetailsOpen(false);
    onDictate?.({ autoSubmit: true });
  }, [wakeWord.revision, wakeWord.status]);
  useEffect(() => {
    const dismiss = event => {
      if (event?.detail?.revision != null && event.detail.revision !== wakeWord.revision) return;
      workspaceMemory.wakeAwaitingAnswer = null;
      workspaceMemory.wakeTranscript = null;
      setPendingVoice(null);
    };
    window.addEventListener('island:voice-dismiss', dismiss);
    return () => window.removeEventListener('island:voice-dismiss', dismiss);
  }, [wakeWord.revision]);
  const sessions = agent.sessions || [];
  const session = sessions.find(item => item.id === agent.selectedId);
  const response = agent.response?.sessionId === session?.id ? agent.response : null;
  const permissions = (agent.permissions || []).filter(item => item.ownerSessionId === session?.id);
  const remainingPermissions = (agent.permissions || []).filter(item => item.ownerSessionId !== session?.id);
  const questions = (agent.questions || []).filter(item => item.ownerSessionId === session?.id);
  const remainingQuestions = (agent.questions || []).filter(item => item.ownerSessionId !== session?.id);
  const decisions = [...permissions.map(item => ({ kind: 'approval', item })), ...questions.map(item => ({ kind: 'question', item }))].sort((left, right) => left.item.raisedAt - right.item.raisedAt);
  const candidateOptions = session?.configOptions || [];
  const options = cleanOptions(candidateOptions);
  const activeConfig = session ? Object.fromEntries(options.map(option => [option.id, option.currentValue])) : config[provider];
  const commandQuery = (drafts[provider] || '').match(/^\/([^\s]*)$/)?.[1]?.toLowerCase();
  const matchingCommands = commandQuery == null ? [] : (session?.commands || []).filter(item => item.name?.toLowerCase().includes(commandQuery)).slice(0, 6);
  const modelOption = options.find(option => /model/i.test(`${option.id} ${option.name || ''}`));
  const modelValue = modelOption ? optionChoices(modelOption).find(choice => choice.value === activeConfig[modelOption.id])?.name || activeConfig[modelOption.id] : '';
  const projectRoot = session?.directory || directory[provider] || '';
  const mentionItems = useMemo(() => {
    if (!mention) return [];
    const queryText = mention.query.toLocaleLowerCase();
    const pathQuery = queryText.replace(/^file:/, '');
    const slash = pathQuery.lastIndexOf('/');
    const expectedDirectory = slash >= 0 ? pathQuery.slice(0, slash) : '';
    const entries = (projectEntries.root === projectRoot && projectEntries.relative === expectedDirectory ? projectEntries.entries || [] : []).filter(item => !pathQuery || item.name.toLocaleLowerCase().includes(pathQuery.split('/').at(-1) || ''))
      .map(item => ({ kind: item.kind, label: item.name, detail: item.kind === 'directory' ? 'Folder' : item.relative, path: item.path, relative: item.relative }));
    const generalQuery = !/^(file|skills?|mcp|app):/.test(queryText);
    const matchesText = (item, prefix) => `${item.name} ${item.description || ''}`.toLocaleLowerCase().includes(queryText.replace(prefix, ''));
    const skills = (library.skills || []).filter(item => (generalQuery || /^skills?:/.test(queryText)) && matchesText(item, /^skills?:/))
      .map(item => ({ kind: 'skill', label: item.name, detail: item.description || 'Shared skill', value: item.name }));
    const mcpServers = (library.mcpServers || []).filter(item => (generalQuery || queryText.startsWith('mcp:')) && matchesText(item, /^mcp:/))
      .map(item => ({ kind: 'mcp', label: item.name, detail: item.enabled ? 'MCP tool server' : 'Disabled server', value: item.name }));
    const apps = (state.commandApps || []).map(item => typeof item === 'string' ? item : item?.name || '').filter(Boolean)
      .map(path => ({ kind: 'app', label: path.split(/[\\/]/).at(-1).replace(/\.exe$/i, ''), detail: 'Local app', value: path }))
      .filter(item => (generalQuery || queryText.startsWith('app:')) && item.label.toLocaleLowerCase().includes(queryText.replace(/^app:/, '')));
    if (generalQuery) return [...entries.slice(0, 12), ...skills.slice(0, 6).map(item => ({ ...item, detail: `Skill · ${item.detail}` })), ...mcpServers.slice(0, 6), ...apps.slice(0, 6)];
    if (queryText.startsWith('skill:') || queryText.startsWith('skills:')) return skills.slice(0, 32);
    if (queryText.startsWith('mcp:')) return mcpServers.slice(0, 32);
    if (queryText.startsWith('app:')) return apps.slice(0, 32);
    return entries.slice(0, 100);
  }, [mention, projectEntries, library, state.commandApps, projectRoot]);
  const orderedSessions = useMemo(() => sessions.filter(item => `${item.title} ${item.directory}`.toLowerCase().includes(query.toLowerCase())), [sessions, query]);

  useEffect(() => { workspaceMemory.drafts = drafts; workspaceMemory.files = files; workspaceMemory.view = view; workspaceMemory.query = query; workspaceMemory.provider = provider; }, [drafts, files, view, query, provider]);
  useEffect(() => {
    const receive = result => {
      if (Array.isArray(result?.skills)) setLibrary(current => ({ ...current, skills: result.skills }));
      if (Array.isArray(result?.mcpServers)) setLibrary(current => ({ ...current, mcpServers: result.mcpServers }));
      if (result?.action === 'projectEntries' && result.requestId === mentionRequest.current) setProjectEntries(result.project ? { ...result.project, root: result.requestId.split('\n')[0] } : { entries: [], error: result.error });
    };
    window.islandAgentLibraryResult = receive;
    send('agentLibrary', { action: 'refresh' });
    return () => { if (window.islandAgentLibraryResult === receive) delete window.islandAgentLibraryResult; };
  }, [send]);
  useEffect(() => {
    const queryText = mention?.query || '';
    if (!mention || !projectRoot || /^(skills?|mcp|app):/i.test(queryText)) return undefined;
    const requested = queryText.replace(/^file:/i, '');
    const slash = requested.lastIndexOf('/');
    const relative = slash >= 0 ? requested.slice(0, slash) : '';
    const query = requested.slice(slash + 1);
    const requestId = `${projectRoot}\n${relative}\n${query}`;
    mentionRequest.current = requestId;
    const timer = setTimeout(() => send('agentLibrary', { action: 'projectEntries', root: projectRoot, relative, query, requestId }), 100);
    return () => clearTimeout(timer);
  }, [mention?.query, !!mention, projectRoot, send]);
  useEffect(() => {
    if (!mention || /^(file|skills?|mcp):/i.test(mention.query)) return undefined;
    const timer = setTimeout(() => send('commandAppSearch', { query: mention.query.replace(/^app:/i, '') }), 120);
    return () => clearTimeout(timer);
  }, [mention?.query, !!mention, send]);
  useEffect(() => { setMentionIndex(index => Math.min(index, Math.max(0, mentionItems.length - 1))); }, [mentionItems.length]);
  useEffect(() => { composerInputRef.current?.closest('form')?.querySelector(`[data-mention-index="${mentionIndex}"]`)?.scrollIntoView({ block: 'nearest' }); }, [mentionIndex]);
  useEffect(() => {
    const dictated = window.islandAgentDictatedText;
    if (typeof dictated !== 'string' || !dictated.trim()) return;
    delete window.islandAgentDictatedText;
    const id = providerRef.current;
    setDrafts(current => {
      const existing = current[id] || '';
      const next = `${existing}${existing && !/\s$/.test(existing) ? ' ' : ''}${dictated.trim()}`;
      workspaceMemory.drafts[id] = next;
      return { ...current, [id]: next };
    });
    requestAnimationFrame(() => composerInputRef.current?.focus());
  }, []);
  useEffect(() => {
    const field = composerInputRef.current;
    if (!field) return;
    field.style.height = 'auto';
    field.style.height = `${Math.min(field.scrollHeight, 112)}px`;
  }, [drafts[provider], provider, session?.id, view]);

  useEffect(() => setDetailsOpen(preferences.activityDetail === 'all'), [preferences.activityDetail]);
  useEffect(() => { autoOpenedFailures.current.clear(); }, [provider, session?.id]);
  useEffect(() => {
    const failed = response?.steps?.filter(step => ['failed', 'denied'].includes(agentStepState(step.status))) || [];
    if (failed.some(step => !autoOpenedFailures.current.has(step.id))) {
      failed.forEach(step => autoOpenedFailures.current.add(step.id));
      setDetailsOpen(true);
    }
  }, [response?.steps]);
  useEffect(() => {
    if (view !== 'options') { preparedFor.current = ''; return; }
    if (session || !agent.connected || agent.installing) return;
    const key = `${provider}:${projectRoot}`;
    if (preparedFor.current === key) return;
    preparedFor.current = key;
    sendAgent(send, provider, 'prepare', { directory: projectRoot, additionalDirectories: additionalDirectories[provider], config: Object.entries(config[provider]).map(([id, value]) => ({ id, value })) });
  }, [view, provider, projectRoot, session?.id, agent.connected, agent.installing]);
  useEffect(() => setAnswerExpanded(preferences.answerView === 'full'), [provider, session?.id, preferences.answerView]);
  useEffect(() => {
    const receive = event => {
      const text = event.detail;
      if (typeof text !== 'string' || !text.trim()) return;
      const id = providerRef.current;
      setDrafts(current => ({ ...current, [id]: `${current[id] || ''}${current[id] && !/\s$/.test(current[id]) ? ' ' : ''}${text.trim()}` }));
      requestAnimationFrame(() => composerInputRef.current?.focus());
    };
    window.addEventListener('island:agent-dictated', receive);
    return () => window.removeEventListener('island:agent-dictated', receive);
  }, []);

  useEffect(() => {
    if (lastDefaultProvider.current === preferences.defaultProvider) return;
    lastDefaultProvider.current = preferences.defaultProvider;
    if (!window.islandAgentFocus?.provider) setProvider(preferences.defaultProvider);
  }, [preferences.defaultProvider]);

  useEffect(() => {
    if (!state.agents?.[provider]?.connected && !state.agents?.[provider]?.installing) sendAgent(send, provider, 'connect');
  }, [provider]);
  useEffect(() => {
    if (!focusSessionId || !agent.connected || !sessions.some(item => item.id === focusSessionId)) return;
    sendAgent(send, provider, 'select', { sessionId: focusSessionId });
    setFocusSessionId('');
    window.islandAgentFocus = null;
  }, [provider, focusSessionId, agent.connected, sessions]);

  useEffect(() => {
    const previousFolder = window.islandFolderPicked;
    const previousFiles = window.islandFilesPicked;
    window.islandFolderPicked = folder => {
      const id = providerRef.current;
      if (folderPurpose.current === 'additional') {
        folderPurpose.current = 'project';
        setAdditionalDirectories(current => {
          const next = { ...current, [id]: [...new Set([...(current[id] || []), folder])].slice(0, 6) };
          const prefs = readAgentPreferences();
          saveAgentPreferences({ ...prefs, provider: { ...prefs.provider, [id]: { ...prefs.provider[id], additionalDirectories: next[id] } } });
          return next;
        });
        return;
      }
      setDirectory(current => ({ ...current, [id]: folder }));
      const next = readAgentPreferences();
      saveAgentPreferences({ ...next, provider: { ...next.provider, [id]: { ...next.provider[id], directory: folder } } });
    };
    window.islandFilesPicked = picked => { const id = providerRef.current; setFiles(current => ({ ...current, [id]: [...new Set([...(current[id] || []), ...(picked || [])])].slice(0, 8) })); };
    return () => { window.islandFolderPicked = previousFolder; window.islandFilesPicked = previousFiles; };
  }, []);

  useEffect(() => {
    for (const id of ['opencode', 'codex']) {
      const pending = pendingRevision.current[id];
      const current = state.agents?.[id];
      if (!current) continue;
      const revision = current.operationRevision || 0;
      const saved = submitted.current[id];
      if (saved && revision > saved.revision && current.operationKind === 'prompt' && !current.operationOk) {
        setDrafts(drafts => ({ ...drafts, [id]: drafts[id] || saved.text }));
        setFiles(files => ({ ...files, [id]: files[id]?.length ? files[id] : saved.files }));
        delete submitted.current[id];
      }
      if (saved && revision > saved.revision && current.operationKind === 'prompt' && current.operationOk && current.notice === 'Task finished') delete submitted.current[id];
      if (!pending || revision <= pending.revision || !['start', 'prompt'].includes(current.operationKind)) continue;
      if (['start', 'prompt'].includes(current.operationKind) && current.operationOk) {
        if (!pending.preserveDraft) {
          setDrafts(drafts => ({ ...drafts, [id]: '' }));
          setFiles(files => ({ ...files, [id]: [] }));
        }
        if (id === providerRef.current) setView('task');
      } else delete submitted.current[id];
      delete pendingRevision.current[id];
      setSending(sending => ({ ...sending, [id]: false }));
    }
  }, [state.agents]);

  const chooseSession = item => { sendAgent(send, provider, 'select', { sessionId: item.id }); setView('task'); setDetailsOpen(false); };
  const chooseConfig = (id, value) => {
    setConfig(current => ({ ...current, [provider]: { ...current[provider], [id]: value } }));
    if (session) sendAgent(send, provider, 'configure', { sessionId: session.id, configId: id, configValue: value });
    const prefs = readAgentPreferences();
    saveAgentPreferences({ ...prefs, provider: { ...prefs.provider, [provider]: { ...prefs.provider[provider], config: { ...prefs.provider[provider]?.config, [id]: value } } } });
  };
  const submitText = (value, attachedFiles = files[provider] || [], preserveDraft = false, responseMode = askMode ? 'ask' : voiceSurface ? 'wake' : 'agent') => {
    const text = value.trim();
    if ((!text && !attachedFiles.length) || sending[provider] || !agent.connected || agent.installing || (session && (!session.ready || session.active)) || (!session && agent.activeCount > 0)) return false;
    pendingRevision.current[provider] = { revision: agent.operationRevision || 0, preserveDraft };
    submitted.current[provider] = { revision: agent.operationRevision || 0, text, files: [...attachedFiles] };
    setSending(current => ({ ...current, [provider]: true }));
    if (responseMode === 'ask' || responseMode === 'wake') setVoiceAnswerRevision(value => value + 1);
    const systemPrompt = responseMode === 'ask'
      ? 'Island response mode: spoken-script\n' + [preferences.askInstructions ?? defaultAskInstructions, `Context supplied by the user for this conversation:\n${askContext}`].filter(Boolean).join('\n\n')
      : responseMode === 'wake'
        ? 'Island response mode: wake-answer\n' + (preferences.askInstructions ?? defaultAskInstructions)
      : preferences.systemPrompt || '';
    if (session) sendAgent(send, provider, 'prompt', { sessionId: session.id, text, files: attachedFiles, systemPrompt, hybridWhiteboard: preferences.hybridWhiteboard === true });
    else sendAgent(send, provider, 'start', { directory: directory[provider] || '', additionalDirectories: additionalDirectories[provider], text, files: attachedFiles, config: Object.entries(config[provider]).map(([id, value]) => ({ id, value })), systemPrompt, hybridWhiteboard: preferences.hybridWhiteboard === true });
    return true;
  };
  const submit = event => { event.preventDefault(); submitText(drafts[provider] || '', files[provider] || []); };
  useEffect(() => {
    const pending = pendingVoice;
    const transcript = pending?.text;
    if (pending && pending.provider !== provider) return;
    if (!transcript || !agent.connected || agent.installing || sending[provider] || session?.active || (!session && agent.activeCount > 0)) return;
    // Session history entries are not ready until ACP has loaded them. Voice
    // must perform the same load as clicking a session before sending a prompt.
    const targetId = pending.sessionId || '';
    if (targetId !== (agent.selectedId || '') || (session && !session.ready)) {
      if (voiceLoadRequest.current !== `${provider}:${targetId}`) {
        voiceLoadRequest.current = `${provider}:${targetId}`;
        sendAgent(send, provider, 'select', { sessionId: targetId });
      }
      return;
    }
    if (!submitText(transcript, [], true, pending.mode)) return;
    workspaceMemory.wakeTranscript = null;
    setPendingVoice(null);
    workspaceMemory.wakeAwaitingAnswer = pending.mode === 'wake' ? { provider, revision: pending.revision, operationRevision: agent.operationRevision || 0 } : null;
  }, [pendingVoice, agent.connected, agent.installing, agent.activeCount, agent.selectedId, session?.id, session?.active, session?.ready, sending[provider], provider, askContext]);
  useEffect(() => {
    const awaiting = workspaceMemory.wakeAwaitingAnswer;
    if (!awaiting || awaiting.provider !== provider || awaiting.revision !== wakeWord.revision || dictateActive || wakeCapturePending) return;
    if ((agent.operationRevision || 0) <= awaiting.operationRevision) return;
    if (session?.id) {
      workspaceMemory.wakeSessions ||= {}; workspaceMemory.wakeSessions[provider] = session.id;
    }
    if (session?.active || sending[provider] || response?.waiting) return;
    if (!response?.text && !response?.presentation && !agent.error) return;
    workspaceMemory.wakeAwaitingAnswer = null;
    send('wakeWordResume', { phrase: wakeWord.phrase || 'Hey Island', revision: wakeWord.revision });
  }, [session?.active, sending[provider], response?.waiting, response?.text, agent.error, agent.operationRevision, provider, wakeWord.revision, dictateActive, wakeCapturePending]);
  const changeDraft = value => setDrafts(current => ({ ...current, [provider]: value }));
  const updateMention = (value, caret) => {
    const before = value.slice(0, caret);
    const match = before.match(/(?:^|\s)@(?:"([^"]*)|([^\s]*))$/);
    if (!match) { setMention(null); return; }
    const query = match[1] ?? match[2];
    setMention({ query, start: caret - query.length - (match[1] != null ? 2 : 1), end: caret });
    setMentionIndex(0);
  };
  const chooseMention = item => {
    if (!mention) return;
    if (item.kind === 'file' && files[provider]?.length >= 8 && !files[provider].includes(item.path)) return;
    const current = drafts[provider] || '';
    let insertion = '';
    if (item.kind === 'directory') insertion = `@${/\s/.test(item.relative) ? '"' : ''}${item.relative}/`;
    else if (item.kind === 'file') {
      setFiles(previous => {
        const next = { ...previous, [provider]: [...new Set([...(previous[provider] || []), item.path])].slice(0, 8) };
        workspaceMemory.files[provider] = next[provider];
        return next;
      });
    } else insertion = `@${item.kind}:${item.kind === 'app' ? item.label : item.value} `;
    const next = `${current.slice(0, mention.start)}${insertion}${current.slice(mention.end)}`;
    changeDraft(next);
    setMention(item.kind === 'directory' ? { query: `${item.relative}/`, start: mention.start, end: mention.start + insertion.length } : null);
    const caret = mention.start + insertion.length;
    requestAnimationFrame(() => { composerInputRef.current?.focus(); composerInputRef.current?.setSelectionRange(caret, caret); });
  };
  const mentionFilter = prefix => {
    const current = drafts[provider] || '';
    if (!mention) return;
    const prefixText = `@${prefix}:`;
    const next = `${current.slice(0, mention.start)}${prefixText}${current.slice(mention.end)}`;
    changeDraft(next);
    setMention({ ...mention, query: `${prefix}:`, end: mention.start + prefixText.length });
    setMentionIndex(0);
    requestAnimationFrame(() => { composerInputRef.current?.focus(); composerInputRef.current?.setSelectionRange(mention.start + prefixText.length, mention.start + prefixText.length); });
  };
  const attach = () => send('opencodePickFiles');
  const chooseFolder = () => { folderPurpose.current = 'project'; send('opencodePickFolder'); };
  const chooseAdditionalFolder = () => { folderPurpose.current = 'additional'; send('opencodePickFolder'); };
  const removeAdditionalFolder = folder => {
    const next = { ...additionalDirectories, [provider]: additionalDirectories[provider].filter(item => item !== folder) };
    setAdditionalDirectories(next);
    const prefs = readAgentPreferences();
    saveAgentPreferences({ ...prefs, provider: { ...prefs.provider, [provider]: { ...prefs.provider[provider], additionalDirectories: next[provider] } } });
  };

  const voiceThinking = !!(dictatedText?.trim() || pendingVoice || sending[provider] || response?.waiting || session?.active);
  const voiceAwaitingText = !!(dictatedText?.trim() || pendingVoice || sending[provider] || (voiceThinking && !response?.text));
  const wakeCompact = voiceSurface && !askMode && ((dictateActive || wakeCapturePending)
    || (!agent.error && decisions.length === 0 && (voiceAwaitingText || (!response?.text && !response?.presentation))));
  const thinkingIndicator = <div className="voice-thinking" role="status"><span aria-hidden="true"><i/><i/><i/></span>Thinking…</div>;
  if (askMode && contextOpen) return <section className="voice-answer-surface is-ask ask-context-page" data-layout-height={330}>
    <header><strong>Context</strong></header>
    <label className="ask-context">Background and preferences<textarea autoFocus value={askContext} onChange={e => { setAskContext(e.target.value); try { localStorage.setItem('island.ask.context', e.target.value); } catch {} }} placeholder="What is this conversation about? What should the answers take into account?" /></label>
    <footer><button type="button" onClick={() => setContextOpen(false)}><Icon name="PreviousMonth" size={15}/>Back to Ask</button></footer>
  </section>;
  if (voiceSurface || askMode) return <section className={'voice-answer-surface ' + (askMode ? 'is-ask' : 'is-wake')} data-wake-phase={wakeCompact ? "compact" : "answer"} data-layout-height={wakeCompact ? 34 : 330}>
    {(!voiceSurface || askMode) && <header><strong>{askMode ? 'Ask' : 'Voice'}</strong><span>{dictateActive ? 'Microphone' : voiceThinking ? 'Thinking' : 'Ready'}</span>
      {askMode && <button onClick={() => setContextOpen(!contextOpen)} aria-expanded={contextOpen}>Context</button>}
      <button onClick={() => { send('voiceSurface', { open: false }); send('page', { page: 'Agents' }); }}>Session</button>
      {voiceSurface && <button aria-label="Dismiss voice answer" onClick={() => { if (dictateActive) window.dispatchEvent(new Event('island:voice-dismiss')); send('voiceSurface', { open: false, dismiss: true }); }}><Icon name="Cancel" size={15}/></button>}
    </header>}
    {contextOpen && <label className="ask-context">Background and preferences<textarea value={askContext} onChange={e => { setAskContext(e.target.value); try { localStorage.setItem('island.ask.context', e.target.value); } catch {} }} placeholder="What is this conversation about?" /></label>}
    {(dictateActive || wakeCapturePending) ? <div key="capture" className="voice-capture"><div ref={dictateSlotRef}/></div> : <>
      {decisions.map(decision => decision.kind === 'approval' ? <ApprovalCard key={decision.item.id} permission={decision.item} provider={provider} send={send} Icon={Icon}/> : <QuestionCard key={decision.item.id} question={decision.item} provider={provider} send={send} Icon={Icon}/>)}
      <div key={`${provider}:${voiceAnswerRevision}`} className="voice-answer-text" aria-busy={voiceThinking} aria-live="polite">{agent.error ? <p role="alert">{agent.error}</p> : wakeCompact ? <div className="wake-listening-surface" role="status" aria-label="Thinking"><WakeEyes phase="thinking" /></div> : voiceAwaitingText ? thinkingIndicator : response?.text || (!voiceSurface && response?.presentation) ? <>{!voiceSurface && response?.presentation && <AgentPresentationView presentation={response.presentation}/>} {(askMode || voiceSurface) ? (voiceSurface ? plainVoiceText(response.text) : response.text || '').split(/\n+/).filter(Boolean).map((line, index) => <div className="ask-answer-line" key={`${voiceAnswerRevision}:${index}`} style={{ '--line-delay': `${Math.min(index, 6) * 45}ms` }}>{voiceSurface ? line : renderAnswer(line)}</div>) : renderAnswer(response.text || '')}</> : voiceThinking ? thinkingIndicator : voiceSurface && response?.presentation ? 'This response has no text summary. Its details are available in Agents → Sessions.' : 'Ask a question to begin.'}</div>
      {askMode && <button className="ask-hold" disabled={!state.speechSettings?.configured || !agent.connected || session?.active} onPointerDown={e => { e.currentTarget.setPointerCapture(e.pointerId); onDictate?.({ autoSubmit: true, hold: true }); }} onPointerUp={() => { window.islandHoldReleased = true; window.dispatchEvent(new Event('island:hold-release')); }} onPointerCancel={() => { window.islandHoldReleased = true; window.dispatchEvent(new Event('island:hold-release')); }} onKeyDown={e => { if (e.key === "Enter") onDictate?.({ autoSubmit: true, hold: true }); }}>Hold Space to ask · microphone</button>}
    </>}
  </section>;
  if (!preferences.providerSetupComplete && !window.islandAgentFocus?.provider) {
    const setupAgent = state.agents?.[provider] || {};
    const setupHeight = setupStep === 'configure' ? 330 : 230;
    return <section className={'agent-workspace agent-onboarding ' + (setupStep === 'configure' ? 'is-configuring' : '')} data-layout-height={setupHeight} style={{ '--agent-height': `${setupHeight}px` }} aria-label="Set up agent provider">
      <div className="agent-onboarding-heading">
        {setupStep === 'configure' && <button type="button" className="agent-onboarding-back" aria-label="Back to provider choice" onClick={() => setSetupStep('provider')}><Icon name="PreviousMonth" size={16} /></button>}
        <span className="eyebrow">{setupStep === 'provider' ? '01 / 02' : '02 / 02'}</span>
        <h2>{setupStep === 'provider' ? 'Choose your agent' : `Set up ${providerName(provider)}`}</h2>
      </div>
      {setupStep === 'provider' ? <>
        <div className="agent-onboarding-providers" role="group" aria-label="Choose a provider">{['opencode', 'codex'].map(id => <button type="button" key={id} className={provider === id ? 'selected' : ''} aria-pressed={provider === id} onClick={() => setProvider(id)}><span className="agent-onboarding-provider-icon"><Icon name={id === 'codex' ? 'Agents' : 'Terminal'} size={19} /></span><strong>{providerName(id)}</strong></button>)}</div>
        <button type="button" className="agent-onboarding-continue" onClick={() => setSetupStep('configure')}>Next<Icon name="ChevronRight" size={15} /></button>
      </> : <>
        <div className="agent-onboarding-project"><span>Project <small>Optional</small></span><button type="button" title={directory[provider] || 'Choose a project folder'} onClick={() => { folderPurpose.current = 'project'; send('opencodePickFolder'); }}><Icon name="FolderAdd" size={15} /><span>{directory[provider]?.split(/[\\/]/).at(-1) || 'Choose folder'}</span></button></div>
        {(setupAgent.error || setupAgent.missing) && <AgentFeedback error={setupAgent.error} notice={!setupAgent.error ? `${providerName(provider)} is not installed yet.` : ''} severity="warning" operationKind={setupAgent.operationKind} authRequired={setupAgent.authRequired} provider={provider} embedded Icon={Icon} />}
        <div className="agent-onboarding-actions">
          {setupAgent.missing && <button type="button" disabled={setupAgent.installing} onClick={() => sendAgent(send, provider, 'install')}><Icon name="Add" size={14} />{setupAgent.installing ? 'Installing…' : 'Install'}</button>}
          {!setupAgent.connected && !setupAgent.missing && <button type="button" onClick={() => sendAgent(send, provider, 'connect')}><Icon name="Refresh" size={14} />Reconnect</button>}
          {setupAgent.authMethods?.map(method => <button type="button" key={method.id} disabled={setupAgent.authenticating} onClick={() => sendAgent(send, provider, 'login', { methodId: method.id })}>{setupAgent.authenticating ? 'Signing in…' : method.name || 'Sign in'}</button>)}
        </div>
        <button type="button" className="agent-onboarding-continue" onClick={() => setPreferences({ defaultProvider: provider, providerSetupComplete: true })}>Start<Icon name="ChevronRight" size={15} /></button>
      </>}
    </section>;
  }

  const layoutHeight = !session && view === 'task' ? (mention ? 350 : 260) : 400;
  const showComposer = !session?.active || preferences.showComposerDuringRun || !!drafts[provider]?.trim() || !!files[provider]?.length;
  const steps = response?.steps || [];
  const activeStep = [...steps].reverse().find(step => agentStepState(step.status) === 'running');
  const attentionStepCount = steps.filter(step => ['failed', 'denied'].includes(agentStepState(step.status))).length;
  const fullAnswer = answerExpanded && view === 'task' && !!session;
  return <section className={'agent-workspace ' + ((session && view === 'task') || view !== 'task' ? 'has-session' : '') + (fullAnswer ? ' is-answer-full' : '')} data-layout-height={layoutHeight} style={{ '--agent-height': `${layoutHeight}px` }} aria-label="Agents workspace">

    <div className={'agent-tabs ' + (fullAnswer ? 'is-answer-toolbar' : '')} role={fullAnswer ? undefined : 'tablist'} aria-label={fullAnswer ? undefined : 'Agent work'}>
      {fullAnswer ? <div className="agent-full-toolbar"><button type="button" className="agent-new-task" onClick={() => { sendAgent(send, provider, 'select', { sessionId: '' }); setView('task'); setAnswerExpanded(false); }}>New task</button></div> : <><button type="button" role="tab" aria-selected={view === 'task'} className={view === 'task' ? 'selected' : ''} onClick={() => setView('task')}><Icon name="Add" size={14} />Task</button><button type="button" role="tab" aria-selected={view === 'options'} className={view === 'options' ? 'selected' : ''} onClick={() => { setOptionId(''); setView('options'); }}><Icon name="Settings" size={14} />Options</button><button type="button" role="tab" aria-selected={view === 'sessions'} className={view === 'sessions' ? 'selected' : ''} onClick={() => { setView('sessions'); sendAgent(send, provider, 'refresh'); }}><Icon name="History" size={14} />Sessions</button><button type="button" role="tab" aria-selected={view === 'profiles'} className={view === 'profiles' ? 'selected' : ''} onClick={() => setView('profiles')}><Icon name="Bot" size={14} />Profiles</button><span className="agent-tab-spacer" />{session && <button type="button" className="agent-new-task" onClick={() => { sendAgent(send, provider, 'select', { sessionId: '' }); setView('task'); }}>New task</button>}</>}
    </div>
    {!agent.connected ? <div className="agent-empty"><span><Icon name={provider === 'codex' ? 'Agents' : 'Terminal'} size={22} /></span><strong>{agent.missing ? `Add ${providerName(provider)} ACP` : `Connecting to ${providerName(provider)}`}</strong>{agent.error ? <AgentFeedback error={agent.error} operationKind={agent.operationKind} authRequired={agent.authRequired} provider={provider} embedded Icon={Icon} /> : <p>{provider === 'codex' ? 'Install the codex-acp adapter to use your Codex sign-in.' : 'Start the OpenCode CLI to continue.'}</p>}<div className="agent-empty-actions"><button type="button" onClick={() => sendAgent(send, provider, 'connect')}><Icon name="Refresh" size={15} />Reconnect</button>{agent.missing && <button type="button" disabled={agent.installing} onClick={() => sendAgent(send, provider, 'install')}><Icon name="Add" size={15} />{agent.installing ? 'Installing…' : 'Install'}</button>}{agent.authMethods?.map(method => <button type="button" key={method.id} onClick={() => sendAgent(send, provider, 'login', { methodId: method.id })}>{method.name || 'Sign in'}</button>)}</div></div> : <>
      {view === 'sessions' && <div className="agent-sessions-view"><div className="agent-list-header"><div className="agent-list-title"><strong>Sessions</strong>{agent.capabilities?.sessionCapabilities?.list != null && <span className="agent-session-count">{orderedSessions.length}</span>}</div><button type="button" className="agent-list-refresh" onClick={() => sendAgent(send, provider, 'refresh')} aria-label="Refresh sessions"><Icon name="Refresh" size={15} /></button></div>{agent.capabilities?.sessionCapabilities?.list != null ? <><label className="agent-search"><Icon name="Search" size={15} /><input value={query} onChange={event => setQuery(event.target.value)} placeholder="Find a task" aria-label="Find agent session" /></label><div className="agent-session-list">{orderedSessions.map(item => <div className="agent-session-row" key={item.id}><button type="button" className="agent-session-main" onClick={() => chooseSession(item)}><span className="agent-session-copy"><strong>{item.title || 'Untitled task'}</strong><small title={item.directory || 'No folder'}>{item.directory || 'No folder'}</small></span>{item.active && <span className="agent-session-state"><i className="agent-live-dot" />Active</span>}</button><div className="agent-session-actions">
        {agent.capabilities?.sessionCapabilities?.fork != null && !item.active && <button type="button" title="Fork session" aria-label={`Fork ${item.title || 'session'}`} onClick={() => { sendAgent(send, provider, 'fork', { sessionId: item.id }); setView('task'); }}><Icon name="Copy" size={14} /></button>}
        {agent.capabilities?.sessionCapabilities?.close != null && item.active && <button type="button" title="Close session" aria-label={`Close ${item.title || 'session'}`} onClick={() => sendAgent(send, provider, 'close', { sessionId: item.id })}><Icon name="Cancel" size={14} /></button>}
        {agent.capabilities?.sessionCapabilities?.delete != null && !item.active && <button type="button" title={armedDelete === item.id ? 'Confirm removal' : 'Remove session'} aria-label={`${armedDelete === item.id ? 'Confirm removal of' : 'Remove'} ${item.title || 'session'}`} onClick={() => { if (armedDelete === item.id) { sendAgent(send, provider, 'delete', { sessionId: item.id }); setArmedDelete(''); } else setArmedDelete(item.id); }}>{armedDelete === item.id ? 'Delete?' : <Icon name="Trash" size={14} />}</button>}
      </div></div>)}{!orderedSessions.length && <div className="agent-list-empty">{query ? 'No matching sessions' : 'No sessions yet'}</div>}</div>{agent.nextCursor && <button type="button" className="agent-more" onClick={() => sendAgent(send, provider, 'more')}>More sessions</button>}</> : <div className="agent-list-empty">This provider does not advertise session listing.</div>}</div>}
      {view === 'profiles' && <div className="agent-profiles-view">{provider === 'opencode' ? openCodeProfiles : <div className="agent-provider-profile"><span><Icon name="Agents" size={19} /></span><div><strong>Codex</strong><small>Uses your Codex account and session choices</small></div></div>}</div>}
      {view === 'options' && <div className="agent-config-view">
        <div className="agent-config-heading"><div><span className="eyebrow">TASK OPTIONS</span><strong>{session ? session.title || 'Current task' : 'New task'}</strong></div>{session && <small>{session.directory?.split(/[\\/]/).at(-1) || 'No project folder'}</small>}</div>
        {!session && <div className="agent-project"><button type="button" onClick={chooseFolder}><Icon name="FolderAdd" size={16} /><span>{directory[provider] ? directory[provider].split(/[\\/]/).at(-1) : 'Choose project'}</span></button>{directory[provider] && <small title={directory[provider]}>{directory[provider]}</small>}</div>}
        {session && <>
          {options.length ? <AgentOptions key={provider + session.id} options={options} values={activeConfig} onChange={chooseConfig} Icon={Icon} selectedId={optionId} onSelectId={setOptionId} /> : <div className="agent-list-empty">This session has no provider-specific options.</div>}
          {!options.length && session.modes?.availableModes?.length > 0 && <ChoiceControl label="Mode" value={session.modes.currentModeId || ''} options={session.modes.availableModes.map(mode => ({ value: mode.id, label: mode.name || mode.id }))} onChange={modeId => sendAgent(send, provider, 'mode', { sessionId: session.id, modeId })} />}
        </>}
        {!session && <>
          {agent.capabilities?.sessionCapabilities?.additionalDirectories != null && <div className="agent-extra-folders"><button type="button" onClick={chooseAdditionalFolder}><Icon name="Add" size={13} />More folders</button>{additionalDirectories[provider]?.map(folder => <span key={folder} title={folder}>{folder.split(/[\\/]/).at(-1)}<button type="button" aria-label={`Remove ${folder}`} onClick={() => removeAdditionalFolder(folder)}><Icon name="Cancel" size={11} /></button></span>)}</div>}
          <button type="button" className="agent-prepare" disabled={!agent.connected || agent.installing} onClick={() => sendAgent(send, provider, 'prepare', { directory: directory[provider] || '', additionalDirectories: additionalDirectories[provider], config: Object.entries(config[provider]).map(([id, value]) => ({ id, value })) })}>Reload choices</button>
          <small className="agent-config-note">Loading provider choices…</small>
        </>}
      </div>}
      {view === 'task' && <div className="agent-task-view">
        {session ? <div className="agent-result"><div className="agent-result-head">{answerExpanded && <button type="button" className="agent-answer-back" onClick={() => setAnswerExpanded(false)} aria-label="Back to task">Back</button>}<div><small>{providerName(provider)} · {session.active ? 'Running' : session.ready ? 'Ready' : 'Loading'}</small><strong>{session.title || 'Untitled task'}</strong></div>{!dictateActive && !answerExpanded && (response?.text || response?.presentation) && <button type="button" className="agent-answer-open" onClick={() => setAnswerExpanded(true)} aria-label="Open full answer">Full view</button>}{session.active && <button type="button" aria-label="Stop agent task" title="Stop task" className="agent-stop" onClick={() => sendAgent(send, provider, 'cancel', { sessionId: session.id })}><i /></button>}</div>
          {decisions[0]?.kind === 'approval' && <ApprovalCard key={decisions[0].item.id} permission={decisions[0].item} provider={provider} send={send} Icon={Icon} />}
          {decisions[0]?.kind === 'question' && <QuestionCard key={decisions[0].item.id} question={decisions[0].item} provider={provider} send={send} Icon={Icon} />}
          <div className="agent-result-scroll" tabIndex={0} aria-label="Agent response and activity">
            {decisions.slice(1).map(decision => decision.kind === 'approval' ? <ApprovalCard key={decision.item.id} permission={decision.item} provider={provider} send={send} Icon={Icon} /> : <QuestionCard key={decision.item.id} question={decision.item} provider={provider} send={send} Icon={Icon} />)}
            {response?.presentation && <AgentPresentationView presentation={response.presentation} />}
            {response?.text ? <><div className={'opencode-response-text agent-answer ' + (response.text.length > 1800 && !answerExpanded ? 'is-preview' : '')}>{renderAnswer(response.text)}</div>{response.text.length > 1800 && !answerExpanded && !dictateActive && <button type="button" className="agent-answer-toggle" onClick={() => setAnswerExpanded(true)}>Open full answer</button>}</> : !response?.presentation && <div className="agent-result-placeholder">{session.active || response?.waiting ? 'The response will appear here.' : 'No response loaded for this session.'}</div>}
            {response?.truncated && <small className="agent-limit-note">The response reached the display limit.</small>}
            {!!steps.length && <div className="agent-activity"><button type="button" className="agent-activity-trigger" onClick={() => setDetailsOpen(value => !value)} aria-expanded={detailsOpen}><Icon name="Terminal" size={14} /><span>{activeStep?.title || `${steps.length} tool actions`}{attentionStepCount > 0 && <small>{attentionStepCount} need review</small>}</span><Icon name="ChevronDown" size={13} /></button>{detailsOpen && <ol aria-label="Tool activity">{steps.map(step => {
              const state = agentStepState(step.status);
              return <li key={step.id} className={`state-${state}`}><i aria-hidden="true" /><div className="agent-activity-copy"><strong>{step.title || 'Tool action'}</strong>{step.detail && <small className="agent-step-detail" title={step.detail}>{step.detail}</small>}<AgentStepContent content={step.content} /></div><small className={`agent-step-state state-${state}`}>{agentStepLabel(state)}</small></li>;
            })}</ol>}</div>}
            {response?.stopReason && response.stopReason !== 'end_turn' && <small className="agent-stop-reason">{({ max_tokens: 'Response reached the token limit', max_turn_requests: 'Response reached the turn limit', refusal: 'Provider declined this request', cancelled: 'Task cancelled' })[response.stopReason] || response.stopReason}</small>}
            {!!response?.plan?.length && <div className="agent-plan"><strong>Plan</strong>{response.plan.map((entry, index) => <div key={index}>{entry.content || entry.title || 'Step'}<small>{entry.status || ''}</small></div>)}</div>}
            {(response?.usage?.totalTokens > 0 || response?.usage?.used > 0) && <small className="agent-usage">{response.usage.totalTokens > 0 ? `${response.usage.totalTokens.toLocaleString()} tokens` : `${response.usage.used.toLocaleString()}${response.usage.size ? ` / ${response.usage.size.toLocaleString()}` : ''} context tokens`}</small>}
          </div>
        </div> : <div className="agent-start">
          <div className="agent-start-copy"><h2>New task</h2></div>
          <div className="agent-project"><button type="button" title={projectRoot || 'Choose a project folder'} onClick={chooseFolder}><Icon name="FolderAdd" size={16} /><span>{projectRoot.split(/[\\/]/).at(-1) || 'Choose project'}</span></button></div>
        </div>}
        {(dictateActive || showComposer) && <div className={'agent-composer-slot ' + (answerExpanded ? 'is-concealed' : '')} inert={answerExpanded} aria-hidden={answerExpanded}>{dictateActive ? <div className="agent-voice-composer" ref={dictateSlotRef} /> : <form className={'agent-composer ' + (session ? 'is-followup ' : '') + (mention ? 'has-context' : '')} onSubmit={submit}>
          {!!matchingCommands.length && <div className="agent-command-list" aria-label="Available agent commands">{matchingCommands.map(command => <button type="button" key={command.name} onClick={() => changeDraft(`/${command.name} `)}><strong>/{command.name}</strong><span>{command.description || ''}</span></button>)}</div>}
          {mention && <div className="agent-mention-menu" role="listbox" aria-label="Add context">
            <div className="agent-mention-filters"><span>Context</span>{[['file', 'Files'], ['skill', 'Skills'], ['mcp', 'MCP'], ['app', 'Apps']].map(([id, label]) => <button type="button" key={id} onMouseDown={event => event.preventDefault()} onClick={() => mentionFilter(id)}>{label}</button>)}</div>
            <div className="agent-mention-results">
              {mentionItems.map((item, index) => <button type="button" role="option" aria-selected={mentionIndex === index} data-mention-index={index} disabled={item.kind === 'file' && files[provider]?.length >= 8 && !files[provider].includes(item.path)} key={`${item.kind}:${item.path || item.value}`} onMouseDown={event => event.preventDefault()} onMouseEnter={() => setMentionIndex(index)} onClick={() => chooseMention(item)}>
                <span className="agent-mention-icon"><Icon name={item.kind === 'directory' ? 'FolderAdd' : item.kind === 'file' ? attachmentIcon(item.path) : item.kind === 'skill' ? 'Task' : item.kind === 'mcp' ? 'Cloud' : 'Computer'} size={14} /></span><span className="agent-mention-copy"><strong>{item.label}</strong><small>{item.detail}</small></span>{item.kind !== 'directory' && <Icon name="Add" size={13} />}
              </button>)}
              {!mentionItems.length && (!projectRoot && !/^(skills?|mcp|app):/i.test(mention.query) ? <button type="button" className="agent-mention-empty" onMouseDown={event => event.preventDefault()} onClick={() => { setMention(null); chooseFolder(); }}>Choose a project folder to browse files</button> : <span className="agent-mention-empty">{! /^(skills?|mcp|app):/i.test(mention.query) && projectEntries.error || 'No matching context'}</span>)}
              {mentionItems.length > 0 && (projectEntries.hasMore && !/^(skills?|mcp|app):/i.test(mention.query)) && <small className="agent-mention-limit">Showing the first 100 items. Type to narrow the list.</small>}
            </div>
          </div>}
          <textarea ref={composerInputRef} rows={session ? 1 : 2} aria-label={session ? 'Follow up with agent' : 'Describe agent task'} placeholder={session ? 'What’s next? Use @ to add context.' : 'Describe your task. Use @ to add context.'} value={drafts[provider] || ''} onChange={event => { changeDraft(event.target.value); updateMention(event.target.value, event.target.selectionStart); }} onClick={event => updateMention(event.currentTarget.value, event.currentTarget.selectionStart)} onKeyDown={event => {
            if (event.nativeEvent.isComposing) return;
            if (mention && mentionItems.length && ['ArrowDown', 'ArrowUp'].includes(event.key)) { event.preventDefault(); setMentionIndex(index => (index + (event.key === 'ArrowDown' ? 1 : mentionItems.length - 1)) % mentionItems.length); return; }
            if (mention && event.key === 'Enter') { event.preventDefault(); if (mentionItems.length) chooseMention(mentionItems[mentionIndex] || mentionItems[0]); return; }
            if (mention && event.key === 'Escape') { event.preventDefault(); setMention(null); return; }
            if (event.key === 'Enter' && !event.shiftKey && !event.nativeEvent.isComposing) { event.preventDefault(); submit(event); }
          }} maxLength={100000} />
          {!!files[provider]?.length && <div className="agent-files" aria-label="Attached files">{files[provider].map(path => <span className="agent-file-chip" key={path} title={path}><span className="agent-file-kind"><Icon name={attachmentIcon(path)} size={14} /></span><span className="agent-file-name">{path.split(/[\\/]/).at(-1)}</span><button type="button" className="agent-file-remove" aria-label={`Remove ${path}`} onClick={() => setFiles(current => ({ ...current, [provider]: current[provider].filter(file => file !== path) }))}><Icon name="Cancel" size={12} /></button></span>)}</div>}
          <div className="agent-composer-footer"><button type="button" className="agent-attach" onClick={attach} title="Attach files" aria-label="Attach files"><Icon name="Add" size={18} /></button><button type="button" className="agent-config-chip" onClick={() => { setOptionId(modelOption?.id || ''); setView('options'); }} title="Choose model and task options"><span>{modelValue || 'Model & options'}</span></button><span className="agent-composer-spacer" /><button type="button" className="agent-dictate" onClick={() => { setView('task'); setAnswerExpanded(false); onDictate(); }} title="Dictate into task" aria-label="Dictate into task"><Icon name="Mic" size={17} /></button><button type="submit" className="agent-submit" disabled={sending[provider] || agent.installing || (session && (!session.ready || session.active)) || (!drafts[provider]?.trim() && !files[provider]?.length)} aria-label={sending[provider] ? 'Sending task' : session ? 'Send follow-up' : 'Start task'}><Icon name={sending[provider] ? 'Loading' : 'AgentSend'} size={17} /></button></div>
        </form>}</div>}
        {agent.error && <AgentFeedback error={agent.error} operationKind={agent.operationKind} authRequired={agent.authRequired} authMethods={agent.authMethods} provider={provider} send={send} authenticating={agent.authenticating} Icon={Icon} />}
      </div>}
      {!!remainingPermissions.length && <div className="agent-other-approvals"><strong>Other sessions need approval</strong>{remainingPermissions.map(permission => <ApprovalCard key={permission.id} permission={permission} provider={provider} send={send} Icon={Icon} compact />)}</div>}
      {!!remainingQuestions.length && <div className="agent-other-approvals"><strong>Other sessions have a question</strong>{remainingQuestions.map(question => <QuestionCard key={question.id} question={question} provider={provider} send={send} Icon={Icon} />)}</div>}
    </>}
  </section>;
}
