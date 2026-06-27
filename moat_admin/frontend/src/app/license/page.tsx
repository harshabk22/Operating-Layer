'use client';

import React, { useState } from 'react';
import { withRoleProtection, RoleGuard } from '@/lib/rbac';
import { useAdminQuery, useAdminMutation } from '@/lib/api';
import { 
  Key, 
  Plus, 
  Trash2, 
  CheckCircle, 
  AlertTriangle,
  Clipboard,
  X
} from 'lucide-react';
import { z } from 'zod';

interface RuntimeNode {
  id: string;
  customer_id: string;
  environment: string;
  status: string;
  runtime_api_url: string;
  last_heartbeat_at: string | null;
}

interface Customer {
  id: string;
  organization_name: string;
}

const generateLicenseSchema = z.object({
  customerId: z.string().min(1, 'Select a customer organization'),
  environment: z.enum(['Production', 'Staging', 'Development']),
  runtime_api_url: z.string().url('Must be a valid endpoint HTTPS URL'),
});

function LicensePage() {
  const [activeModal, setActiveModal] = useState<any>(null);
  const [generatedKey, setGeneratedKey] = useState<string | null>(null);
  const [genData, setGenData] = useState({ customerId: '', environment: 'Production', runtime_api_url: '' });
  const [errors, setErrors] = useState<Record<string, string>>({});

  // Query runtimes
  const { data: nodes = [], refetch, isLoading } = useAdminQuery<RuntimeNode[]>(['runtimes'], '/runtimes');

  // Query customers
  const { data: customers = [] } = useAdminQuery<Customer[]>(['customers'], '/customers');

  // Mutation to create runtime
  const createRuntimeMutation = useAdminMutation<any, any>(
    '/customers',
    'POST',
    (data: any) => {
      refetch();
      setGeneratedKey(data.api_key);
    }
  );

  const handleGenerate = (e: React.FormEvent) => {
    e.preventDefault();
    setErrors({});
    const validation = generateLicenseSchema.safeParse(genData);
    if (!validation.success) {
      const fieldErrors: Record<string, string> = {};
      validation.error.errors.forEach(err => {
        if (err.path[0]) fieldErrors[err.path[0].toString()] = err.message;
      });
      setErrors(fieldErrors);
      return;
    }

    createRuntimeMutation.mutateAsync({
      url: `/customers/${genData.customerId}/runtimes`,
      method: 'POST',
      data: {
        environment: genData.environment,
        runtime_api_url: genData.runtime_api_url
      }
    } as any).catch(err => {
      alert(`Generation failed: ${err.response?.data?.error || err.message}`);
    });
  };

  const handleCloseModal = () => {
    setActiveModal(null);
    setGeneratedKey(null);
    setGenData({ customerId: '', environment: 'Production', runtime_api_url: '' });
  };

  const copyToClipboard = (text: string) => {
    navigator.clipboard.writeText(text);
    alert('API Key copied to clipboard!');
  };

  return (
    <div className="space-y-6">
      <div className="flex flex-col md:flex-row md:items-center md:justify-between gap-4">
        <div>
          <h1 className="text-2xl font-bold tracking-tight font-sans">Credential & License Manager</h1>
          <p className="text-xs text-on-surfaceVariant mt-1">
            Issue, audit, and revoke runtime license keys governing capability access.
          </p>
        </div>
        <RoleGuard allowedRoles={['Super Admin', 'Finance']}>
          <button
            onClick={() => {
              if (customers.length > 0) {
                setGenData(prev => ({ ...prev, customerId: customers[0]?.id || '' }));
              }
              setActiveModal('generate');
            }}
            className="flex items-center space-x-2 bg-primary text-primary-foreground text-xs font-semibold px-4 py-2.5 rounded-lg hover:bg-primary/95 transition-all shadow cursor-pointer"
          >
            <Plus className="h-4 w-4" />
            <span>Generate Key</span>
          </button>
        </RoleGuard>
      </div>

      <div className="bg-surface-container border border-outline-variant rounded-xl overflow-hidden shadow">
        <div className="overflow-x-auto">
          <table className="w-full text-left border-collapse">
            <thead>
              <tr className="bg-surface-container-high border-b border-outline-variant text-xs text-on-surfaceVariant uppercase font-mono tracking-wider font-semibold">
                <th className="px-6 py-4">Runtime Node</th>
                <th className="px-6 py-4">Customer ID</th>
                <th className="px-6 py-4">Control Endpoint</th>
                <th className="px-6 py-4">Environment</th>
                <th className="px-6 py-4">Expires / Heartbeat</th>
                <th className="px-6 py-4 text-right">Status</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-outline-variant/40 text-xs font-mono">
              {isLoading ? (
                <tr>
                  <td colSpan={6} className="px-6 py-8 text-center text-on-surfaceVariant font-sans">
                    Loading credentials logs...
                  </td>
                </tr>
              ) : nodes.map((n) => (
                <tr key={n.id} className="hover:bg-surface-container-high/40 transition-colors">
                  <td className="px-6 py-4 font-bold text-foreground">{n.id}</td>
                  <td className="px-6 py-4 text-foreground font-bold">{n.customer_id}</td>
                  <td className="px-6 py-4 text-primary font-mono">{n.runtime_api_url}</td>
                  <td className="px-6 py-4">
                    <span className="px-2 py-0.5 rounded text-[10px] font-bold bg-surface-container-highest text-on-surfaceVariant border border-outline-variant">
                      {n.environment}
                    </span>
                  </td>
                  <td className="px-6 py-4 text-foreground">{n.last_heartbeat_at || 'No check-ins'}</td>
                  <td className="px-6 py-4 text-right">
                    <span className={`inline-flex items-center space-x-1 px-2.5 py-0.5 rounded text-[10px] font-bold ${
                      n.status === 'Active' || n.status === 'Online' ? 'bg-green-500/10 text-green-400 border border-green-500/20' :
                      'bg-error-container/10 text-error-dim border border-error-container/20'
                    }`}>
                      <span>{n.status}</span>
                    </span>
                  </td>
                </tr>
              ))}
              {!isLoading && nodes.length === 0 && (
                <tr>
                  <td colSpan={6} className="px-6 py-8 text-center text-on-surfaceVariant font-sans">
                    No active runtime credentials generated.
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      </div>

      {/* GENERATE MODAL */}
      {activeModal === 'generate' && (
        <div className="fixed inset-0 bg-black/60 backdrop-blur-sm z-50 flex items-center justify-center p-4">
          <div className="bg-surface-container border border-outline-variant w-full max-w-md rounded-xl p-6 shadow-2xl space-y-6">
            <div className="flex justify-between items-center">
              <h2 className="text-sm font-semibold tracking-wide text-foreground uppercase font-mono">Generate License key</h2>
              <button onClick={handleCloseModal} className="text-outline hover:text-foreground">
                <X className="h-5 w-5" />
              </button>
            </div>
            
            {generatedKey ? (
              <div className="space-y-4">
                <div className="p-3 bg-green-500/10 border border-green-500/20 rounded text-xs text-green-400 space-y-1">
                  <strong>License key generated successfully!</strong>
                  <p className="text-[11px] text-on-surfaceVariant">
                    Provide this key in the client daemon header. It will not be shown again.
                  </p>
                </div>
                <div className="space-y-1">
                  <label className="text-[10px] font-semibold text-on-surfaceVariant uppercase font-mono block">Raw API License Key</label>
                  <div className="flex space-x-2">
                    <input
                      type="text"
                      readOnly
                      value={generatedKey}
                      className="flex-1 bg-surface-container-low border border-outline-variant rounded px-3 py-2 text-xs text-foreground focus:outline-none font-mono text-center select-all font-bold"
                    />
                    <button
                      onClick={() => copyToClipboard(generatedKey)}
                      className="p-2 rounded bg-surface-container-high border border-outline-variant hover:text-foreground text-outline"
                    >
                      Copy
                    </button>
                  </div>
                </div>
                <button
                  onClick={handleCloseModal}
                  className="w-full bg-primary text-primary-foreground font-semibold py-2.5 rounded text-xs hover:bg-primary/95 transition-all cursor-pointer font-sans"
                >
                  Close & Refresh Node list
                </button>
              </div>
            ) : (
              <form onSubmit={handleGenerate} className="space-y-4 font-sans text-xs">
                <div className="space-y-1">
                  <label className="text-[10px] font-semibold text-on-surfaceVariant uppercase font-mono block">Target Customer Org</label>
                  <select
                    value={genData.customerId}
                    onChange={(e) => setGenData({...genData, customerId: e.target.value})}
                    className="w-full bg-surface-container-low border border-outline-variant rounded px-3 py-2 text-xs text-foreground focus:outline-none focus:border-primary cursor-pointer font-mono"
                  >
                    {customers.map(c => (
                      <option key={c.id} value={c.id}>{c.organization_name} ({c.id})</option>
                    ))}
                  </select>
                </div>

                <div className="grid grid-cols-2 gap-4">
                  <div className="space-y-1">
                    <label className="text-[10px] font-semibold text-on-surfaceVariant uppercase font-mono block">Environment</label>
                    <select
                      value={genData.environment}
                      onChange={(e) => setGenData({...genData, environment: e.target.value as any})}
                      className="w-full bg-surface-container-low border border-outline-variant rounded px-3 py-2 text-xs text-foreground focus:outline-none focus:border-primary cursor-pointer font-mono"
                    >
                      <option value="Production">Production</option>
                      <option value="Staging">Staging</option>
                      <option value="Development">Development</option>
                    </select>
                  </div>
                  <div className="space-y-1">
                    <label className="text-[10px] font-semibold text-on-surfaceVariant uppercase font-mono block">API Endpoint URL</label>
                    <input
                      type="text"
                      placeholder="https://api.west.moatageex.net"
                      value={genData.runtime_api_url}
                      onChange={(e) => setGenData({...genData, runtime_api_url: e.target.value})}
                      className="w-full bg-surface-container-low border border-outline-variant rounded px-3 py-2 text-xs text-foreground focus:outline-none focus:border-primary font-mono text-[11px]"
                    />
                    {errors.runtime_api_url && <span className="text-[10px] text-error-dim font-mono">{errors.runtime_api_url}</span>}
                  </div>
                </div>

                <button
                  type="submit"
                  disabled={createRuntimeMutation.isPending}
                  className="w-full bg-primary text-primary-foreground font-semibold py-2.5 rounded text-xs hover:bg-primary/95 transition-all shadow cursor-pointer disabled:opacity-50"
                >
                  {createRuntimeMutation.isPending ? 'Generating...' : 'Sign Key & Save'}
                </button>
              </form>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

export default withRoleProtection(LicensePage, ['Super Admin', 'Developer', 'Finance']);
