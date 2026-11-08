import React from 'react';
import { HugeiconsIcon } from '@hugeicons/react';
import { Mic01Icon } from '@hugeicons/core-free-icons';

export function AskActivity({ phase = 'processing', levels = [], label = 'Thinking…' }) {
  const energy = phase === 'recording' && levels.length ? levels.reduce((a,b)=>a+b,0) / levels.length : 0;
  return <div className="ask-capture-surface" data-capture-state={phase}>
    <div className="ask-voice-mark" aria-hidden="true" style={{ '--voice-energy': energy }}>
      <span className="ask-voice-ring" />
      <span className="ask-voice-mic"><HugeiconsIcon icon={Mic01Icon} size={23} strokeWidth={1.7} /></span>
      <span className="ask-voice-wave">{[0,4,8,12,16,20,24].map(index=><i key={index} style={{'--voice-level':phase === 'recording' ? levels[index] || 0 : 0}} />)}</span>
      <span className="ask-voice-processing"><i/><i/><i/></span>
    </div>
    <span className="ask-capture-status" key={label} role="status">{label}</span>
  </div>;
}
