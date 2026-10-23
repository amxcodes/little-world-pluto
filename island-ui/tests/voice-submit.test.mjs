import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
const source = readFileSync(new URL('../src/agents-ui.jsx', import.meta.url), 'utf8');
const instruction = source.slice(source.indexOf('export const defaultAskInstructions'), source.indexOf('\n', source.indexOf('export const defaultAskInstructions'))).replace('export ', '');
const body = instruction + '\n' + source.slice(source.indexOf('export function AgentsWorkspace('), source.indexOf('  const voiceThinking =')).replace('export function', 'function') + '\nreturn { pendingVoice, voiceAnswerRevision }; }';
function harness(agent, customStyle) {
  const slots = [], deps = [], calls = [], listeners = new Map();
  let index = 0, effects = [], dirty = false;
  const prefs = { defaultProvider:'opencode', providerSetupComplete:true, provider:{ opencode:{}, codex:{} } };
  if (customStyle !== undefined) prefs.askInstructions = customStyle;
  const memory = { drafts:{opencode:'',codex:''},files:{opencode:[],codex:[]},view:'task',query:'',pendingRevision:{},submitted:{},provider:'' };
  const context = {
    window:{addEventListener:(n,f)=>listeners.set(n,f),removeEventListener(){}},
    localStorage:{getItem:()=> 'Context'}, requestAnimationFrame(){},
    setTimeout, clearTimeout, console,
    useAgentPreferences:()=>[prefs,()=>{}],readAgentPreferences:()=>prefs,saveAgentPreferences(){},
    workspaceMemory:memory, cleanOptions:()=>[],optionChoices:()=>[],agentStepState:s=>s,
    sendAgent:(send,provider,action,data={})=>send('agentAction',{provider,action,...data}),
    useState(value) { const i=index++; if (!(i in slots)) slots[i]=typeof value==='function'?value():value; return [slots[i],next=>{const v=typeof next==='function'?next(slots[i]):next; if(!Object.is(v,slots[i])){slots[i]=v;dirty=true;}}]; },
    useRef(value) { const i=index++; return slots[i] ||= {current:value}; },
    useMemo(fn) { index++; return fn(); },
    useEffect(fn, values) { const i=index++; if (!deps[i] || values.some((v,j)=>!Object.is(v,deps[i][j]))) {deps[i]=values;effects.push(fn);} },
  };
  vm.createContext(context); vm.runInContext(body,context);
  const props={state:{agents:{opencode:agent}},send:(kind,data)=>calls.push({kind,...data}),dictatedText:'Explain this',dictatedAutoSubmit:true,askMode:true,onDictatedTextConsumed(){props.dictatedText='';props.dictatedAutoSubmit=false;dirty=true;}};
  let rendered;
  const flush=()=>{for(let n=0;n<20;n++){index=0;effects=[];dirty=false;rendered=context.AgentsWorkspace(props);for(const effect of effects)effect();if(!dirty)return;}throw new Error('render loop');};
  return {props,calls,flush,memory,listeners,rendered:()=>rendered};
}
test('Ask submits a transcript once to a new session',()=>{
  const h=harness({connected:true,selectedId:'',sessions:[]});h.flush();
  const starts=h.calls.filter(c=>c.action==='start');assert.equal(starts.length,1);assert.equal(starts[0].text,'Explain this');assert.match(starts[0].systemPrompt,/Context/);
});
test('Ask loads a saved session before delivering the transcript',()=>{
  const session={id:'saved',ready:false,active:false};
  const h=harness({connected:true,selectedId:'saved',sessions:[session]});h.flush();
  assert.equal(h.calls.filter(c=>c.action==='select').length,1);
  assert.equal(h.calls.filter(c=>c.action==='prompt').length,0);
  session.ready=true;h.flush();
  assert.equal(h.calls.filter(c=>c.action==='prompt').length,1);
});
test('Ask keeps its transcript while the provider connects',()=>{
  const agent={connected:false,selectedId:'',sessions:[]};const h=harness(agent);h.flush();
  assert.equal(h.calls.filter(c=>c.action==='start').length,0);
  agent.connected=true;h.flush();assert.equal(h.calls.filter(c=>c.action==='start').length,1);
});

test('Ask uses configurable spoken style and separate conversation context',()=>{
  const h=harness({connected:true,selectedId:'',sessions:[]},'Use three natural sentences. No em dashes.');h.flush();
  const prompt=h.calls.find(c=>c.action==='start').systemPrompt;
  assert.match(prompt,/Use three natural sentences/);assert.match(prompt,/Context supplied/);
});

