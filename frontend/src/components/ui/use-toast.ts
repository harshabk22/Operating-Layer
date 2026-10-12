import { useState, useEffect } from 'react';

export interface ToastProps {
  id?: number;
  title?: string;
  description?: string;
  variant?: 'default' | 'destructive';
}

type ToastListener = (toasts: ToastProps[]) => void;

let memoryState: ToastProps[] = [];
let listeners: ToastListener[] = [];

export function useToast() {
  const [toasts, setToasts] = useState<ToastProps[]>(memoryState);
  useEffect(() => {
    listeners.push(setToasts);
    return () => {
      listeners = listeners.filter((l) => l !== setToasts);
    };
  }, []);
  return {
    toast: (t: Omit<ToastProps, 'id'>) => {
      const id = Date.now();
      memoryState = [...memoryState, { id, ...t }];
      listeners.forEach((l) => l(memoryState));
      setTimeout(() => {
        memoryState = memoryState.filter((x) => x.id !== id);
        listeners.forEach((l) => l(memoryState));
      }, 3000);
    },
    toasts,
  };
}
