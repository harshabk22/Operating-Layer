'use client';

import React, { useState } from 'react';
import { withRoleProtection, RoleGuard } from '@/lib/rbac';
import { useAdminQuery, useAdminMutation } from '@/lib/api';
import { 
  CreditCard, 
  TrendingUp, 
  DollarSign, 
  AlertTriangle,
  Receipt,
  Search,
  CheckCircle,
  X,
  RotateCw
} from 'lucide-react';

interface Subscription {
  id: string;
  customer_id: string;
  plan_id: string;
  status: string;
  payment_status: string;
  current_period_start: string;
  current_period_end: string;
  amount: number; // In cents
  created_at: string;
}

function BillingPage() {
  const [searchTerm, setSearchTerm] = useState('');

  // Query hook
  const { data: subscriptions = [], refetch, isLoading } = useAdminQuery<Subscription[]>(['subscriptions'], '/billing/subscriptions');

  // Manual bill check mutation
  const checkMutation = useAdminMutation<any, any>('/billing/subscriptions/check', 'POST', (data: any) => {
    refetch();
    alert(`Billing check complete. Processed active renewals. Past due count: ${data.past_due_count}`);
  });

  const handleRunBilling = () => {
    checkMutation.mutate({});
  };

  const filteredSubs = subscriptions.filter(s => 
    s.customer_id.toLowerCase().includes(searchTerm.toLowerCase()) ||
    s.plan_id.toLowerCase().includes(searchTerm.toLowerCase())
  );

  return (
    <div className="space-y-6">
      <div className="flex flex-col md:flex-row md:items-center md:justify-between gap-4">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">Billing & Subscriptions</h1>
          <p className="text-xs text-on-surfaceVariant mt-1">
            Monitor client billing logs, active packages, current periods, and simulate renewals.
          </p>
        </div>
        <RoleGuard allowedRoles={['Super Admin', 'Finance']}>
          <button
            onClick={handleRunBilling}
            disabled={checkMutation.isPending}
            className="flex items-center space-x-2 bg-primary text-primary-foreground text-xs font-semibold px-4 py-2.5 rounded-lg hover:bg-primary/95 transition-all shadow cursor-pointer disabled:opacity-50"
          >
            <RotateCw className={`h-4 w-4 ${checkMutation.isPending ? 'animate-spin' : ''}`} />
            <span>{checkMutation.isPending ? 'Processing Billing...' : 'Trigger Billing Run'}</span>
          </button>
        </RoleGuard>
      </div>

      {/* Overview Cards */}
      <div className="grid gap-6 md:grid-cols-3 text-mono">
        <div className="bg-surface-container border border-outline-variant rounded-xl p-5 space-y-2">
          <span className="text-[10px] text-on-surfaceVariant uppercase block">Active Subscriptions</span>
          <h3 className="text-2xl font-bold text-foreground">
            {subscriptions.filter(s => s.status === 'Active').length}
          </h3>
          <span className="text-[10px] text-primary block">Active across all customer nodes</span>
        </div>
        <div className="bg-surface-container border border-outline-variant rounded-xl p-5 space-y-2">
          <span className="text-[10px] text-on-surfaceVariant uppercase block">Past Due subscriptions</span>
          <h3 className="text-2xl font-bold text-error-dim">
            {subscriptions.filter(s => s.status === 'Past Due').length}
          </h3>
          <span className="text-[10px] text-error-dim block">Requires support notification</span>
        </div>
        <div className="bg-surface-container border border-outline-variant rounded-xl p-5 space-y-2">
          <span className="text-[10px] text-on-surfaceVariant uppercase block">Total recurring volume</span>
          <h3 className="text-2xl font-bold text-green-400">
            ${(subscriptions.reduce((sum, s) => sum + (s.status === 'Active' ? s.amount : 0), 0) / 100).toLocaleString()}
          </h3>
          <span className="text-[10px] text-outline block">Recurring volume in cents</span>
        </div>
      </div>

      {/* Ledger Table Search */}
      <div className="relative w-full md:w-80">
        <Search className="absolute left-3 top-2.5 h-4 w-4 text-outline" />
        <input
          type="text"
          placeholder="Filter by customer organization..."
          value={searchTerm}
          onChange={(e) => setSearchTerm(e.target.value)}
          className="pl-9 pr-4 py-2 w-full text-xs bg-surface-container-low border border-outline-variant rounded-lg text-foreground focus:outline-none focus:border-primary font-mono"
        />
      </div>

      {/* Ledger Table */}
      <div className="bg-surface-container border border-outline-variant rounded-xl overflow-hidden shadow">
        <div className="overflow-x-auto">
          <table className="w-full text-left border-collapse">
            <thead>
              <tr className="bg-surface-container-high border-b border-outline-variant text-xs text-on-surfaceVariant uppercase font-mono tracking-wider font-semibold">
                <th className="px-6 py-4">Subscription ID</th>
                <th className="px-6 py-4">Customer ID</th>
                <th className="px-6 py-4">Plan Name</th>
                <th className="px-6 py-4">Billing Rate</th>
                <th className="px-6 py-4">Period Start</th>
                <th className="px-6 py-4">Period End</th>
                <th className="px-6 py-4">Payment Status</th>
                <th className="px-6 py-4">Status</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-outline-variant/40 text-xs font-mono">
              {isLoading ? (
                <tr>
                  <td colSpan={8} className="px-6 py-8 text-center text-on-surfaceVariant font-sans">
                    Loading billing subscriptions...
                  </td>
                </tr>
              ) : filteredSubs.map((s) => (
                <tr key={s.id} className="hover:bg-surface-container-high/40 transition-colors">
                  <td className="px-6 py-4 text-primary font-bold">{s.id}</td>
                  <td className="px-6 py-4 text-foreground font-semibold">{s.customer_id}</td>
                  <td className="px-6 py-4 text-foreground font-bold">{s.plan_id}</td>
                  <td className="px-6 py-4 text-foreground">${(s.amount / 100).toFixed(2)}</td>
                  <td className="px-6 py-4 text-on-surfaceVariant">{s.current_period_start}</td>
                  <td className="px-6 py-4 text-on-surfaceVariant font-bold">{s.current_period_end}</td>
                  <td className="px-6 py-4">
                    <span className={`inline-flex items-center px-2 py-0.5 rounded text-[10px] font-bold ${
                      s.payment_status === 'Valid' ? 'bg-green-500/10 text-green-400 border border-green-500/20' :
                      s.payment_status === 'SimulateFailure' ? 'bg-yellow-500/10 text-yellow-400 border border-yellow-500/20' :
                      'bg-error-container/10 text-error-dim border border-error-container/20'
                    }`}>
                      {s.payment_status}
                    </span>
                  </td>
                  <td className="px-6 py-4">
                    <span className={`inline-flex items-center px-2.5 py-0.5 rounded text-[10px] font-bold ${
                      s.status === 'Active' ? 'bg-green-500/10 text-green-400 border border-green-500/20' :
                      'bg-error-container/10 text-error-dim border border-error-container/20'
                    }`}>
                      <span>{s.status}</span>
                    </span>
                  </td>
                </tr>
              ))}
              {!isLoading && filteredSubs.length === 0 && (
                <tr>
                  <td colSpan={8} className="px-6 py-8 text-center text-on-surfaceVariant font-sans">
                    No subscriptions found.
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  );
}

export default withRoleProtection(BillingPage, ['Super Admin', 'Finance']);