test('wake questions leave the coding session and use the text-only voice mode',()=>{
  const agent={connected:true,selectedId:'coding',sessions:[{id:'coding',ready:true,active:false}]};
  const h=harness(agent);
  h.props.askMode=false;h.props.voiceSurface=true;h.props.dictatedText='';
  h.props.state.wakeWord={revision:1,status:'triggered'};h.flush();
  h.props.dictatedText='Is the browser running?';h.props.dictatedAutoSubmit=true;h.flush();
  assert.equal(h.calls.filter(c=>c.action==='prompt').length,0);
  assert.ok(h.calls.some(c=>c.action==='select' && c.sessionId===''));
  agent.selectedId='';h.flush();
  const start=h.calls.find(c=>c.action==='start');
  assert.ok(start);assert.match(start.systemPrompt,/^Island response mode: wake-answer/);
  assert.equal(start.text,'Is the browser running?');
});

test('a dismissed wake request cannot resume a later capture',()=>{
  const agent={connected:true,selectedId:'',sessions:[],operationRevision:0};
  const h=harness(agent);
  h.props.askMode=false;h.props.voiceSurface=true;h.props.dictatedText='';
  h.props.state.wakeWord={revision:1,status:'triggered'};h.flush();
  h.props.dictatedText='First question';h.props.dictatedAutoSubmit=true;h.flush();
  assert.equal(h.memory.wakeAwaitingAnswer.revision,1);
  h.listeners.get('island:voice-dismiss')();h.flush();
  h.props.state.wakeWord={revision:2,status:'triggered'};h.props.dictateActive=true;h.flush();
  agent.operationRevision=2;agent.operationKind='start';agent.operationOk=true;
  agent.selectedId='voice';agent.sessions=[{id:'voice',ready:true,active:false}];
  agent.response={sessionId:'voice',text:'Old answer',waiting:false};h.flush();
  assert.equal(h.calls.filter(c=>c.kind==='wakeWordResume').length,0);
  assert.equal(h.memory.wakeAwaitingAnswer,null);
});

test('answer reveal identity changes per question, not per streamed chunk',()=>{
  const session={id:'ask',ready:true,active:false};
  const agent={connected:true,selectedId:'ask',sessions:[session],operationRevision:0};
  const h=harness(agent);h.flush();
  const first=h.rendered().voiceAnswerRevision;
  agent.operationRevision=1;agent.operationKind='prompt';agent.operationOk=true;
  h.props.state.agents={opencode:agent};
  session.active=true;agent.response={sessionId:'ask',text:'A',waiting:false};h.flush();
  agent.response={...agent.response,text:'A longer answer'};h.flush();
  assert.equal(h.rendered().voiceAnswerRevision,first);
  session.active=false;h.flush();
  h.props.dictatedText='Next question';h.props.dictatedAutoSubmit=true;h.flush();
  assert.equal(h.rendered().voiceAnswerRevision,first+1);
});

test('the current wake request rearms once after its accepted response finishes',()=>{
  const agent={connected:true,selectedId:'',sessions:[],operationRevision:0};
  const h=harness(agent);
  h.props.askMode=false;h.props.voiceSurface=true;h.props.dictatedText='';
  h.props.state.wakeWord={revision:5,status:'triggered'};h.flush();
  h.props.dictatedText='Current question';h.props.dictatedAutoSubmit=true;h.flush();
  assert.equal(h.calls.filter(c=>c.kind==='wakeWordResume').length,0);
  agent.operationRevision=1;agent.operationKind='start';agent.operationOk=true;
  agent.selectedId='voice';agent.sessions=[{id:'voice',ready:true,active:true}];
  agent.response={sessionId:'voice',text:'',waiting:true};h.props.state.agents={opencode:agent};h.flush();
  assert.equal(h.calls.filter(c=>c.kind==='wakeWordResume').length,0);
  agent.operationRevision=2;agent.operationKind='prompt';agent.notice='Task finished';
  agent.sessions[0].active=false;agent.response={sessionId:'voice',text:'Finished answer',waiting:false};
  h.props.state.agents={opencode:agent};h.flush();h.flush();
  const resumes=h.calls.filter(c=>c.kind==='wakeWordResume');
  assert.equal(resumes.length,1);assert.equal(resumes[0].revision,5);
});
