import { useState } from 'react';
import { useData, useMutateData } from '@/lib/api';
import { useQueryClient } from '@tanstack/react-query';
import { Button } from '@/components/ui/button';
import { useToast } from '@/components/ui/use-toast';

export default function Leads() {
  const { toast } = useToast();
  const { data, isLoading } = useData('/api/sales/leads');
  const createMutation = useMutateData('/api/sales/leads', 'Lead created successfully');
  const [showModal, setShowModal] = useState(false);
  const [formData, setFormData] = useState({ name: '', email: '' });
  const [search, setSearch] = useState('');

  const filteredData = data ? (Object.values(data) as any[]).filter(d => 
    (d.name?.toLowerCase() || '').includes(search.toLowerCase())
  ) : [];

  const queryClient = useQueryClient();
  const handleAction = async (endpoint: string, method: string = 'POST') => {
    try {
      const res = await fetch(endpoint, { method });
      if (res.status === 401 || res.status === 403) {
        localStorage.removeItem('auth');
        window.location.href = '/login';
        return;
      }
      if (!res.ok) throw new Error('Request failed');
      toast({ title: 'Success', description: 'Action completed successfully' });
      queryClient.invalidateQueries();
    } catch (err) {
      toast({ title: 'Error', description: 'Action failed', variant: 'destructive' });
    }
  };

  return (
    <div className="space-y-6">
      <div className="flex justify-between items-center">
        <div>
          <h1 className="text-3xl font-bold tracking-tight">Leads</h1>
          <p className="text-muted-foreground text-sm">Manage incoming customer leads and qualification status.</p>
        </div>
        <Button onClick={() => setShowModal(true)}>New Lead</Button>
      </div>

      <input 
        type="text" 
        placeholder="Search leads..." 
        className="w-full max-w-sm p-2 rounded-md border bg-background text-sm"
        value={search}
        onChange={e => setSearch(e.target.value)}
      />

      <div className="border rounded-md bg-card">
        {isLoading ? <div className="p-8 text-center text-muted-foreground">Loading...</div> : (
          filteredData.length === 0 ? <div className="p-8 text-center text-muted-foreground">No leads found</div> : (
            <div className="p-4 space-y-4">
              {filteredData.map((item: any, i: number) => (
                <div key={i} className="flex justify-between items-center p-4 border rounded-lg hover:bg-accent/50 transition-colors">
                  <div>
                    <p className="font-medium text-sm">{item.name || item.id}</p>
                    <p className="text-xs text-muted-foreground">{item.email || 'No email'}</p>
                  </div>
                  <div className="flex gap-2">
                    <Button variant="outline" size="sm" onClick={() => handleAction(`/api/sales/leads/${item.id}/qualify`)}>Qualify</Button>
                    <Button variant="destructive" size="sm" onClick={() => handleAction(`/api/sales/leads/${item.id}`, 'DELETE')}>Delete</Button>
                  </div>
                </div>
              ))}
            </div>
          )
        )}
      </div>

      {showModal && (
        <div className="fixed inset-0 bg-background/80 flex items-center justify-center z-50">
          <form className="bg-card p-6 rounded-lg w-[400px] border shadow-xl" onSubmit={e => {
            e.preventDefault();
            createMutation.mutate({ ...formData, id: `lead_${Date.now()}` });
            setShowModal(false);
          }}>
            <h2 className="text-xl font-bold mb-4">Create Lead</h2>
            <div className="space-y-4">
              <input required placeholder="Name" className="w-full p-2 border rounded-md bg-background text-sm" onChange={e => setFormData({...formData, name: e.target.value})} />
              <input required placeholder="Email" className="w-full p-2 border rounded-md bg-background text-sm" onChange={e => setFormData({...formData, email: e.target.value})} />
            </div>
            <div className="flex justify-end gap-2 mt-6">
              <Button type="button" variant="outline" onClick={() => setShowModal(false)}>Cancel</Button>
              <Button type="submit">Save</Button>
            </div>
          </form>
        </div>
      )}
    </div>
  );
}
