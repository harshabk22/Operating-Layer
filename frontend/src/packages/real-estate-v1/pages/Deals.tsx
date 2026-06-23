import { useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { useData, useMutateData } from '@/lib/api';
import { Button } from '@/components/ui/button';
import { useToast } from '@/components/ui/use-toast';

export default function Deals() {
  const queryClient = useQueryClient();
  const { toast } = useToast();
  
  const handleAction = async (endpoint: string, method: string = 'POST', body?: any) => {
    try {
      const res = await fetch(endpoint, { 
        method, 
        headers: body ? { 'Content-Type': 'application/json' } : undefined,
        body: body ? JSON.stringify(body) : undefined 
      });
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

  const { data, isLoading } = useData('/api/realestate/deals');
  const createMutation = useMutateData('/api/realestate/deals', 'Deals created successfully');
  const [showModal, setShowModal] = useState(false);
  const [formData, setFormData] = useState({});

  return (
    <div className="space-y-6">
      <div className="flex justify-between items-center">
        <div>
          <h1 className="text-3xl font-bold tracking-tight">Deals</h1>
          <p className="text-muted-foreground text-sm">Finalize and close property transaction deals.</p>
        </div>
        <Button onClick={() => setShowModal(true)}>New Deal</Button>
      </div>

      <div className="border rounded-md bg-card">
        {isLoading ? <div className="p-8 text-center text-muted-foreground">Loading...</div> : (
          !data || Object.keys(data).length === 0 ? <div className="p-8 text-center text-muted-foreground">No records found</div> : (
            <div className="p-4 space-y-4">
              {Object.values(data).map((item: any, i: number) => (
                <div key={i} className="flex justify-between items-center p-4 border rounded-lg hover:bg-accent/50 transition-colors">
                  <div>
                    <p className="font-medium text-sm">{item.id || item.name || item.title || JSON.stringify(item)}</p>
                    {item.value && <p className="text-xs text-muted-foreground">Value: ${item.value}</p>}
                  </div>
                  <div className="flex gap-2">
                    <Button variant="outline" size="sm" onClick={() => handleAction(`/api/realestate/deals/${item.id}`, 'PATCH', { status: 'finalised' })}>Finalise</Button>
                    <Button variant="destructive" size="sm" onClick={() => handleAction(`/api/realestate/deals/${item.id}`, 'DELETE')}>Delete</Button>
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
            createMutation.mutate({ ...formData, id: `deals_${Date.now()}` });
            setShowModal(false);
          }}>
            <h2 className="text-xl font-bold mb-4">Create Deal</h2>
            <div className="space-y-4">
              <input required name="value" placeholder="value" className="w-full p-2 border rounded-md bg-background text-sm" onChange={e => setFormData({...formData, value: e.target.value})} />
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
