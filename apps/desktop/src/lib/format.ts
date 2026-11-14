/** Plain-language formatting: timecodes, relative times, sizes. */

export function timecode(seconds: number, tenths = false): string {
  const s = Math.max(0, seconds);
  if (tenths) {
    const total = Math.round(s * 10);
    const whole = Math.floor(total / 10);
    return `${Math.floor(whole / 60)}:${String(whole % 60).padStart(2, '0')}.${total % 10}`;
  }
  const whole = Math.round(s);
  const h = Math.floor(whole / 3600);
  const m = Math.floor((whole % 3600) / 60);
  const sec = String(whole % 60).padStart(2, '0');
  return h > 0 ? `${h}:${String(m).padStart(2, '0')}:${sec}` : `${m}:${sec}`;
}

export function relativeTime(date: Date, now = new Date()): string {
  const diff = (now.getTime() - date.getTime()) / 1000;
  if (diff < 45) return 'just now';
  if (diff < 3600) return `${Math.round(diff / 60)} min ago`;
  if (diff < 86400) {
    const h = Math.round(diff / 3600);
    return `${h} h ago`;
  }
  const days = Math.floor(diff / 86400);
  if (days === 1) return 'yesterday';
  if (days < 7) return `${days} days ago`;
  if (days < 14) return 'last week';
  return date.toLocaleDateString(undefined, { day: 'numeric', month: 'short' });
}

export function fileSize(bytes: number): string {
  if (bytes < 1024 * 1024) return `${Math.max(1, Math.round(bytes / 1024))} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${Math.round(bytes / (1024 * 1024))} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}

export function plural(n: number, one: string, many = `${one}s`): string {
  return `${n} ${n === 1 ? one : many}`;
}
