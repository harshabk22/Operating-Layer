import { useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { useData, useMutateData } from '@/lib/api';
import { Button } from '@/components/ui/button';
import { useToast } from '@/components/ui/use-toast';

interface Property {
  id: string;
  title?: string;
  name?: string;
}

interface Offer {
  id: string;
  property_id?: string;
  property_title?: string;
  buyer_name?: string;
  status?: string;
  amount?: string | number;
}

export default function Offers() {
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

  const { data, isLoading } = useData('/api/realestate/offers');
  const { data: properties } = useData('/api/realestate/properties');
  const createMutation = useMutateData('/api/realestate/offers', 'Offer created successfully');
  const [showModal, setShowModal] = useState(false);
  const [formData, setFormData] = useState({ property_id: '', property_title: '', buyer_name: '', amount: '' });

  const handlePropertyChange = (e: React.ChangeEvent<HTMLSelectElement>) => {
    const propertyId = e.target.value;
    const propList = properties ? (Object.values(properties) as Property[]) : [];
    const selectedProp = propList.find((p) => p.id === propertyId);
    setFormData({
      ...formData,
      property_id: propertyId,
      property_title: selectedProp ? (selectedProp.title || selectedProp.name || selectedProp.id) : ''
    });
  };

  return (
    <div className="space-y-6">
      <div className="flex justify-between items-center">
        <div>
          <h1 className="text-3xl font-bold tracking-tight">Offers</h1>
          <p className="text-muted-foreground text-sm">Review, accept, or reject property offers from potential buyers.</p>
        </div>
        <Button onClick={() => {
          setFormData({ property_id: '', property_title: '', buyer_name: '', amount: '' });
          setShowModal(true);
        }}>New Offer</Button>
      </div>

      <div className="border rounded-md bg-card">
        {isLoading ? <div className="p-8 text-center text-muted-foreground">Loading...</div> : (
          !data || Object.keys(data).length === 0 ? <div className="p-8 text-center text-muted-foreground">No records found</div> : (
            <div className="p-4 space-y-4">
              {(Object.values(data) as Offer[]).map((item, i) => (
                <div key={i} className="flex justify-between items-center p-4 border rounded-lg hover:bg-accent/50 transition-colors">
                  <div>
                    <p className="font-medium text-sm">{item.property_title || item.property_id || item.id}</p>
                    <p className="text-xs text-muted-foreground">Buyer: {item.buyer_name || 'Anonymous'}</p>
                    <div className="flex items-center gap-2 mt-1">
                      <span className={`text-[10px] uppercase font-semibold px-2 py-0.5 rounded font-mono ${
                        item.status === 'accepted' ? 'bg-green-100 text-green-800 dark:bg-green-950 dark:text-green-300' :
                        item.status === 'rejected' ? 'bg-red-100 text-red-800 dark:bg-red-950 dark:text-red-300' :
                        'bg-yellow-100 text-yellow-800 dark:bg-yellow-950 dark:text-yellow-300'
                      }`}>
                        {item.status || 'pending'}
                      </span>
                      {item.amount && <span className="text-xs text-muted-foreground">Amount: ${item.amount}</span>}
                    </div>
                  </div>
                  <div className="flex gap-2">
                    <Button variant="outline" size="sm" onClick={() => handleAction(`/api/realestate/offers/${item.id}`, 'PATCH', { status: 'accepted' })}>Accept</Button>
                    <Button variant="outline" size="sm" onClick={() => handleAction(`/api/realestate/offers/${item.id}`, 'PATCH', { status: 'rejected' })}>Reject</Button>
                    <Button variant="destructive" size="sm" onClick={() => handleAction(`/api/realestate/offers/${item.id}`, 'DELETE')}>Delete</Button>
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
            createMutation.mutate({ ...formData, id: `offer_${Date.now()}` });
            setShowModal(false);
          }}>
            <h2 className="text-xl font-bold mb-4">Create Offer</h2>
            <div className="space-y-4">
              <div className="space-y-1">
                <label className="text-xs font-medium">Property</label>
                <select
                  required
                  className="w-full p-2 border rounded-md bg-background text-sm"
                  value={formData.property_id}
                  onChange={handlePropertyChange}
                >
                  <option value="">-- Select Property --</option>
                  {properties && (Object.values(properties) as Property[]).map((p) => (
                    <option key={p.id} value={p.id}>{p.title || p.name || p.id}</option>
                  ))}
                </select>
              </div>
              <div className="space-y-1">
                <label className="text-xs font-medium">Buyer Name</label>
                <input
                  required
                  placeholder="Buyer Name"
                  className="w-full p-2 border rounded-md bg-background text-sm"
                  value={formData.buyer_name}
                  onChange={e => setFormData({ ...formData, buyer_name: e.target.value })}
                />
              </div>
              <div className="space-y-1">
                <label className="text-xs font-medium">Offer Amount ($)</label>
                <input
                  required
                  type="number"
                  placeholder="Offer Amount"
                  className="w-full p-2 border rounded-md bg-background text-sm"
                  value={formData.amount}
                  onChange={e => setFormData({ ...formData, amount: e.target.value })}
                />
              </div>
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
