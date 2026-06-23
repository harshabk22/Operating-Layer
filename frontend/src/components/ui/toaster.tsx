import { useToast } from './use-toast';
export function Toaster() {
  const { toasts } = useToast();
  return (
    <div className="fixed bottom-0 right-0 z-[100] flex max-h-screen w-full flex-col-reverse p-4 sm:bottom-0 sm:right-0 sm:top-auto sm:flex-col md:max-w-[420px]">
      {toasts.map((t: any) => (
        <div key={t.id} className="mb-2 p-4 border rounded-lg shadow-lg bg-card text-foreground">
          <h3 className="font-semibold">{t.title}</h3>
          {t.description && <p className="text-sm text-muted-foreground">{t.description}</p>}
        </div>
      ))}
    </div>
  );
}
