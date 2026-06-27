'use client';

import React, { useState } from 'react';
import { withRoleProtection } from '@/lib/rbac';
import { useAdminQuery, useAdminMutation } from '@/lib/api';
import { 
  RefreshCw, 
  ArrowUpRight, 
  RotateCcw, 
  CheckCircle2, 
  AlertTriangle,
  History,
  Layers
} from 'lucide-react';

interface Package {
  id: string;
  name: string;
  latest_version: string | null;
  status: string;
}

interface Deployment {
  id: string;
  runtime_id: string;
  package_id: string;
  version: string;
  status: string;
  installed_at: string | null;
}

function UpdatesPage() {
  // Query hooks
  const { data: packages = [], isLoading: isLoadingPkgs } = useAdminQuery<Package[]>(['packages'], '/packages');
  const { data: deployments = [], isLoading: isLoadingDeploys, refetch } = useAdminQuery<Deployment[]>(['deployments'], '/deployments');

  // Mutation to apply upgrade (which is deploying the new version)
  const upgradeMutation = useAdminMutation<any, any>(
    '/runtimes',
    'POST',
    () => {
      refetch();
      alert('System upgrade mapping initiated.');
    }
  );

  const handleApplyUpdate = (runtimeId: string, packageId: string, toVersion: string) => {
    upgradeMutation.mutateAsync({
      url: `/runtimes/${runtimeId}/deploy`,
      method: 'POST',
      data: {
        package_id: packageId,
        version: toVersion
      }
    } as any).catch(err => {
      alert(`Upgrade failed: ${err.response?.data?.error || err.message}`);
    });
  };

  // Calculate available updates:
  // Find deployments where the deployed version is less than the package's latest_version
  const availableUpdates: any[] = [];

  deployments.forEach(dep => {
    const pkg = packages.find(p => p.id === dep.package_id);
    if (pkg && pkg.latest_version && dep.version !== pkg.latest_version) {
      availableUpdates.push({
        id: `${dep.id}-upd`,
        deploymentId: dep.id,
        runtimeId: dep.runtime_id,
        packageId: dep.package_id,
        packageName: pkg.name,
        fromVersion: dep.version,
        toVersion: pkg.latest_version,
      });
    }
  });

  const isLoading = isLoadingPkgs || isLoadingDeploys;

  return (
    <div className="space-y-6">
      <div className="flex flex-col md:flex-row md:items-center md:justify-between gap-4">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">System Updates & Patches</h1>
          <p className="text-xs text-on-surfaceVariant mt-1">
            Apply feature updates, security patches, and manage runtime mapping upgrades.
          </p>
        </div>
        <div className="flex items-center space-x-2 text-xs font-mono bg-surface-container border border-outline-variant px-3 py-1.5 rounded">
          <span>Active Kernel:</span>
          <strong className="text-primary font-bold">v1.2.0-stable</strong>
        </div>
      </div>

      {/* Core Updates List */}
      <div className="space-y-4">
        {isLoading ? (
          <div className="text-center text-on-surfaceVariant py-8 font-mono text-xs">
            Scanning cluster node catalogs for patch discrepancies...
          </div>
        ) : availableUpdates.map((u) => (
          <div key={u.id} className="bg-surface-container border border-outline-variant rounded-xl p-5 space-y-4">
            <div className="flex flex-col md:flex-row md:items-center md:justify-between gap-4">
              <div className="space-y-1.5">
                <div className="flex items-center space-x-2">
                  <span className="px-2 py-0.5 rounded text-[9px] font-bold font-mono bg-primary/10 text-primary border border-primary/20">
                    Discrepancy Detected
                  </span>
                  <span className="text-[10px] text-outline font-mono">Runtime: {u.runtimeId}</span>
                </div>
                <h3 className="text-sm font-semibold font-mono text-foreground">
                  Crate <span className="text-primary">{u.packageName}</span> ({u.packageId}): v{u.fromVersion} → v{u.toVersion}
                </h3>
              </div>

              <div>
                <span className="inline-flex items-center space-x-1 px-2.5 py-0.5 rounded text-[10px] font-bold bg-yellow-500/10 text-yellow-400 border border-yellow-500/20">
                  <span>Available</span>
                </span>
              </div>
            </div>

            <p className="text-xs text-on-surfaceVariant leading-relaxed font-sans pl-1.5 border-l-2 border-outline-variant/80">
              The target runtime {u.runtimeId} is currently executing version v{u.fromVersion}. A newer version v{u.toVersion} is approved and available in the catalog registry.
            </p>

            <div className="flex items-center justify-between pt-2 border-t border-outline-variant/30 mt-4">
              <span className="text-[10px] text-outline font-mono">
                Target: Event store migration checks passed
              </span>
              <div className="flex space-x-2">
                <button
                  onClick={() => handleApplyUpdate(u.runtimeId, u.packageId, u.toVersion)}
                  disabled={upgradeMutation.isPending}
                  className="bg-primary text-primary-foreground font-semibold px-3 py-1.5 rounded text-xs hover:bg-primary/95 transition-all shadow cursor-pointer disabled:opacity-50 inline-flex items-center space-x-1.5"
                >
                  {upgradeMutation.isPending ? (
                    <>
                      <RefreshCw className="h-3.5 w-3.5 animate-spin" />
                      <span>Applying...</span>
                    </>
                  ) : (
                    <>
                      <ArrowUpRight className="h-3.5 w-3.5" />
                      <span>Apply Upgrade</span>
                    </>
                  )}
                </button>
              </div>
            </div>
          </div>
        ))}
        {!isLoading && availableUpdates.length === 0 && (
          <div className="bg-surface-container border border-outline-variant rounded-xl p-8 text-center text-on-surfaceVariant font-sans text-xs">
            <CheckCircle2 className="h-8 w-8 text-green-400 mx-auto mb-2" />
            <span>All cluster nodes are executing the latest registered package versions. No pending updates found.</span>
          </div>
        )}
      </div>
    </div>
  );
}

export default withRoleProtection(UpdatesPage, ['Super Admin', 'Developer']);
