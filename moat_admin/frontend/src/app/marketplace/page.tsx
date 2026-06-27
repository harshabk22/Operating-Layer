'use client';

import React, { useState } from 'react';
import { withRoleProtection, RoleGuard } from '@/lib/rbac';
import { useAdminQuery, useAdminMutation } from '@/lib/api';
import { z } from 'zod';
import { 
  ShoppingBag, 
  Download, 
  Star, 
  ShieldCheck, 
  ExternalLink,
  Plus,
  X,
  Sparkles
} from 'lucide-react';

interface Package {
  id: string;
  name: string;
  description: string | null;
  latest_version: string | null;
  status: string;
  developer_id: string | null;
  signature_pubkey: string;
  created_at: string;
}

const registerPackageSchema = z.object({
  name: z.string().min(2, 'Name is required'),
  description: z.string().min(10, 'Provide a description of at least 10 chars'),
  signature_pubkey: z.string().length(64, 'Signature public key must be a 64-char Hex string'),
});

function MarketplacePage() {
  const [activeModal, setActiveModal] = useState(false);
  const [regData, setRegData] = useState({ name: '', description: '', signature_pubkey: '' });
  const [errors, setErrors] = useState<Record<string, string>>({});

  // Query hook
  const { data: packages = [], refetch, isLoading } = useAdminQuery<Package[]>(['packages'], '/packages');

  // Mutation hook
  const registerMutation = useAdminMutation<any, any>('/packages', 'POST', () => {
    refetch();
    setActiveModal(false);
    setRegData({ name: '', description: '', signature_pubkey: '' });
  });

  const handleRegister = (e: React.FormEvent) => {
    e.preventDefault();
    setErrors({});
    const validation = registerPackageSchema.safeParse(regData);
    if (!validation.success) {
      const fieldErrors: Record<string, string> = {};
      validation.error.errors.forEach(err => {
        if (err.path[0]) fieldErrors[err.path[0].toString()] = err.message;
      });
      setErrors(fieldErrors);
      return;
    }
    registerMutation.mutate(regData);
  };

  return (
    <div className="space-y-6">
      <div className="flex flex-col md:flex-row md:items-center md:justify-between gap-4">
        <div>
          <h1 className="text-2xl font-bold tracking-tight font-sans">Marketplace Registry</h1>
          <p className="text-xs text-on-surfaceVariant mt-1">
            Discover, configure, and install isolated capability modules for your tenants.
          </p>
        </div>
        <RoleGuard allowedRoles={['Super Admin', 'Developer']}>
          <button
            onClick={() => setActiveModal(true)}
            className="flex items-center space-x-2 bg-primary text-primary-foreground text-xs font-semibold px-4 py-2.5 rounded-lg hover:bg-primary/95 transition-all shadow cursor-pointer"
          >
            <Plus className="h-4 w-4" />
            <span>Register Package</span>
          </button>
        </RoleGuard>
      </div>

      {/* Package grid */}
      <div className="grid gap-6 md:grid-cols-2">
        {isLoading ? (
          <div className="col-span-2 text-center text-on-surfaceVariant py-8 font-mono text-xs">
            Querying Marketplace registry...
          </div>
        ) : packages.map((p) => (
          <div key={p.id} className="bg-surface-container border border-outline-variant rounded-xl p-6 flex flex-col justify-between hover:border-outline transition-all duration-200">
            <div className="space-y-4">
              <div className="flex justify-between items-start">
                <div className="space-y-1">
                  <h3 className="text-base font-bold text-foreground">{p.name}</h3>
                  <span className="text-[10px] text-primary font-mono block">Package ID: {p.id}</span>
                </div>
                <span className={`px-2 py-0.5 rounded text-[10px] font-bold ${
                  p.status === 'Approved' 
                    ? 'bg-green-500/10 text-green-400 border border-green-500/20' 
                    : p.status === 'Rejected'
                    ? 'bg-red-500/10 text-red-400 border border-red-500/20'
                    : 'bg-surface-container-high text-on-surfaceVariant border border-outline-variant'
                }`}>
                  {p.status}
                </span>
              </div>

              <p className="text-xs text-on-surfaceVariant leading-relaxed">{p.description}</p>

              <div className="flex items-center space-x-4 text-[10px] font-mono text-outline">
                <span className="flex items-center space-x-1">
                  <ShieldCheck className="h-3.5 w-3.5" />
                  <span>Developer: {p.developer_id || 'System'}</span>
                </span>
              </div>
            </div>

            <div className="flex items-center justify-between border-t border-outline-variant/40 pt-4 mt-6">
              <span className="text-xs font-semibold font-mono text-foreground">
                {p.latest_version ? `v${p.latest_version}` : 'No versions approved'}
              </span>
            </div>
          </div>
        ))}
        {!isLoading && packages.length === 0 && (
          <div className="col-span-2 text-center text-on-surfaceVariant py-8 font-mono text-xs">
            No packages registered in the marketplace.
          </div>
        )}
      </div>

      {/* REGISTER MODAL */}
      {activeModal && (
        <div className="fixed inset-0 bg-black/60 backdrop-blur-sm z-50 flex items-center justify-center p-4">
          <div className="bg-surface-container border border-outline-variant w-full max-w-md rounded-xl p-6 shadow-2xl space-y-6">
            <div className="flex justify-between items-center">
              <h2 className="text-sm font-semibold tracking-wide text-foreground uppercase font-mono">Register New Package</h2>
              <button onClick={() => setActiveModal(false)} className="text-outline hover:text-foreground">
                <X className="h-5 w-5" />
              </button>
            </div>
            <form onSubmit={handleRegister} className="space-y-4">
              <div className="space-y-1">
                <label className="text-[10px] font-semibold text-on-surfaceVariant uppercase font-mono block">Package Display Name</label>
                <input
                  type="text"
                  placeholder="PDF Invoice Generator"
                  value={regData.name}
                  onChange={(e) => setRegData({...regData, name: e.target.value})}
                  className="w-full bg-surface-container-low border border-outline-variant rounded px-3 py-2 text-xs text-foreground focus:outline-none focus:border-primary"
                />
                {errors.name && <span className="text-[10px] text-error-dim font-mono">{errors.name}</span>}
              </div>

              <div className="space-y-1">
                <label className="text-[10px] font-semibold text-on-surfaceVariant uppercase font-mono block">Description</label>
                <textarea
                  placeholder="Explain what capabilities this package installs into the tenant's context..."
                  value={regData.description}
                  onChange={(e) => setRegData({...regData, description: e.target.value})}
                  rows={3}
                  className="w-full bg-surface-container-low border border-outline-variant rounded px-3 py-2 text-xs text-foreground focus:outline-none focus:border-primary resize-none"
                />
                {errors.description && <span className="text-[10px] text-error-dim font-mono">{errors.description}</span>}
              </div>

              <div className="space-y-1">
                <label className="text-[10px] font-semibold text-on-surfaceVariant uppercase font-mono block">Ed25519 Public Key (Hex)</label>
                <input
                  type="text"
                  value={regData.signature_pubkey}
                  onChange={(e) => setRegData({...regData, signature_pubkey: e.target.value})}
                  className="w-full bg-surface-container-low border border-outline-variant rounded px-3 py-2 text-xs text-foreground focus:outline-none focus:border-primary font-mono text-[10px]"
                />
                {errors.signature_pubkey && <span className="text-[10px] text-error-dim font-mono">{errors.signature_pubkey}</span>}
              </div>

              <button
                type="submit"
                disabled={registerMutation.isPending}
                className="w-full bg-primary text-primary-foreground font-semibold py-2.5 rounded text-xs hover:bg-primary/95 transition-all shadow cursor-pointer disabled:opacity-50"
              >
                {registerMutation.isPending ? 'Registering...' : 'Register Module Crate'}
              </button>
            </form>
          </div>
        </div>
      )}
    </div>
  );
}

export default withRoleProtection(MarketplacePage, ['Super Admin', 'Developer', 'Support Engineer', 'Finance', 'Read Only']);
