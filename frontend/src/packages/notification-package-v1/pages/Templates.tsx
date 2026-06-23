import { useState } from 'react';
import { useData, useMutateData } from '@/lib/api';
import { useQueryClient } from '@tanstack/react-query';
import { Button } from '@/components/ui/button';
import { useToast } from '@/components/ui/use-toast';

interface Template {
  id: string;
  name: string;
  subject: string;
  body: string;
}

export default function Templates() {
  const { toast } = useToast();
  const queryClient = useQueryClient();
  const { data, isLoading } = useData('/api/operations/templates');
  const createMutation = useMutateData('/api/operations/templates', 'Template created successfully');

  const [showModal, setShowModal] = useState(false);
  const [formData, setFormData] = useState({ name: '', subject: '', body: '' });

  const templates = data ? (Object.values(data) as Template[]) : [];

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    createMutation.mutate({
      id: `tmpl_${Date.now()}`,
      ...formData
    });
    setShowModal(false);
    setFormData({ name: '', subject: '', body: '' });
  };

  const handleDelete = async (id: string) => {
    try {
      const res = await fetch(`/api/operations/templates/${id}`, { method: 'DELETE' });
      if (res.status === 401 || res.status === 403) {
        localStorage.removeItem('auth');
        window.location.href = '/login';
        return;
      }
      if (!res.ok) throw new Error('Delete failed');
      toast({ title: 'Success', description: 'Template deleted' });
      queryClient.invalidateQueries();
    } catch (err) {
      toast({ title: 'Error', description: 'Failed to delete template', variant: 'destructive' });
    }
  };

  return (
    <div className="space-y-6">
      <div className="flex justify-between items-center">
        <div>
          <h1 className="text-3xl font-bold tracking-tight">Notification Templates</h1>
          <p className="text-muted-foreground text-sm">Create and manage pre-defined message templates.</p>
        </div>
        <Button onClick={() => setShowModal(true)}>New Template</Button>
      </div>

      {isLoading ? (
        <div className="p-8 text-center text-muted-foreground">Loading templates...</div>
      ) : (
        <div className="grid gap-4 md:grid-cols-2 lg:grid-cols-3">
          {templates.map((tmpl: any) => (
            <div key={tmpl.id} className="border rounded-md bg-card p-4 space-y-3 flex flex-col justify-between">
              <div className="space-y-2">
                <h3 className="font-bold text-sm">{tmpl.name}</h3>
                <p className="text-xs text-muted-foreground"><strong>Subject:</strong> {tmpl.subject}</p>
                <p className="text-xs text-muted-foreground p-2 rounded border bg-slate-50 font-mono whitespace-pre-wrap">{tmpl.body}</p>
              </div>
              <div className="flex justify-end pt-2 border-t mt-2">
                <Button variant="destructive" size="sm" onClick={() => handleDelete(tmpl.id)}>
                  Delete
                </Button>
              </div>
            </div>
          ))}
          {templates.length === 0 && (
            <div className="col-span-full p-8 text-center text-muted-foreground border border-dashed rounded-lg">
              No templates created yet. Click "New Template" to get started.
            </div>
          )}
        </div>
      )}

      {showModal && (
        <div className="fixed inset-0 bg-background/80 flex items-center justify-center z-50">
          <form className="bg-card p-6 rounded-lg w-[400px] border shadow-xl space-y-4" onSubmit={handleSubmit}>
            <h2 className="text-xl font-bold">Create Template</h2>
            <div className="space-y-2">
              <label className="text-sm font-medium">Template Name</label>
              <input required className="w-full p-2 border rounded-md bg-background text-sm" value={formData.name} onChange={e => setFormData({...formData, name: e.target.value})} />
            </div>
            <div className="space-y-2">
              <label className="text-sm font-medium">Subject</label>
              <input required className="w-full p-2 border rounded-md bg-background text-sm" value={formData.subject} onChange={e => setFormData({...formData, subject: e.target.value})} />
            </div>
            <div className="space-y-2">
              <label className="text-sm font-medium">Body</label>
              <textarea required rows={4} className="w-full p-2 border rounded-md bg-background text-sm" value={formData.body} onChange={e => setFormData({...formData, body: e.target.value})} />
            </div>
            <div className="flex justify-end gap-2 pt-2">
              <Button type="button" variant="outline" onClick={() => setShowModal(false)}>Cancel</Button>
              <Button type="submit" disabled={createMutation.isPending}>
                {createMutation.isPending ? 'Saving...' : 'Save Template'}
              </Button>
            </div>
          </form>
        </div>
      )}
    </div>
  );
}
