// Parse locally and preview the exact result before Enter creates the alarm.
export function parseReminderSentence(value, now = new Date()) {
  let text = String(value || '').trim().replace(/\s+/g, ' ').replace(/[.!?]+$/, '');
  if (!text) return null;
  let at;
  const relative = text.match(/\b(?:in|after)\s+(\d{1,3})\s*(minutes?|mins?|hours?|hrs?|days?)\b/i);
  if (relative) {
    const amount = Number(relative[1]);
    if (!amount || amount > 365) return null;
    const unit = relative[2].toLowerCase();
    const millis = amount * (unit.startsWith('day') ? 86400000 : unit.startsWith('h') ? 3600000 : 60000);
    at = new Date(now.getTime() + millis);
    text = text.replace(relative[0], ' ');
  } else {
    const time = text.match(/\b(?:at\s*)?(\d{1,2})(?::([0-5]\d))?\s*(a\.?m\.?|p\.?m\.?)\b|\bat\s+(\d{1,2})(?::([0-5]\d))?\b|\b([01]?\d|2[0-3]):([0-5]\d)\b/i);
    const namedTime = text.match(/\b(?:at\s+)?(noon|midnight)\b/i);
    if (!time && !namedTime) return null;
    const match = time || namedTime;
    const meridiem = time?.[3]?.toLowerCase().replaceAll('.', '');
    let hour = namedTime ? (namedTime[1].toLowerCase() === 'noon' ? 12 : 0) : Number(time[1] || time[4] || time[6]);
    const minute = namedTime ? 0 : Number(time[2] || time[5] || time[7] || 0);
    if ((meridiem && (hour < 1 || hour > 12)) || (!meridiem && hour > 23)) return null;
    if (meridiem) hour = hour % 12 + (meridiem.startsWith('p') ? 12 : 0);
    text = text.replace(match[0], ' ');
    at = new Date(now);
    const day = text.match(/\b(today|tomorrow|(?:next\s+)?(?:monday|tuesday|wednesday|thursday|friday|saturday|sunday))\b/i);
    const date = day ? null : text.match(/\b(?:on\s+)?(?:(\d{1,2})(?:st|nd|rd|th)?\s+(jan(?:uary)?|feb(?:ruary)?|mar(?:ch)?|apr(?:il)?|may|jun(?:e)?|jul(?:y)?|aug(?:ust)?|sep(?:tember)?|oct(?:ober)?|nov(?:ember)?|dec(?:ember)?)|(jan(?:uary)?|feb(?:ruary)?|mar(?:ch)?|apr(?:il)?|may|jun(?:e)?|jul(?:y)?|aug(?:ust)?|sep(?:tember)?|oct(?:ober)?|nov(?:ember)?|dec(?:ember)?)\s+(\d{1,2})(?:st|nd|rd|th)?)\b/i);
    if (day) {
      const label = day[1].toLowerCase();
      if (label === 'tomorrow') at.setDate(at.getDate() + 1);
      else if (label !== 'today') {
        const weekday = ['sunday','monday','tuesday','wednesday','thursday','friday','saturday'].indexOf(label.replace(/^next\s+/, ''));
        let offset = (weekday - at.getDay() + 7) % 7;
        if (offset === 0 || label.startsWith('next ')) offset += 7;
        at.setDate(at.getDate() + offset);
      }
      text = text.replace(day[0], ' ');
    } else if (date) {
      const months = ['jan','feb','mar','apr','may','jun','jul','aug','sep','oct','nov','dec'];
      const month = months.indexOf((date[2] || date[3]).slice(0, 3).toLowerCase());
      const dateNumber = Number(date[1] || date[4]);
      const year = now.getFullYear();
      at = new Date(year, month, dateNumber);
      if (at.getMonth() !== month || at.getDate() !== dateNumber) return null;
      text = text.replace(date[0], ' ');
    }
    at.setHours(hour, minute, 0, 0);
    if (!day && !date && at.getTime() <= now.getTime()) at.setDate(at.getDate() + 1);
    if (date && at.getTime() <= now.getTime()) at.setFullYear(at.getFullYear() + 1);
  }
  const reminderText = text.replace(/\s+/g, ' ').trim()
    .replace(/^(?:please\s+)?(?:remind\s+me(?:\s+to)?|reminder(?:\s+to)?|to)\s+/i, '')
    .replace(/^(?:on|at)\s+/i, '').replace(/\s+(?:on|at)$/i, '')
    .replace(/^[,:\s-]+|[,:\s-]+$/g, '');
  if (!reminderText || new TextEncoder().encode(reminderText).length > 200 || at.getTime() <= now.getTime()) return null;
  return { text: reminderText, at: Math.floor(at.getTime() / 1000) };
}

// Notes are free-form. Only offer a reminder for an explicit reminder line.
export function findReminderInNote(note, now = new Date()) {
  for (const line of String(note || '').split(/\r?\n/)) {
    const sentence = line.trim().replace(/^[-*•]\s*/, '');
    if (!/^(?:please\s+)?(?:remind\s+me\b|reminder\b)/i.test(sentence)) continue;
    const reminder = parseReminderSentence(sentence, now);
    if (reminder) return reminder;
  }
  return null;
}
