import test from 'node:test';
import assert from 'node:assert/strict';
import { findReminderInNote, parseReminderSentence } from '../src/reminder-sentence.js';

test('a reminder typed in the Notes editor becomes an Add suggestion', () => {
  const now = new Date(2026, 9, 27, 8, 30);
  const parsed = findReminderInNote('Some notes\nremind me to call sam tomorrow at 9am', now);
  assert.equal(parsed.text, 'call sam');
  assert.equal(new Date(parsed.at * 1000).getHours(), 9);
  assert.equal(findReminderInNote('Meeting tomorrow at 9am', now), null);
});

test('quick reminder requires an explicit future day and time', () => {
  const now = new Date(2026, 9, 27, 8, 30);
  const result = parseReminderSentence('Remind me to call Sam tomorrow at 9:15 am', now);
  assert.equal(result.text, 'call Sam');
  assert.deepEqual(Object.keys(result).sort(), ['at', 'text']);
  assert.equal(new Date(result.at * 1000).getDate(), 28);
  assert.equal(new Date(result.at * 1000).getHours(), 9);
  assert.equal(new Date(result.at * 1000).getMinutes(), 15);
  assert.equal(parseReminderSentence('today at 8:00 call Sam', now), null);
  assert.equal(parseReminderSentence('call Sam', now), null);
  assert.equal(parseReminderSentence('call Sam tomorrow at 25:00', now), null);
});

test('quick reminder accepts the time before the task', () => {
  const result = parseReminderSentence('tomorrow at 17:45 to send invoice', new Date(2026, 9, 27, 12));
  assert.equal(result.text, 'send invoice');
  assert.equal(new Date(result.at * 1000).getHours(), 17);
});

test('quick reminder handles natural word order, weekdays, and relative times', () => {
  const now = new Date(2026, 9, 27, 8, 30);
  const cases = [
    ['Call Sam at 5pm tomorrow', 'Call Sam', 28, 17, 0],
    ['Remind me in 30 minutes to call Sam', 'call Sam', 27, 9, 0],
    ['Send the report on Friday at noon', 'Send the report', 30, 12, 0],
    ['Tomorrow at 9 to go to the doctor', 'go to the doctor', 28, 9, 0],
    ['Pay rent on 29 Oct at 6:30pm', 'Pay rent', 29, 18, 30],
  ];
  for (const [sentence, task, date, hour, minute] of cases) {
    const parsed = parseReminderSentence(sentence, now);
    assert.ok(parsed, sentence);
    assert.equal(parsed.text, task, sentence);
    const at = new Date(parsed.at * 1000);
    assert.equal(at.getDate(), date, sentence);
    assert.equal(at.getHours(), hour, sentence);
    assert.equal(at.getMinutes(), minute, sentence);
  }
  assert.equal(parseReminderSentence('Remind me to call Sam sometime tomorrow', now), null);
});
