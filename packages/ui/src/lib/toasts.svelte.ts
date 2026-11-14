/**
 * Toasts are for results that arrive later (an export finished, an agent take is ready) and for Undo. Changes the
 * person just made show inline instead ("Saved ✓").
 */

export type ToastTone = 'neutral' | 'success' | 'danger';

export interface Toast {
  id: number;
  message: string;
  tone: ToastTone;
  action?: { label: string; run: () => void };
  /** Milliseconds before it leaves on its own; 0 keeps it until dismissed. */
  timeout: number;
}

let nextId = 1;
export const toasts = $state<Toast[]>([]);
const timers = new Map<number, ReturnType<typeof setTimeout>>();

export function toast(message: string, options: Partial<Omit<Toast, 'id' | 'message'>> = {}): number {
  const id = nextId++;
  const item: Toast = {
    id,
    message,
    tone: options.tone ?? 'neutral',
    action: options.action,
    timeout: options.timeout ?? 4500,
  };
  toasts.push(item);
  if (toasts.length > 3) dismiss(toasts[0].id);
  if (item.timeout > 0)
    timers.set(
      id,
      setTimeout(() => dismiss(id), item.timeout),
    );
  return id;
}

export function dismiss(id: number): void {
  const index = toasts.findIndex((t) => t.id === id);
  if (index >= 0) toasts.splice(index, 1);
  const timer = timers.get(id);
  if (timer) clearTimeout(timer);
  timers.delete(id);
}

/** Pauses auto-dismiss while the pointer is over the toasts, so nobody has to race a timer. */
export function holdToasts(hold: boolean): void {
  for (const t of toasts) {
    const timer = timers.get(t.id);
    if (hold && timer) {
      clearTimeout(timer);
      timers.delete(t.id);
    } else if (!hold && !timer && t.timeout > 0) {
      timers.set(
        t.id,
        setTimeout(() => dismiss(t.id), 2500),
      );
    }
  }
}
