'use client';

import React, { useState } from 'react';
import { withRoleProtection, RoleGuard } from '@/lib/rbac';
import { useAdminQuery, useAdminMutation } from '@/lib/api';
import { 
  LifeBuoy, 
  Plus, 
  AlertTriangle, 
  CheckCircle2, 
  HelpCircle,
  Clock,
  X,
  FileText,
  RotateCw
} from 'lucide-react';
import { z } from 'zod';

interface Incident {
  id: string;
  runtime_id: string;
  incident_type: string;
  severity: string;
  summary: string;
  details: string | null;
  status: string;
  reported_at: string;
  resolved_at: string | null;
}

interface RuntimeNode {
  id: string;
  customer_id: string;
}

const incidentSchema = z.object({
  runtimeId: z.string().min(1, 'Select a target runtime node'),
  incidentType: z.enum(['heartbeat_missed', 'runtime_crash', 'user_reported']),
  severity: z.enum(['Low', 'Medium', 'High', 'Critical']),
  summary: z.string().min(5, 'Provide a summary of at least 5 chars'),
  details: z.string().optional(),
});

function SupportPage() {
  const [activeModal, setActiveModal] = useState<'ticket' | 'resolve' | null>(null);
  const [selectedIncident, setSelectedIncident] = useState<Incident | null>(null);
  
  // Form states
  const [ticketData, setTicketData] = useState({ runtimeId: '', incidentType: 'user_reported', severity: 'Medium', summary: '', details: '' });
  const [errors, setErrors] = useState<Record<string, string>>({});

  // Query hook
  const { data: incidents = [], refetch, isLoading } = useAdminQuery<Incident[]>(['incidents'], '/incidents');

  // Query runtimes
  const { data: runtimes = [] } = useAdminQuery<RuntimeNode[]>(['runtimes'], '/runtimes');

  // Mutations
  const createIncidentMutation = useAdminMutation<any, any>('/incidents', 'POST', () => {
    refetch();
    setActiveModal(null);
    setTicketData({ runtimeId: '', incidentType: 'user_reported', severity: 'Medium', summary: '', details: '' });
  });

  const resolveIncidentMutation = useAdminMutation<any, any>(
    selectedIncident ? `/incidents/${selectedIncident.id}/status` : '/incidents',
    'PUT',
    () => {
      refetch();
      setActiveModal(null);
    }
  );

  const handleCreateTicket = (e: React.FormEvent) => {
    e.preventDefault();
    setErrors({});
    const validation = incidentSchema.safeParse(ticketData);
    if (!validation.success) {
      const fieldErrors: Record<string, string> = {};
      validation.error.errors.forEach(err => {
        if (err.path[0]) fieldErrors[err.path[0].toString()] = err.message;
      });
      setErrors(fieldErrors);
      return;
    }
    
    createIncidentMutation.mutate({
      runtime_id: ticketData.runtimeId,
      incident_type: ticketData.incidentType,
      severity: ticketData.severity,
      summary: ticketData.summary,
      details: ticketData.details || null
    });
  };

  const handleResolve = (incident: Incident) => {
    setSelectedIncident(incident);
    setActiveModal('resolve');
  };

  const confirmResolve = () => {
    if (selectedIncident) {
      resolveIncidentMutation.mutate({ status: 'Resolved' });
    }
  };

  return (
    <div className="space-y-6">
      <div className="flex flex-col md:flex-row md:items-center md:justify-between gap-4">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">Support Operations Center</h1>
          <p className="text-xs text-on-surfaceVariant mt-1">
            Manage operational incidents, crashes, and runtime diagnostics.
          </p>
        </div>
        <div className="flex space-x-2">
          <RoleGuard allowedRoles={['Super Admin', 'Support Engineer', 'Developer']}>
            <button
              onClick={() => {
                if (runtimes.length > 0) {
                  setTicketData(prev => ({ ...prev, runtimeId: runtimes[0]?.id || '' }));
                }
                setActiveModal('ticket');
              }}
              className="flex items-center space-x-2 bg-primary text-primary-foreground text-xs font-semibold px-4 py-2.5 rounded-lg hover:bg-primary/95 transition-all shadow cursor-pointer"
            >
              <Plus className="h-4 w-4" />
              <span>Create Incident</span>
            </button>
          </RoleGuard>
        </div>
      </div>

      {/* Grid of Incidents */}
      <div className="bg-surface-container border border-outline-variant rounded-xl overflow-hidden shadow">
        <div className="overflow-x-auto">
          <table className="w-full text-left border-collapse">
            <thead>
              <tr className="bg-surface-container-high border-b border-outline-variant text-xs text-on-surfaceVariant uppercase font-mono tracking-wider font-semibold">
                <th className="px-6 py-4">Incident ID</th>
                <th className="px-6 py-4">Runtime ID</th>
                <th className="px-6 py-4">Type</th>
                <th className="px-6 py-4">Summary details</th>
                <th className="px-6 py-4">Severity</th>
                <th className="px-6 py-4">Reported Date</th>
                <th className="px-6 py-4 text-right">Status</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-outline-variant/40 text-xs font-mono">
              {isLoading ? (
                <tr>
                  <td colSpan={7} className="px-6 py-8 text-center text-on-surfaceVariant font-sans">
                    Loading incidents logs...
                  </td>
                </tr>
              ) : incidents.map((t) => (
                <tr key={t.id} className="hover:bg-surface-container-high/40 transition-colors">
                  <td className="px-6 py-4 text-primary font-bold">{t.id}</td>
                  <td className="px-6 py-4 text-foreground font-bold">{t.runtime_id}</td>
                  <td className="px-6 py-4 text-on-surfaceVariant">{t.incident_type}</td>
                  <td className="px-6 py-4 text-foreground font-sans max-w-sm truncate" title={t.summary}>{t.summary}</td>
                  <td className="px-6 py-4">
                    <span className={`px-2 py-0.5 rounded text-[10px] font-bold ${
                      t.severity === 'Critical' || t.severity === 'High' ? 'bg-red-500/10 text-red-400 border border-red-500/20' :
                      t.severity === 'Medium' ? 'bg-yellow-500/10 text-yellow-400 border border-yellow-500/20' :
                      'bg-surface-container-highest text-on-surfaceVariant border border-outline-variant'
                    }`}>
                      {t.severity}
                    </span>
                  </td>
                  <td className="px-6 py-4 text-outline">{t.reported_at}</td>
                  <td className="px-6 py-4 text-right">
                    {t.status === 'Open' ? (
                      <button
                        onClick={() => handleResolve(t)}
                        className="text-xs bg-error-container/10 hover:bg-error-container/20 text-error-dim border border-error-container/20 px-2 py-1 rounded transition-all cursor-pointer font-semibold font-sans"
                      >
                        Resolve
                      </button>
                    ) : (
                      <span className="inline-flex items-center space-x-1 px-2.5 py-0.5 rounded text-[10px] font-bold bg-green-500/10 text-green-400 border border-green-500/20">
                        <span>{t.status}</span>
                      </span>
                    )}
                  </td>
                </tr>
              ))}
              {!isLoading && incidents.length === 0 && (
                <tr>
                  <td colSpan={7} className="px-6 py-8 text-center text-on-surfaceVariant font-sans">
                    No incidents recorded.
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      </div>

      {/* CREATE TICKET MODAL */}
      {activeModal === 'ticket' && (
        <div className="fixed inset-0 bg-black/60 backdrop-blur-sm z-50 flex items-center justify-center p-4">
          <div className="bg-surface-container border border-outline-variant w-full max-w-md rounded-xl p-6 shadow-2xl space-y-6">
            <div className="flex justify-between items-center">
              <h2 className="text-sm font-semibold tracking-wide text-foreground uppercase font-mono">Create Incident Ticket</h2>
              <button onClick={() => setActiveModal(null)} className="text-outline hover:text-foreground">
                <X className="h-5 w-5" />
              </button>
            </div>
            <form onSubmit={handleCreateTicket} className="space-y-4 font-sans text-xs">
              <div className="space-y-1">
                <label className="text-[10px] font-semibold text-on-surfaceVariant uppercase font-mono block">Runtime Cluster Node</label>
                <select
                  value={ticketData.runtimeId}
                  onChange={(e) => setTicketData({...ticketData, runtimeId: e.target.value})}
                  className="w-full bg-surface-container-low border border-outline-variant rounded px-3 py-2 text-xs text-foreground focus:outline-none focus:border-primary cursor-pointer font-mono"
                >
                  {runtimes.map(rt => (
                    <option key={rt.id} value={rt.id}>{rt.id} (Tenant: {rt.customer_id})</option>
                  ))}
                </select>
                {errors.runtimeId && <span className="text-[10px] text-error-dim font-mono">{errors.runtimeId}</span>}
              </div>

              <div className="grid grid-cols-2 gap-4">
                <div className="space-y-1">
                  <label className="text-[10px] font-semibold text-on-surfaceVariant uppercase font-mono block">Incident Type</label>
                  <select
                    value={ticketData.incidentType}
                    onChange={(e) => setTicketData({...ticketData, incidentType: e.target.value})}
                    className="w-full bg-surface-container-low border border-outline-variant rounded px-3 py-2 text-xs text-foreground focus:outline-none focus:border-primary cursor-pointer"
                  >
                    <option value="user_reported">User Reported</option>
                    <option value="runtime_crash">Runtime Crash</option>
                    <option value="heartbeat_missed">Heartbeat Missed</option>
                  </select>
                </div>
                <div className="space-y-1">
                  <label className="text-[10px] font-semibold text-on-surfaceVariant uppercase font-mono block">Severity Level</label>
                  <select
                    value={ticketData.severity}
                    onChange={(e) => setTicketData({...ticketData, severity: e.target.value as any})}
                    className="w-full bg-surface-container-low border border-outline-variant rounded px-3 py-2 text-xs text-foreground focus:outline-none focus:border-primary cursor-pointer"
                  >
                    <option value="Low">Low</option>
                    <option value="Medium">Medium</option>
                    <option value="High">High</option>
                    <option value="Critical">Critical</option>
                  </select>
                </div>
              </div>

              <div className="space-y-1">
                <label className="text-[10px] font-semibold text-on-surfaceVariant uppercase font-mono block">Incident Summary</label>
                <input
                  type="text"
                  placeholder="Execution fails due to key re-evaluation latency exception"
                  value={ticketData.summary}
                  onChange={(e) => setTicketData({...ticketData, summary: e.target.value})}
                  className="w-full bg-surface-container-low border border-outline-variant rounded px-3 py-2 text-xs text-foreground focus:outline-none focus:border-primary"
                />
                {errors.summary && <span className="text-[10px] text-error-dim font-mono">{errors.summary}</span>}
              </div>

              <div className="space-y-1">
                <label className="text-[10px] font-semibold text-on-surfaceVariant uppercase font-mono block">Detailed Description (Optional)</label>
                <textarea
                  placeholder="Additional debug logs or details..."
                  value={ticketData.details}
                  onChange={(e) => setTicketData({...ticketData, details: e.target.value})}
                  rows={3}
                  className="w-full bg-surface-container-low border border-outline-variant rounded px-3 py-2 text-xs text-foreground focus:outline-none focus:border-primary resize-none"
                />
              </div>

              <button
                type="submit"
                disabled={createIncidentMutation.isPending}
                className="w-full bg-primary text-primary-foreground font-semibold py-2.5 rounded text-xs hover:bg-primary/95 transition-all shadow cursor-pointer disabled:opacity-50"
              >
                {createIncidentMutation.isPending ? 'Logging Incident...' : 'Log Incident Ticket'}
              </button>
            </form>
          </div>
        </div>
      )}

      {/* RESOLVE CONFIRM MODAL */}
      {activeModal === 'resolve' && selectedIncident && (
        <div className="fixed inset-0 bg-black/60 backdrop-blur-sm z-50 flex items-center justify-center p-4">
          <div className="bg-surface-container border border-outline-variant w-full max-w-md rounded-xl p-6 shadow-2xl space-y-4">
            <h2 className="text-sm font-semibold tracking-wide text-foreground uppercase font-mono">Resolve Incident</h2>
            <p className="text-xs text-on-surfaceVariant">
              Confirm resolution of incident <strong className="text-primary font-mono">{selectedIncident.id}</strong>: &quot;{selectedIncident.summary}&quot;?
            </p>
            <div className="flex space-x-2 pt-2 justify-end">
              <button
                onClick={() => setActiveModal(null)}
                className="px-3 py-1.5 bg-surface-container-high border border-outline-variant rounded text-xs font-semibold cursor-pointer"
              >
                Cancel
              </button>
              <button
                onClick={confirmResolve}
                disabled={resolveIncidentMutation.isPending}
                className="px-3 py-1.5 bg-primary text-primary-foreground rounded text-xs font-semibold cursor-pointer disabled:opacity-50"
              >
                {resolveIncidentMutation.isPending ? 'Resolving...' : 'Confirm Resolve'}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

export default withRoleProtection(SupportPage, ['Super Admin', 'Support Engineer', 'Developer']);
