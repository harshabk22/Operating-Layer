import { useState } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';

interface Visit {
  id: string;
  name: string;
  version: number;
  timestamp: number;
}

export default function Calendar() {
  const queryClient = useQueryClient();
  const [showNew, setShowNew] = useState(false);
  const [title, setTitle] = useState('');
  const [date, setDate] = useState('');
  const [statusMsg, setStatusMsg] = useState<string | null>(null);

  const { data: visits, isLoading, error } = useQuery<Visit[]>({
    queryKey: ['visits'],
    queryFn: async () => {
      const res = await fetch('/api/operations/visits');
      if (!res.ok) throw new Error('Failed to fetch visits');
      return res.json();
    }
  });

  const createMutation = useMutation({
    mutationFn: async (payload: { id: string, title: string, date: string }) => {
      const res = await fetch('/api/operations/visits', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload)
      });
      if (!res.ok) throw new Error('Scheduling failed');
      return res.json();
    },
    onSuccess: () => {
      setStatusMsg('Visit scheduled successfully');
      setShowNew(false);
      setTitle('');
      setDate('');
      queryClient.invalidateQueries({ queryKey: ['visits'] });
    },
    onError: () => setStatusMsg('Failed to schedule visit')
  });

  return (
    <div className="p-8">
      <h1 className="text-2xl font-bold mb-4">Calendar & Visits</h1>
      
      <div className="mb-4">
        <button className="bg-indigo-600 px-4 py-2 rounded" onClick={() => setShowNew(true)}>
          Schedule Visit
        </button>
      </div>

      {showNew && (
        <form onSubmit={(e) => {
          e.preventDefault();
          createMutation.mutate({ id: `visit_${Date.now()}`, title, date });
        }} className="mb-8 p-4 border border-slate-700 rounded flex flex-col gap-2 max-w-sm">
          <input name="title" placeholder="Event Title" className="p-2 bg-slate-800 border border-slate-600 rounded" value={title} onChange={e => setTitle(e.target.value)} required />
          <input name="date" type="datetime-local" className="p-2 bg-slate-800 border border-slate-600 rounded" value={date} onChange={e => setDate(e.target.value)} required />
          <button type="submit" className="bg-green-600 p-2 rounded" disabled={createMutation.isPending}>
            {createMutation.isPending ? 'Scheduling...' : 'Confirm'}
          </button>
        </form>
      )}

      {statusMsg && <div className="mb-4 p-4 bg-slate-800 text-slate-300 rounded border border-slate-700">{statusMsg}</div>}

      {isLoading && <div className="text-slate-400">Loading calendar...</div>}
      {error && <div className="text-red-400">Error: {error.message}</div>}
      
      {!isLoading && !error && visits?.length === 0 && (
        <div className="p-8 border border-slate-800 rounded-lg text-center text-slate-500">
          No upcoming visits scheduled.
        </div>
      )}

      {!isLoading && !error && visits && visits.length > 0 && (
        <div className="grid gap-4">
          {visits.map(visit => (
            <div key={visit.id} className="border-l-4 border-l-indigo-500 bg-slate-900 p-4 rounded shadow-sm">
              <h3 className="font-semibold text-lg">{visit.id}</h3>
              <p className="text-sm text-slate-400">{visit.name}</p>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
