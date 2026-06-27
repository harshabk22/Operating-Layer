'use client';

import React, { useState } from 'react';
import { withRoleProtection, RoleGuard } from '@/lib/rbac';
import { useAdminQuery, useAdminMutation } from '@/lib/api';
import { 
  Rocket, 
  CheckCircle, 
  AlertTriangle, 
  RotateCw, 
  ExternalLink,
  Info,
  Layers,
  Search
} from 'lucide-react';

interface Deployment {
  id: string;
  runtime_id: string;
  package_id: string;
  version: string;
  status: string;
  error_message: string | null;
  installed_at: string | null;
  created_at: string;
}

function DeploymentsPage() {
  const [searchTerm, setSearchTerm] = useState('');

  // Query hook
  const { data: deployments = [], refetch, isLoading } = useAdminQuery<Deployment[]>(['deployments'], '/deployments');

  // Rollback mutation
  const rollbackMutation = useAdminMutation<any, any>(
    '/deployments', // Fallback, we'll build dynamic URL in mutate call below
    'POST',
    () => refetch()
  );

  const handleRollback = (deploymentId: string) => {
    if (confirm('Warning: Rolling back this deployment changes the active version mapping and might trigger runtime rehydration. Proceed?')) {
      rollbackMutation.mutateAsync({
        url: `/deployments/${deploymentId}/rollback`,
        method: 'POST',
      } as any);
    }
  };

  const handleSyncAll = () => {
    refetch();
  };

  return (
    <div className="space-y-6">
      <div className="flex flex-col md:flex-row md:items-center md:justify-between gap-4">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">Active Deployments</h1>
          <p className="text-xs text-on-surfaceVariant mt-1">
            Orchestrate business Crates and map package releases to physical runtimes.
          </p>
        </div>
        <div className="flex space-x-2">
          <button
            onClick={handleSyncAll}
            className="flex items-center space-x-2 bg-primary text-primary-foreground text-xs font-semibold px-4 py-2.5 rounded-lg hover:bg-primary/95 transition-all shadow cursor-pointer"
          >
            <RotateCw className="h-4 w-4" />
            <span>Refresh Deployments</span>
          </button>
        </div>
      </div>

      {/* Info Warning Banner */}
      <div className="bg-surface-container-high border border-outline-variant p-4 rounded-xl flex items-start space-x-3">
        <Info className="h-5 w-5 text-primary shrink-0 mt-0.5" />
        <div className="text-xs leading-relaxed space-y-1">
          <strong className="text-foreground">Package Version Pinning Law</strong>
          <p className="text-on-surfaceVariant">
            Under active rules, cases are pinned to their specific <strong className="text-primary font-mono">PackageVersion</strong> upon creation. Upgrading version mapping here will only affect newly spawned execution workflows.
          </p>
        </div>
      </div>

      {/* Grid search */}
      <div className="relative w-full md:w-80">
        <Search className="absolute left-3 top-2.5 h-4 w-4 text-outline" />
        <input
          type="text"
          placeholder="Filter package deployments..."
          value={searchTerm}
          onChange={(e) => setSearchTerm(e.target.value)}
          className="pl-9 pr-4 py-2 w-full text-xs bg-surface-container-low border border-outline-variant rounded-lg text-foreground focus:outline-none focus:border-primary font-mono"
        />
      </div>

      {/* Deployments List */}
      <div className="grid gap-6 md:grid-cols-2">
        {isLoading ? (
          <div className="col-span-2 text-center text-on-surfaceVariant py-8 font-mono text-xs">
            Querying active deployments list...
          </div>
        ) : deployments.map((d) => (
          <div key={d.id} className="bg-surface-container border border-outline-variant rounded-xl p-5 space-y-4">
            <div className="flex justify-between items-start">
              <div className="space-y-1">
                <div className="flex items-center space-x-2">
                  <Layers className="h-4.5 w-4.5 text-primary" />
                  <h3 className="text-sm font-semibold font-mono text-foreground">{d.package_id}</h3>
                </div>
                <span className="text-[10px] text-outline font-mono block">Deploy ID: {d.id}</span>
              </div>
              <span className={`inline-flex items-center space-x-1 px-2.5 py-0.5 rounded text-[10px] font-bold ${
                d.status === 'Installed' || d.status === 'success' ? 'bg-green-500/10 text-green-400 border border-green-500/20' :
                d.status === 'Installing' ? 'bg-yellow-500/10 text-yellow-400 border border-yellow-500/20' :
                'bg-error-container/10 text-error-dim border border-error-container/20'
              }`}>
                <span>{d.status}</span>
              </span>
            </div>

            <div className="grid grid-cols-2 gap-4 py-2 border-t border-b border-outline-variant/40 text-xs font-mono">
              <div>
                <span className="text-[10px] text-on-surfaceVariant uppercase block">Target Runtime</span>
                <span className="text-foreground font-semibold">{d.runtime_id}</span>
              </div>
              <div>
                <span className="text-[10px] text-on-surfaceVariant uppercase block">Version Pin</span>
                <span className="text-foreground font-semibold">v{d.version}</span>
              </div>
              <div>
                <span className="text-[10px] text-on-surfaceVariant uppercase block">Installed At</span>
                <span className="text-foreground">{d.installed_at || 'Installing...'}</span>
              </div>
              <div>
                <span className="text-[10px] text-on-surfaceVariant uppercase block">Created At</span>
                <span className="text-foreground">{d.created_at}</span>
              </div>
            </div>

            {d.error_message && (
              <div className="p-2 bg-error-container/10 border border-error-container/20 rounded text-[10px] text-error-dim font-mono break-all">
                Error: {d.error_message}
              </div>
            )}

            <div className="flex justify-between items-center pt-2">
              <span className="text-[10px] text-on-surfaceVariant font-mono">
                Engine: WASM Sandbox V8
              </span>
              <div className="flex space-x-2">
                <button
                  onClick={() => handleRollback(d.id)}
                  disabled={rollbackMutation.isPending}
                  className="px-3 py-1.5 bg-error-container/10 hover:bg-error-container/20 border border-error-container/20 text-error-dim rounded text-xs font-semibold cursor-pointer font-sans disabled:opacity-50"
                >
                  Rollback Version
                </button>
              </div>
            </div>
          </div>
        ))}
        {!isLoading && deployments.length === 0 && (
          <div className="col-span-2 text-center text-on-surfaceVariant py-8 font-mono text-xs">
            No active deployments recorded.
          </div>
        )}
      </div>
    </div>
  );
}

export default withRoleProtection(DeploymentsPage, ['Super Admin', 'Developer']);
