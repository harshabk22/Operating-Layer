import { useToast, type ToastProps } from './use-toast';
export function Toaster() {
  const { toasts } = useToast();
  return (
    <div className="fixed bottom-0 right-0 z-[100] flex max-h-screen w-full flex-col-reverse p-4 sm:bottom-0 sm:right-0 sm:top-auto sm:flex-col md:max-w-[420px]">
      {toasts.map((t: ToastProps) => {
        const isDestructive = t.variant === 'destructive';
        return (
          <div 
            key={t.id} 
            className={`mb-2 p-4 border rounded-lg shadow-lg animate-fade-in ${
              isDestructive 
                ? 'bg-destructive/10 border-destructive/30 text-destructive' 
                : 'bg-card text-foreground border-border'
            }`}
          >
            <h3 className="font-semibold text-sm">{t.title}</h3>
            {t.description && (
              <p className={`text-sm mt-1 ${isDestructive ? 'text-destructive/80' : 'text-muted-foreground'}`}>
                {t.description}
              </p>
            )}
          </div>
        );
      })}
    </div>
  );
}
