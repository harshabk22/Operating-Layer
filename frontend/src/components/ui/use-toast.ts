import { useState, useEffect } from 'react';
let memoryState: any[] = [];
let listeners: any[] = [];
export function useToast() {
  const [toasts, setToasts] = useState<any[]>(memoryState);
  useEffect(() => {
    listeners.push(setToasts);
    return () => { listeners = listeners.filter(l => l !== setToasts); };
  }, []);
  return {
    toast: (t: any) => {
      memoryState = [...memoryState, { id: Date.now(), ...t }];
      listeners.forEach(l => l(memoryState));
      setTimeout(() => {
        memoryState = memoryState.filter(x => x.id !== t.id);
        listeners.forEach(l => l(memoryState));
      }, 3000);
    },
    toasts
  };
}
