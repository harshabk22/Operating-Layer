'use client';

import React, { useState } from 'react';
import { withRoleProtection, RoleGuard } from '@/lib/rbac';
import { useAdminQuery, useAdminMutation } from '@/lib/api';
import { z } from 'zod';
import { 
  Cpu, 
  Plus, 
  Terminal, 
  Settings, 
  AlertTriangle,
  X,
  Activity,
  HardDrive
} from 'lucide-react';

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

const registerRuntimeSchema = z.object({
  customerId: z.string().min(1, 'Select a customer organization'),
  environment: z.enum(['Production', 'Staging', 'Development']),
  runtime_api_url: z.string().url('Must be a valid endpoint HTTPS URL'),
});

function RuntimePage() {
  const [activeModal, setActiveModal] = useState(false);
  const [generatedKey, setGeneratedKey] = useState<string | null>(null);
  
  // Registration state
  const [regData, setRegData] = useState({ customerId: '', environment: 'Production', runtime_api_url: '' });
  const [errors, setErrors] = useState<Record<string, string>>({});

  // Query runtimes
  const { data: nodes = [], refetch, isLoading } = useAdminQuery<RuntimeNode[]>(['runtimes'], '/runtimes');

  // Query customers to populate dropdown
  const { data: customers = [] } = useAdminQuery<Customer[]>(['customers'], '/customers');

  // Mutation to create runtime
  const createRuntimeMutation = useAdminMutation<any, any>(
    '/customers', // fallback url
    'POST',
    (data: any) => {
      refetch();
      setGeneratedKey(data.api_key);
    }
  );

  const handleRegister = (e: React.FormEvent) => {
    e.preventDefault();
    setErrors({});
    const validation = registerRuntimeSchema.safeParse(regData);
    if (!validation.success) {
      const fieldErrors: Record<string, string> = {};
      validation.error.errors.forEach(err => {
        if (err.path[0]) fieldErrors[err.path[0].toString()] = err.message;
      });
      setErrors(fieldErrors);
      return;
    }

    createRuntimeMutation.mutateAsync({
      url: `/customers/${regData.customerId}/runtimes`,
      method: 'POST',
      data: {
        environment: regData.environment,
        runtime_api_url: regData.runtime_api_url
      }
    } as any).catch(err => {
      alert(`Registration failed: ${err.response?.data?.error || err.message}`);
    });
  };

  const handleCloseModal = () => {
    setActiveModal(false);
    setGeneratedKey(null);
    setRegData({ customerId: '', environment: 'Production', runtime_api_url: '' });
  };

  return (
    <div className="space-y-6">
      <div className="flex flex-col md:flex-row md:items-center md:justify-between gap-4">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">WASM Runtimes</h1>
          <p className="text-xs text-on-surfaceVariant mt-1">
            Register and monitor standard cluster virtual nodes compiling and executing Crate events.
          </p>
        </div>
        <button
          onClick={() => {
            if (customers.length > 0) {
              setRegData(prev => ({ ...prev, customerId: customers[0]?.id || '' }));
            }
            setActiveModal(true);
          }}
          className="flex items-center space-x-2 bg-primary text-primary-foreground text-xs font-semibold px-4 py-2.5 rounded-lg hover:bg-primary/95 transition-all shadow cursor-pointer"
        >
          <Plus className="h-4 w-4" />
          <span>Register Runtime Node</span>
        </button>
      </div>

      {/* Runtimes Grid */}
      <div className="grid gap-6 md:grid-cols-3">
        {isLoading ? (
          <div className="col-span-3 text-center text-on-surfaceVariant py-8 font-mono text-xs">
            Querying runtime cluster nodes...
          </div>
        ) : nodes.map((n) => (
          <div key={n.id} className="bg-surface-container border border-outline-variant rounded-xl p-5 space-y-4">
            <div className="flex justify-between items-start">
              <div className="space-y-1">
                <div className="flex items-center space-x-2">
                  <Cpu className="h-4.5 w-4.5 text-primary" />
                  <h3 className="text-sm font-semibold font-mono text-foreground">{n.id}</h3>
                </div>
                <span className="text-[10px] text-outline font-mono block">Customer Org ID: {n.customer_id}</span>
              </div>
              <span className={`inline-flex items-center space-x-1 px-2.5 py-0.5 rounded text-[10px] font-bold ${
                n.status === 'Active' || n.status === 'Online' ? 'bg-green-500/10 text-green-400 border border-green-500/20' :
                n.status === 'Suspended' ? 'bg-error-container/10 text-error-dim border border-error-container/20' :
                'bg-surface-container-highest text-on-surfaceVariant border border-outline-variant'
              }`}>
                <span>{n.status}</span>
              </span>
            </div>

            <div className="space-y-3 py-2 border-t border-b border-outline-variant/40 text-xs font-mono">
              <div>
                <span className="text-[10px] text-on-surfaceVariant uppercase block">Control Endpoint</span>
                <span className="text-foreground text-[11px] select-all font-semibold break-all">{n.runtime_api_url}</span>
              </div>
              <div>
                <span className="text-[10px] text-on-surfaceVariant uppercase block">Environment</span>
                <span className="text-foreground font-semibold">{n.environment}</span>
              </div>
              <div>
                <span className="text-[10px] text-on-surfaceVariant uppercase block">Last Heartbeat</span>
                <span className="text-foreground">{n.last_heartbeat_at || 'Never checked in'}</span>
              </div>
            </div>

            <div className="flex justify-between items-center pt-1">
              <span className="text-[10px] text-on-surfaceVariant font-mono">Sandboxed Kernel v1.2</span>
            </div>
          </div>
        ))}
        {!isLoading && nodes.length === 0 && (
          <div className="col-span-3 text-center text-on-surfaceVariant py-8 font-mono text-xs">
            No runtime nodes registered in the cluster.
          </div>
        )}
      </div>

      {/* REGISTER MODAL */}
      {activeModal && (
        <div className="fixed inset-0 bg-black/60 backdrop-blur-sm z-50 flex items-center justify-center p-4">
          <div className="bg-surface-container border border-outline-variant w-full max-w-md rounded-xl p-6 shadow-2xl space-y-6">
            <div className="flex justify-between items-center">
              <h2 className="text-sm font-semibold tracking-wide text-foreground uppercase font-mono">Register Runtime Node</h2>
              <button onClick={handleCloseModal} className="text-outline hover:text-foreground">
                <X className="h-5 w-5" />
              </button>
            </div>
            
            {generatedKey ? (
              <div className="space-y-4">
                <div className="p-3 bg-green-500/10 border border-green-500/20 rounded text-xs text-green-400 space-y-1">
                  <strong>Node Registered successfully!</strong>
                  <p className="text-[11px] text-on-surfaceVariant">
                    Store this X-API-Key immediately. For security reasons, it cannot be recovered or displayed again.
                  </p>
                </div>
                <div className="space-y-1">
                  <label className="text-[10px] font-semibold text-on-surfaceVariant uppercase font-mono block">Raw API Key</label>
                  <input
                    type="text"
                    readOnly
                    value={generatedKey}
                    className="w-full bg-surface-container-low border border-outline-variant rounded px-3 py-2 text-xs text-foreground focus:outline-none font-mono text-center select-all font-bold"
                  />
                </div>
                <button
                  onClick={handleCloseModal}
                  className="w-full bg-primary text-primary-foreground font-semibold py-2.5 rounded text-xs hover:bg-primary/95 transition-all cursor-pointer"
                >
                  Close & Refresh Node list
                </button>
              </div>
            ) : (
              <form onSubmit={handleRegister} className="space-y-4">
                <div className="space-y-1">
                  <label className="text-[10px] font-semibold text-on-surfaceVariant uppercase font-mono block">Tenant Organization</label>
                  <select
                    value={regData.customerId}
                    onChange={(e) => setRegData({...regData, customerId: e.target.value})}
                    className="w-full bg-surface-container-low border border-outline-variant rounded px-3 py-2 text-xs text-foreground focus:outline-none focus:border-primary cursor-pointer font-mono"
                  >
                    {customers.map(c => (
                      <option key={c.id} value={c.id}>{c.organization_name} ({c.id})</option>
                    ))}
                  </select>
                  {errors.customerId && <span className="text-[10px] text-error-dim font-mono">{errors.customerId}</span>}
                </div>

                <div className="space-y-1">
                  <label className="text-[10px] font-semibold text-on-surfaceVariant uppercase font-mono block">Environment</label>
                  <select
                    value={regData.environment}
                    onChange={(e) => setRegData({...regData, environment: e.target.value as any})}
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
                    value={regData.runtime_api_url}
                    onChange={(e) => setRegData({...regData, runtime_api_url: e.target.value})}
                    className="w-full bg-surface-container-low border border-outline-variant rounded px-3 py-2 text-xs text-foreground focus:outline-none focus:border-primary font-mono text-[11px]"
                  />
                  {errors.runtime_api_url && <span className="text-[10px] text-error-dim font-mono">{errors.runtime_api_url}</span>}
                </div>

                <button
                  type="submit"
                  disabled={createRuntimeMutation.isPending}
                  className="w-full bg-primary text-primary-foreground font-semibold py-2.5 rounded text-xs hover:bg-primary/95 transition-all shadow cursor-pointer disabled:opacity-50"
                >
                  {createRuntimeMutation.isPending ? 'Registering Node...' : 'Register Execution Cluster Node'}
                </button>
              </form>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

export default withRoleProtection(RuntimePage, ['Super Admin', 'Developer']);
