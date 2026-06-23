import { useState } from 'react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { Button } from '@/components/ui/button';
import { useToast } from '@/components/ui/use-toast';

export default function Notifications() {
  const queryClient = useQueryClient();
  const { toast } = useToast();
  const [message, setMessage] = useState('');
  const [recipient, setRecipient] = useState('');

  const sendMutation = useMutation({
    mutationFn: async (payload: { id: string, message: string, recipient: string }) => {
      const res = await fetch('/api/operations/notifications', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload)
      });
      if (!res.ok) throw new Error('Dispatch failed');
      return res.json();
    },
    onSuccess: () => {
      toast({ title: 'Success', description: 'Notification sent successfully' });
      setMessage('');
      setRecipient('');
      queryClient.invalidateQueries({ queryKey: ['/api/operations/notifications'] });
    },
    onError: () => {
      toast({ title: 'Error', description: 'Failed to send notification', variant: 'destructive' });
    }
  });

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    sendMutation.mutate({ id: `notif_${Date.now()}`, message, recipient });
  };

  return (
    <div className="space-y-6 max-w-lg">
      <div>
        <h1 className="text-3xl font-bold tracking-tight">Notification Center</h1>
        <p className="text-muted-foreground text-sm">Dispatch ad-hoc notifications and alerts to platform users.</p>
      </div>

      <div className="border rounded-md bg-card p-6 shadow-sm">
        <form onSubmit={handleSubmit} className="space-y-4">
          <div className="space-y-2">
            <label className="text-sm font-medium">Recipient</label>
            <input 
              name="recipient" 
              placeholder="Email or Phone number" 
              className="w-full p-2 border rounded-md bg-background text-sm" 
              value={recipient} 
              onChange={e => setRecipient(e.target.value)} 
              required 
            />
          </div>
          <div className="space-y-2">
            <label className="text-sm font-medium">Message Body</label>
            <textarea 
              name="message" 
              placeholder="Enter message content..." 
              rows={4}
              className="w-full p-2 border rounded-md bg-background text-sm" 
              value={message} 
              onChange={e => setMessage(e.target.value)} 
              required 
            />
          </div>
          <Button type="submit" className="w-full" disabled={sendMutation.isPending}>
            {sendMutation.isPending ? 'Sending...' : 'Send Notification'}
          </Button>
        </form>
      </div>
    </div>
  );
}
