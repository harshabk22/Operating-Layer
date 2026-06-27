'use client';

import React, { useState } from 'react';
import { withRoleProtection, useRBAC, RoleGuard } from '@/lib/rbac';
import { useAdminQuery, useAdminMutation } from '@/lib/api';
import { z } from 'zod';
import { 
  Plus, 
  Search, 
  ShieldAlert, 
  TrendingUp, 
  AlertTriangle,
  X,
  CheckCircle,
  Clock
} from 'lucide-react';

interface Customer {
  id: string;
  organization_name: string;
  organization_id: string;
  status: string;
  tier: string;
  created_at: string;
  updated_at: string;
}

const createCustomerSchema = z.object({
  organization_id: z.string().min(3, 'ID must be at least 3 chars').regex(/^[a-z0-9_]+$/, 'Only lowercase, numbers and underscores allowed'),
  organization_name: z.string().min(2, 'Name is required'),
  tier: z.enum(['Starter', 'Professional', 'Enterprise']),
});

const upgradeTierSchema = z.object({
  tier: z.enum(['Starter', 'Professional', 'Enterprise']),
});

const suspendSchema = z.object({
  reason: z.string().min(10, 'Suspension reason must be at least 10 characters long'),
  confirmWord: z.string().refine((val) => val === 'SUSPEND', {
    message: 'Type "SUSPEND" to confirm',
  }),
});

