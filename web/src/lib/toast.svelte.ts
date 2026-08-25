export type ToastType = 'success' | 'error' | 'info' | 'warning';

export interface ToastMessage {
  id: number;
  type: ToastType;
  message: string;
  duration?: number;
}

let nextId = 1;
const toastList = $state<ToastMessage[]>([]);

export const toasts = {
  get items() {
    return toastList;
  },
  show(message: string, type: ToastType = 'info', duration = 3000) {
    const id = nextId++;
    toastList.push({ id, type, message, duration });
    if (duration > 0) {
      setTimeout(() => {
        toasts.dismiss(id);
      }, duration);
    }
    return id;
  },
  success(message: string, duration = 3000) {
    return toasts.show(message, 'success', duration);
  },
  error(message: string, duration = 4500) {
    return toasts.show(message, 'error', duration);
  },
  info(message: string, duration = 3000) {
    return toasts.show(message, 'info', duration);
  },
  warning(message: string, duration = 3500) {
    return toasts.show(message, 'warning', duration);
  },
  dismiss(id: number) {
    const index = toastList.findIndex((t) => t.id === id);
    if (index !== -1) {
      toastList.splice(index, 1);
    }
  },
};
