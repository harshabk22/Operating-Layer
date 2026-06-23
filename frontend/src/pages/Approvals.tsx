import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';

interface Approval {
  id: string;
  name: string;
  version: number;
  timestamp: number;
}

export default function Approvals() {
  const [statusMsg] = useState<string | null>(null);

  const { data: approvals, isLoading, error } = useQuery<Approval[]>({
    queryKey: ['approvals'],
    queryFn: async () => {
      const res = await fetch('/api/operations/approvals');
      if (!res.ok) throw new Error('Failed to fetch approvals');
      return res.json();
    }
  });

  // const approveMutation = useMutation({
  //   mutationFn: async (id: string) => {
  //     const res = await fetch(`/api/operations/approvals/${id}/approve`, { method: 'POST' });
  //     if (!res.ok) throw new Error('Approval failed');
  //     return res.json();
  //   },
  //   onSuccess: () => {
  //     setStatusMsg('Action approved');
  //     queryClient.invalidateQueries({ queryKey: ['approvals'] });
  //   },
  //   onError: () => setStatusMsg('Failed to approve action')
  // });

  return (
    <div className="p-8">
      <h1 className="text-2xl font-bold mb-4">Human Approvals</h1>
      
      {statusMsg && <div className="mb-4 p-4 bg-slate-800 text-slate-300 rounded border border-slate-700">{statusMsg}</div>}

      {isLoading && <div className="text-slate-400">Loading approvals...</div>}
      {error && <div className="text-red-400">Error: {error.message}</div>}
      
      {!isLoading && !error && approvals?.length === 0 && (
        <div className="p-8 border border-slate-800 rounded-lg text-center text-slate-500">
          No actions required. Inbox zero!
        </div>
      )}

      {!isLoading && !error && approvals && approvals.length > 0 && (
        <div className="grid gap-4">
          {approvals.map(approval => (
            <div key={approval.id} className="border border-yellow-900/50 bg-slate-900 p-4 rounded flex justify-between items-center">
              <div>
                <h3 className="font-semibold text-lg">{approval.id}</h3>
                <p className="text-sm text-slate-400">{approval.name}</p>
              </div>
              <span className="text-green-400 text-sm px-4 py-2 border border-green-900 rounded bg-green-900/20">Approved</span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