function CustomersPage() {
  const { user } = useRBAC();
  const [searchTerm, setSearchTerm] = useState('');
  const [filterStatus, setFilterStatus] = useState<'ALL' | 'Active' | 'Suspended'>('ALL');

  // Modals state
  const [activeModal, setActiveModal] = useState<'create' | 'upgrade' | 'suspend' | null>(null);
  const [selectedCustomer, setSelectedCustomer] = useState<Customer | null>(null);

  // Form states
  const [newCust, setNewCust] = useState({ organization_id: '', organization_name: '', tier: 'Starter' });
  const [upgradeData, setUpgradeData] = useState({ tier: 'Professional' });
  const [suspendData, setSuspendData] = useState({ reason: '', confirmWord: '' });
  
  const [errors, setErrors] = useState<Record<string, string>>({});

  // Query hook
  const { data: customers = [], refetch, isLoading } = useAdminQuery<Customer[]>(['customers'], '/customers');

  // Mutation hooks
  const createMutation = useAdminMutation<any, any>('/customers', 'POST', () => {
    refetch();
    setActiveModal(null);
    setNewCust({ organization_id: '', organization_name: '', tier: 'Starter' });
  });

  const updateTierMutation = useAdminMutation<{ tier: string }, any>(
    selectedCustomer ? `/customers/${selectedCustomer.id}/tier` : '/customers',
    'PUT',
    () => {
      refetch();
      setActiveModal(null);
    }
  );

  const updateStatusMutation = useAdminMutation<{ status: string }, any>(
    selectedCustomer ? `/customers/${selectedCustomer.id}/status` : '/customers',
    'PUT',
    () => {
      refetch();
      setActiveModal(null);
      setSuspendData({ reason: '', confirmWord: '' });
    }
  );

  // Search filter
  const filteredCustomers = customers.filter(c => {
    const nameMatch = c.organization_name.toLowerCase().includes(searchTerm.toLowerCase());
    const idMatch = c.organization_id.toLowerCase().includes(searchTerm.toLowerCase());
    const matchesSearch = nameMatch || idMatch;
    const matchesStatus = filterStatus === 'ALL' || c.status === filterStatus;
    return matchesSearch && matchesStatus;
  });

  // Action handlers
  const handleCreate = (e: React.FormEvent) => {
    e.preventDefault();
    setErrors({});
    const validation = createCustomerSchema.safeParse(newCust);
    if (!validation.success) {
      const fieldErrors: Record<string, string> = {};
      validation.error.errors.forEach(err => {
        if (err.path[0]) fieldErrors[err.path[0].toString()] = err.message;
      });
      setErrors(fieldErrors);
      return;
    }
    createMutation.mutate(newCust);
  };

  const handleUpgrade = (e: React.FormEvent) => {
    e.preventDefault();
    if (!selectedCustomer) return;
    setErrors({});
    const validation = upgradeTierSchema.safeParse(upgradeData);
    if (!validation.success) {
      const fieldErrors: Record<string, string> = {};
      validation.error.errors.forEach(err => {
        if (err.path[0]) fieldErrors[err.path[0].toString()] = err.message;
      });
      setErrors(fieldErrors);
      return;
    }
    updateTierMutation.mutate({ tier: upgradeData.tier });
  };

  const handleSuspend = (e: React.FormEvent) => {
    e.preventDefault();
    if (!selectedCustomer) return;
    setErrors({});
    const validation = suspendSchema.safeParse(suspendData);
    if (!validation.success) {
      const fieldErrors: Record<string, string> = {};
      validation.error.errors.forEach(err => {
        if (err.path[0]) fieldErrors[err.path[0].toString()] = err.message;
      });
      setErrors(fieldErrors);
      return;
    }
    const nextStatus = selectedCustomer.status === 'Active' ? 'Suspended' : 'Active';
    updateStatusMutation.mutate({ status: nextStatus });
  };

  return (
    <div className="space-y-6">
      <div className="flex flex-col md:flex-row md:items-center md:justify-between gap-4">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">Customer Directory</h1>
          <p className="text-xs text-on-surfaceVariant mt-1">
            Manage organization metadata, budget limits, operational bounds, and tiers.
          </p>
        </div>
        <RoleGuard allowedRoles={['Super Admin', 'Support Engineer']}>
          <button
            onClick={() => setActiveModal('create')}
            className="flex items-center space-x-2 bg-primary text-primary-foreground text-xs font-semibold px-4 py-2.5 rounded-lg hover:bg-primary/95 transition-all shadow cursor-pointer"
          >
            <Plus className="h-4 w-4" />
            <span>Create Customer</span>
          </button>
        </RoleGuard>
      </div>

      {/* Filter and Search controls */}
      <div className="bg-surface-container border border-outline-variant rounded-xl p-4 flex flex-col md:flex-row gap-4 items-center justify-between">
        <div className="relative w-full md:w-80">
          <Search className="absolute left-3 top-2.5 h-4 w-4 text-outline" />
          <input
            type="text"
            placeholder="Search by ID or Name..."
            value={searchTerm}
            onChange={(e) => setSearchTerm(e.target.value)}
            className="pl-9 pr-4 py-2 w-full text-xs bg-surface-container-low border border-outline-variant rounded-lg text-foreground focus:outline-none focus:border-primary font-mono"
          />
        </div>
        <div className="flex space-x-2 w-full md:w-auto">
          {['ALL', 'Active', 'Suspended'].map((status) => (
            <button
              key={status}
              onClick={() => setFilterStatus(status as any)}
              className={`px-3 py-1.5 rounded-lg text-xs font-semibold border transition-all cursor-pointer ${
                filterStatus === status
                  ? 'bg-primary/10 border-primary text-primary'
                  : 'bg-surface-container-high border-outline-variant text-on-surfaceVariant hover:text-foreground'
              }`}
            >
              {status}
            </button>
          ))}
        </div>
      </div>

      {/* Datatable */}
      <div className="bg-surface-container border border-outline-variant rounded-xl overflow-hidden shadow">
        <div className="overflow-x-auto">
          <table className="w-full text-left border-collapse">
            <thead>
              <tr className="bg-surface-container-high border-b border-outline-variant text-xs text-on-surfaceVariant uppercase font-mono tracking-wider font-semibold">
                <th className="px-6 py-4">Organization ID</th>
                <th className="px-6 py-4">Name</th>
                <th className="px-6 py-4">Tier</th>
                <th className="px-6 py-4">Status</th>
                <th className="px-6 py-4 text-right">Actions</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-outline-variant/40 text-xs font-mono">
              {isLoading ? (
                <tr>
                  <td colSpan={5} className="px-6 py-8 text-center text-on-surfaceVariant font-sans">
                    Loading customers database...
                  </td>
                </tr>
              ) : filteredCustomers.map((c) => (
                <tr key={c.id} className="hover:bg-surface-container-high/40 transition-colors">
                  <td className="px-6 py-4 font-bold text-primary">{c.organization_id}</td>
                  <td className="px-6 py-4 text-foreground font-sans font-medium">{c.organization_name}</td>
                  <td className="px-6 py-4">
                    <span className={`px-2 py-0.5 rounded text-[10px] font-bold ${
                      c.tier === 'Enterprise' ? 'bg-primary/10 text-primary border border-primary/20' :
                      c.tier === 'Professional' || c.tier === 'Pro' ? 'bg-secondary/10 text-secondary border border-secondary/20' :
                      'bg-surface-container-highest text-on-surfaceVariant border border-outline-variant'
                    }`}>
                      {c.tier}
                    </span>
                  </td>
                  <td className="px-6 py-4">
                    <span className={`inline-flex items-center space-x-1 px-2 py-0.5 rounded text-[10px] font-bold ${
                      c.status === 'Active' ? 'bg-green-500/10 text-green-400 border border-green-500/20' :
                      'bg-error-container/10 text-error-dim border border-error-container/20'
                    }`}>
                      <span className={`h-1.5 w-1.5 rounded-full ${c.status === 'Active' ? 'bg-green-400' : 'bg-error-dim'}`}></span>
                      <span>{c.status}</span>
                    </span>
                  </td>
                  <td className="px-6 py-4 text-right space-x-2 text-sans font-sans">
                    <RoleGuard allowedRoles={['Super Admin', 'Finance']}>
                      <button
                        onClick={() => {
                          setSelectedCustomer(c);
                          setUpgradeData({ tier: c.tier as any });
                          setActiveModal('upgrade');
                        }}
                        className="text-xs bg-surface-container-high hover:bg-surface-container-highest text-foreground border border-outline-variant px-2.5 py-1.5 rounded transition-all cursor-pointer font-semibold"
                      >
                        Adjust Tier
                      </button>
                    </RoleGuard>
                    
                    <RoleGuard allowedRoles={['Super Admin', 'Support Engineer']}>
                      <button
                        onClick={() => {
                          setSelectedCustomer(c);
                          setActiveModal('suspend');
                        }}
                        className={`text-xs px-2.5 py-1.5 rounded font-semibold border transition-all cursor-pointer ${
                          c.status === 'Active' 
                            ? 'bg-error-container/10 hover:bg-error-container/20 text-error-dim border-error-container/20'
                            : 'bg-green-500/10 hover:bg-green-500/20 text-green-400 border-green-500/20'
                        }`}
                      >
                        {c.status === 'Active' ? 'Suspend' : 'Activate'}
                      </button>
                    </RoleGuard>
                  </td>
                </tr>
              ))}
              {!isLoading && filteredCustomers.length === 0 && (
                <tr>
                  <td colSpan={5} className="px-6 py-8 text-center text-on-surfaceVariant font-sans">
                    No customers match your criteria.
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      </div>

      {/* CREATE MODAL */}
      {activeModal === 'create' && (
        <div className="fixed inset-0 bg-black/60 backdrop-blur-sm z-50 flex items-center justify-center p-4">
          <div className="bg-surface-container border border-outline-variant w-full max-w-md rounded-xl p-6 shadow-2xl space-y-6">
            <div className="flex justify-between items-center">
              <h2 className="text-sm font-semibold tracking-wide text-foreground uppercase font-mono">Create Customer Org</h2>
              <button onClick={() => setActiveModal(null)} className="text-outline hover:text-foreground">
                <X className="h-5 w-5" />
              </button>
            </div>
            <form onSubmit={handleCreate} className="space-y-4">
              <div className="space-y-1">
                <label className="text-[10px] font-semibold text-on-surfaceVariant uppercase font-mono block">Organization ID (Slug)</label>
                <input
                  type="text"
                  placeholder="org_netflix_9"
                  value={newCust.organization_id}
                  onChange={(e) => setNewCust({...newCust, organization_id: e.target.value})}
                  className="w-full bg-surface-container-low border border-outline-variant rounded px-3 py-2 text-xs text-foreground focus:outline-none focus:border-primary font-mono"
                />
                {errors.organization_id && <span className="text-[10px] text-error-dim font-mono">{errors.organization_id}</span>}
              </div>

              <div className="space-y-1">
                <label className="text-[10px] font-semibold text-on-surfaceVariant uppercase font-mono block">Organization Name</label>
                <input
                  type="text"
                  placeholder="Netflix Inc."
                  value={newCust.organization_name}
                  onChange={(e) => setNewCust({...newCust, organization_name: e.target.value})}
                  className="w-full bg-surface-container-low border border-outline-variant rounded px-3 py-2 text-xs text-foreground focus:outline-none focus:border-primary"
                />
                {errors.organization_name && <span className="text-[10px] text-error-dim font-mono">{errors.organization_name}</span>}
              </div>

              <div className="space-y-1">
                <label className="text-[10px] font-semibold text-on-surfaceVariant uppercase font-mono block">Service Tier</label>
                <select
                  value={newCust.tier}
                  onChange={(e) => setNewCust({...newCust, tier: e.target.value})}
                  className="w-full bg-surface-container-low border border-outline-variant rounded px-3 py-2 text-xs text-foreground focus:outline-none focus:border-primary cursor-pointer"
                >
                  <option value="Starter">Starter</option>
                  <option value="Professional">Professional</option>
                  <option value="Enterprise">Enterprise</option>
                </select>
              </div>

              <button
                type="submit"
                disabled={createMutation.isPending}
                className="w-full bg-primary text-primary-foreground font-semibold py-2.5 rounded text-xs hover:bg-primary/95 transition-all shadow cursor-pointer disabled:opacity-50"
              >
                {createMutation.isPending ? 'Registering Org...' : 'Submit Org Registration'}
              </button>
            </form>
          </div>
        </div>
      )}

      {/* ADJUST MODAL */}
      {activeModal === 'upgrade' && selectedCustomer && (
        <div className="fixed inset-0 bg-black/60 backdrop-blur-sm z-50 flex items-center justify-center p-4">
          <div className="bg-surface-container border border-outline-variant w-full max-w-md rounded-xl p-6 shadow-2xl space-y-6">
            <div className="flex justify-between items-center">
              <h2 className="text-sm font-semibold tracking-wide text-foreground uppercase font-mono">Upgrade Tier</h2>
              <button onClick={() => setActiveModal(null)} className="text-outline hover:text-foreground">
                <X className="h-5 w-5" />
              </button>
            </div>
            <div className="p-3 bg-surface-container-low border border-outline-variant rounded text-xs space-y-1">
              <span className="text-on-surfaceVariant block">Customer:</span>
              <strong className="text-primary font-mono">{selectedCustomer.organization_name} ({selectedCustomer.organization_id})</strong>
            </div>
            <form onSubmit={handleUpgrade} className="space-y-4">
              <div className="space-y-1">
                <label className="text-[10px] font-semibold text-on-surfaceVariant uppercase font-mono block">Target Tier</label>
                <select
                  value={upgradeData.tier}
                  onChange={(e) => setUpgradeData({...upgradeData, tier: e.target.value as any})}
                  className="w-full bg-surface-container-low border border-outline-variant rounded px-3 py-2 text-xs text-foreground focus:outline-none focus:border-primary cursor-pointer"
                >
                  <option value="Starter">Starter</option>
                  <option value="Professional">Professional</option>
                  <option value="Enterprise">Enterprise</option>
                </select>
              </div>

              <button
                type="submit"
                disabled={updateTierMutation.isPending}
                className="w-full bg-primary text-primary-foreground font-semibold py-2.5 rounded text-xs hover:bg-primary/95 transition-all shadow cursor-pointer disabled:opacity-50"
              >
                {updateTierMutation.isPending ? 'Updating...' : 'Apply Allocation Rules'}
              </button>
            </form>
          </div>
        </div>
      )}

      {/* SUSPEND MODAL */}
      {activeModal === 'suspend' && selectedCustomer && (
        <div className="fixed inset-0 bg-black/60 backdrop-blur-sm z-50 flex items-center justify-center p-4">
          <div className="bg-surface-container border border-outline-variant w-full max-w-md rounded-xl p-6 shadow-2xl space-y-6">
            <div className="flex justify-between items-center">
              <h2 className="text-sm font-semibold tracking-wide text-error-dim uppercase font-mono">
                {selectedCustomer.status === 'Active' ? 'Suspend Organization' : 'Re-Activate Organization'}
              </h2>
              <button onClick={() => setActiveModal(null)} className="text-outline hover:text-foreground">
                <X className="h-5 w-5" />
              </button>
            </div>
            
            <div className="p-3.5 bg-error-container/5 border border-error-container/20 rounded text-xs text-on-surface flex items-start space-x-3">
              <AlertTriangle className="h-5 w-5 text-error-dim shrink-0 mt-0.5" />
              <div className="space-y-1">
                <span className="font-semibold text-foreground">Operational Action Required</span>
                <p className="text-[11px] text-on-surfaceVariant leading-relaxed">
                  {selectedCustomer.status === 'Active' 
                    ? 'Suspension blocks all CQRS commands, stops event queues, and invalidates API credentials for '
                    : 'Re-activating this organization restores all command and query pipeline capabilities for '}
                  <strong className="text-primary font-mono">{selectedCustomer.organization_id}</strong>.
                </p>
              </div>
            </div>

            <form onSubmit={handleSuspend} className="space-y-4">
              <div className="space-y-1">
                <label className="text-[10px] font-semibold text-on-surfaceVariant uppercase font-mono block">Justification Reason</label>
                <textarea
                  placeholder="Justify this administrative operational block..."
                  value={suspendData.reason}
                  onChange={(e) => setSuspendData({...suspendData, reason: e.target.value})}
                  rows={3}
                  className="w-full bg-surface-container-low border border-outline-variant rounded px-3 py-2 text-xs text-foreground focus:outline-none focus:border-primary resize-none"
                />
                {errors.reason && <span className="text-[10px] text-error-dim font-mono">{errors.reason}</span>}
              </div>

              <div className="space-y-1">
                <label className="text-[10px] font-semibold text-on-surfaceVariant uppercase font-mono block">
                  Confirm validation: Type <span className="text-foreground font-bold font-mono">SUSPEND</span>
                </label>
                <input
                  type="text"
                  placeholder="SUSPEND"
                  value={suspendData.confirmWord}
                  onChange={(e) => setSuspendData({...suspendData, confirmWord: e.target.value})}
                  className="w-full bg-surface-container-low border border-outline-variant rounded px-3 py-2 text-xs text-foreground focus:outline-none focus:border-primary font-mono"
                />
                {errors.confirmWord && <span className="text-[10px] text-error-dim font-mono">{errors.confirmWord}</span>}
              </div>

              <button
                type="submit"
                disabled={updateStatusMutation.isPending}
                className={`w-full font-semibold py-2.5 rounded text-xs transition-all shadow cursor-pointer disabled:opacity-50 ${
                  selectedCustomer.status === 'Active'
                    ? 'bg-error text-error-foreground hover:bg-error/95'
                    : 'bg-primary text-primary-foreground hover:bg-primary/95'
                }`}
              >
                {updateStatusMutation.isPending ? 'Processing...' : selectedCustomer.status === 'Active' ? 'Enforce Operational Block' : 'Lift Suspension'}
              </button>
            </form>
          </div>
        </div>
      )}
    </div>
  );
}

export default withRoleProtection(CustomersPage, ['Super Admin', 'Support Engineer', 'Finance', 'Read Only']);
