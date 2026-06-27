'use client';

import React, { useState } from 'react';
import { withRoleProtection } from '@/lib/rbac';
import { useAdminQuery, useAdminMutation } from '@/lib/api';
import { z } from 'zod';
import { 
  ShieldCheck, 
  Check, 
  X, 
  AlertTriangle,
  Info,
  Clock
} from 'lucide-react';

interface PendingPackage {
  id: string;
  package_id: string;
  package_name: string;
  version: string;
  checksum_sha256: string;
  binary_url: string;
  created_at: string;
}

const rejectSchema = z.object({
  reason: z.string().min(15, 'Rejection reason must be at least 15 characters long'),
});

function ApprovalsPage() {
  const [activeRequest, setActiveRequest] = useState<PendingPackage | null>(null);
  const [rejectionReason, setRejectionReason] = useState('');
  const [error, setError] = useState('');

  // Query hook
  const { data: requests = [], refetch, isLoading } = useAdminQuery<PendingPackage[]>(['pending-packages'], '/packages/pending');

  // Approve mutation
  const approveMutation = useAdminMutation<any, any>('/packages', 'POST', () => {
    refetch();
    alert('Package version approved successfully.');
  });

  // Reject mutation
  const rejectMutation = useAdminMutation<any, any>('/packages', 'POST', () => {
    refetch();
    setActiveRequest(null);
    setRejectionReason('');
    alert('Package version rejected successfully.');
  });

  const handleApprove = (packageId: string, version: string) => {
    approveMutation.mutateAsync({
      url: `/packages/${packageId}/versions/${version}/approve`,
      method: 'POST',
    } as any);
  };

  const handleReject = (e: React.FormEvent) => {
    e.preventDefault();
    setError('');

    const validation = rejectSchema.safeParse({ reason: rejectionReason });
    if (!validation.success) {
      setError(validation.error.errors[0]?.message || 'Invalid reason');
      return;
    }

    if (activeRequest) {
      rejectMutation.mutateAsync({
        url: `/packages/${activeRequest.package_id}/versions/${activeRequest.version}/reject`,
        method: 'POST',
        data: { reason: rejectionReason }
      } as any);
    }
  };

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-bold tracking-tight">Pending Approvals</h1>
        <p className="text-xs text-on-surfaceVariant mt-1">
          Review and authorize critical package updates before they become available in the marketplace.
        </p>
      </div>

      <div className="bg-surface-container border border-outline-variant rounded-xl overflow-hidden shadow">
        <div className="overflow-x-auto">
          <table className="w-full text-left border-collapse">
            <thead>
              <tr className="bg-surface-container-high border-b border-outline-variant text-xs text-on-surfaceVariant uppercase font-mono tracking-wider font-semibold">
                <th className="px-6 py-4">Package ID</th>
                <th className="px-6 py-4">Package Name</th>
                <th className="px-6 py-4">Version</th>
                <th className="px-6 py-4">Binary URL</th>
                <th className="px-6 py-4">Checksum SHA-256</th>
                <th className="px-6 py-4">Requested At</th>
                <th className="px-6 py-4 text-right">Actions</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-outline-variant/40 text-xs font-mono">
              {isLoading ? (
                <tr>
                  <td colSpan={7} className="px-6 py-8 text-center text-on-surfaceVariant font-sans">
                    Loading pending requests...
                  </td>
                </tr>
              ) : requests.map((r) => (
                <tr key={r.id} className="hover:bg-surface-container-high/40 transition-colors">
                  <td className="px-6 py-4 text-primary font-bold">{r.package_id}</td>
                  <td className="px-6 py-4 font-sans font-medium text-foreground">{r.package_name}</td>
                  <td className="px-6 py-4 font-bold text-foreground">v{r.version}</td>
                  <td className="px-6 py-4 text-on-surfaceVariant truncate max-w-xs" title={r.binary_url}>
                    {r.binary_url}
                  </td>
                  <td className="px-6 py-4 text-on-surfaceVariant truncate max-w-xs font-mono" title={r.checksum_sha256}>
                    {r.checksum_sha256}
                  </td>
                  <td className="px-6 py-4 text-outline font-sans">
                    {r.created_at}
                  </td>
                  <td className="px-6 py-4 text-right space-x-2">
                    <button
                      onClick={() => handleApprove(r.package_id, r.version)}
                      disabled={approveMutation.isPending}
                      className="text-xs bg-green-500/10 hover:bg-green-500/20 text-green-400 border border-green-500/20 px-2.5 py-1.5 rounded transition-all cursor-pointer font-sans font-semibold inline-flex items-center space-x-1 disabled:opacity-50"
                    >
                      <Check className="h-3.5 w-3.5" />
                      <span>Approve</span>
                    </button>
                    <button
                      onClick={() => setActiveRequest(r)}
                      disabled={rejectMutation.isPending}
                      className="text-xs bg-error-container/10 hover:bg-error-container/20 text-error-dim border border-error-container/20 px-2.5 py-1.5 rounded transition-all cursor-pointer font-sans font-semibold inline-flex items-center space-x-1 disabled:opacity-50"
                    >
                      <X className="h-3.5 w-3.5" />
                      <span>Reject</span>
                    </button>
                  </td>
                </tr>
              ))}
              {!isLoading && requests.length === 0 && (
                <tr>
                  <td colSpan={7} className="px-6 py-8 text-center text-on-surfaceVariant font-sans">
                    All clear! There are no pending approvals.
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      </div>

      {/* REJECTION POPUP */}
      {activeRequest && (
        <div className="fixed inset-0 bg-black/60 backdrop-blur-sm z-50 flex items-center justify-center p-4">
          <div className="bg-surface-container border border-outline-variant w-full max-w-md rounded-xl p-6 shadow-2xl space-y-6">
            <div className="flex justify-between items-center">
              <h2 className="text-sm font-semibold tracking-wide text-error-dim uppercase font-mono">Reject Action Request</h2>
              <button onClick={() => setActiveRequest(null)} className="text-outline hover:text-foreground">
                <X className="h-5 w-5" />
              </button>
            </div>
            
            <div className="p-3 bg-surface-container-low border border-outline-variant rounded text-xs space-y-1">
              <span className="text-on-surfaceVariant block">Target Action:</span>
              <strong className="text-foreground">Publish Version v{activeRequest.version} of {activeRequest.package_name}</strong>
            </div>

            <form onSubmit={handleReject} className="space-y-4">
              <div className="space-y-1">
                <label className="text-[10px] font-semibold text-on-surfaceVariant uppercase font-mono block">Rejection Justification</label>
                <textarea
                  placeholder="Specify why you are rejecting this administrative request..."
                  value={rejectionReason}
                  onChange={(e) => setRejectionReason(e.target.value)}
                  rows={4}
                  className="w-full bg-surface-container-low border border-outline-variant rounded px-3 py-2 text-xs text-foreground focus:outline-none focus:border-primary resize-none"
                />
                <span className="text-[9px] text-outline block">Must explain the reason clearly (min. 15 chars).</span>
                {error && <span className="text-[10px] text-error-dim font-mono block">{error}</span>}
              </div>

              <button
                type="submit"
                disabled={rejectMutation.isPending}
                className="w-full bg-error text-error-foreground font-semibold py-2.5 rounded text-xs hover:bg-error/95 transition-all shadow cursor-pointer disabled:opacity-50"
              >
                {rejectMutation.isPending ? 'Rejecting...' : 'Confirm Rejection Action'}
              </button>
            </form>
          </div>
        </div>
      )}
    </div>
  );
}

export default withRoleProtection(ApprovalsPage, ['Super Admin', 'Developer', 'Finance']);
