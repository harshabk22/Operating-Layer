import { useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { useData, useMutateData } from '@/lib/api';
import { Button } from '@/components/ui/button';
import { useToast } from '@/components/ui/use-toast';

interface Property {
  id: string;
  title?: string;
  name?: string;
  address?: string;
}

export default function Properties() {
  const queryClient = useQueryClient();
  const { toast } = useToast();
  
  const handleAction = async (endpoint: string, method: string = 'POST', body?: unknown) => {
    try {
      const res = await fetch(endpoint, { 
        method, 
        headers: body ? { 'Content-Type': 'application/json' } : undefined,
        body: body ? JSON.stringify(body) : undefined 
      });
      if (res.status === 401 || res.status === 403) {
        localStorage.removeItem('auth');
        window.location.assign('/login');
        return;
      }
      if (!res.ok) throw new Error('Request failed');
      toast({ title: 'Success', description: 'Action completed successfully' });
      queryClient.invalidateQueries();
    } catch {
      toast({ title: 'Error', description: 'Action failed', variant: 'destructive' });
    }
  };

  const { data, isLoading } = useData('/api/realestate/properties');
  const createMutation = useMutateData('/api/realestate/properties', 'Property created successfully');
  const [showModal, setShowModal] = useState(false);
  const [formData, setFormData] = useState({ title: '', address: '' });
  const [selectedProperty, setSelectedProperty] = useState<Property | null>(null);

  return (
    <div className="space-y-6">
      <div className="flex justify-between items-center">
        <div>
          <h1 className="text-3xl font-bold tracking-tight">Properties</h1>
          <p className="text-muted-foreground text-sm">Manage listings, addresses, and physical property inventory.</p>
        </div>
        <Button onClick={() => setShowModal(true)}>New Property</Button>
      </div>

      <div className="border rounded-md bg-card">
        {isLoading ? <div className="p-8 text-center text-muted-foreground">Loading...</div> : (
          !data || Object.keys(data).length === 0 ? <div className="p-8 text-center text-muted-foreground">No properties found</div> : (
            <div className="p-4 space-y-4">
              {(Object.values(data) as Property[]).map((item, i) => (
                <div key={i} className="flex justify-between items-center p-4 border rounded-lg hover:bg-accent/50 transition-colors">
                  <div>
                    <p className="font-medium text-sm">{item.title || item.name || item.id}</p>
                    <p className="text-xs text-muted-foreground">{item.address || 'No address'}</p>
                  </div>
                  <div className="flex gap-2">
                    <Button variant="outline" size="sm" onClick={() => setSelectedProperty(item)}>View</Button>
                    <Button variant="destructive" size="sm" onClick={() => handleAction(`/api/realestate/properties/${item.id}`, 'DELETE')}>Delete</Button>
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
            createMutation.mutate({ ...formData, id: `prop_${Date.now()}` });
            setShowModal(false);
          }}>
            <h2 className="text-xl font-bold mb-4">Create Property</h2>
            <div className="space-y-4">
              <input required name="title" placeholder="Title" className="w-full p-2 border rounded-md bg-background text-sm" onChange={e => setFormData({...formData, title: e.target.value})} />
              <input required name="address" placeholder="Address" className="w-full p-2 border rounded-md bg-background text-sm" onChange={e => setFormData({...formData, address: e.target.value})} />
            </div>
            <div className="flex justify-end gap-2 mt-6">
              <Button type="button" variant="outline" onClick={() => setShowModal(false)}>Cancel</Button>
              <Button type="submit">Save</Button>
            </div>
          </form>
        </div>
      )}

      {selectedProperty && (
        <div className="fixed inset-0 bg-background/80 flex items-center justify-center z-50">
          <div className="bg-card p-6 rounded-lg w-[400px] border shadow-xl">
            <h2 className="text-xl font-bold mb-2">Property Details</h2>
            <div className="space-y-2 py-4 text-sm">
              <p><strong>ID:</strong> {selectedProperty.id}</p>
              <p><strong>Title:</strong> {selectedProperty.title || selectedProperty.name}</p>
              <p><strong>Address:</strong> {selectedProperty.address || 'No address'}</p>
            </div>
            <div className="flex justify-end mt-4">
              <Button onClick={() => setSelectedProperty(null)}>Close</Button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
