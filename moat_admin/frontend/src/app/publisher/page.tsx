'use client';

import React, { useState } from 'react';
import { withRoleProtection } from '@/lib/rbac';
import { useAdminQuery, useAdminMutation } from '@/lib/api';
import { z } from 'zod';
import { 
  UploadCloud, 
  Layers, 
  FileCode, 
  CheckCircle,
  X,
  RefreshCw,
  Info
} from 'lucide-react';

interface Package {
  id: string;
  name: string;
  latest_version: string | null;
  status: string;
}

const uploadVersionSchema = z.object({
  packageId: z.string().min(1, 'Select a target package'),
  version: z.string().regex(/^\d+\.\d+\.\d+(-[a-zA-Z0-9.]+)?$/, 'Must be valid SemVer (e.g. 1.2.3)'),
  checksum_sha256: z.string().regex(/^[a-fA-F0-9]{64}$/, 'Must be a 64-character SHA-256 hex checksum'),
  signature: z.string().min(1, 'Ed25519 signature is required'),
  binary_url: z.string().url('Must be a valid binary URL'),
});

function PublisherPage() {
  const [activeModal, setActiveModal] = useState(false);
  const [uploadData, setUploadData] = useState({ 
    packageId: '', 
    version: '', 
    checksum_sha256: '', 
    signature: '', 
    binary_url: '' 
  });
  const [errors, setErrors] = useState<Record<string, string>>({});

  // Query packages to select
  const { data: packages = [], isLoading: isLoadingPkgs } = useAdminQuery<Package[]>(['packages'], '/packages');

  // Mutation to create version
  const createVersionMutation = useAdminMutation<any, any>(
    '/packages', // fallback URL
    'POST',
    () => {
      alert('Version bundle uploaded and pending review.');
      setActiveModal(false);
      setUploadData({ packageId: '', version: '', checksum_sha256: '', signature: '', binary_url: '' });
    }
  );

  const handleUpload = (e: React.FormEvent) => {
    e.preventDefault();
    setErrors({});
    const validation = uploadVersionSchema.safeParse(uploadData);
    if (!validation.success) {
      const fieldErrors: Record<string, string> = {};
      validation.error.errors.forEach(err => {
        if (err.path[0]) fieldErrors[err.path[0].toString()] = err.message;
      });
      setErrors(fieldErrors);
      return;
    }

    createVersionMutation.mutateAsync({
      url: `/packages/${uploadData.packageId}/versions`,
      method: 'POST',
      data: {
        version: uploadData.version,
        checksum_sha256: uploadData.checksum_sha256,
        signature: uploadData.signature,
        binary_url: uploadData.binary_url
      }
    } as any).catch(err => {
      alert(`Upload failed: ${err.response?.data?.error || err.message}`);
    });
  };

  return (
    <div className="space-y-6">
      <div className="flex flex-col md:flex-row md:items-center md:justify-between gap-4">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">Publisher Console</h1>
          <p className="text-xs text-on-surfaceVariant mt-1">
            Build, upload, and register compiled Rust WASM capability release artifacts.
          </p>
        </div>
        <button
          onClick={() => {
            if (packages.length > 0) {
              setUploadData(prev => ({ ...prev, packageId: packages[0]?.id || '' }));
            }
            setActiveModal(true);
          }}
          className="flex items-center space-x-2 bg-primary text-primary-foreground text-xs font-semibold px-4 py-2.5 rounded-lg hover:bg-primary/95 transition-all shadow cursor-pointer"
        >
          <UploadCloud className="h-4 w-4" />
          <span>Upload Version Bundle</span>
        </button>
      </div>

      {/* Info warning banner */}
      <div className="bg-surface-container-high border border-outline-variant p-4 rounded-xl flex items-start space-x-3">
        <Info className="h-5 w-5 text-primary shrink-0 mt-0.5" />
        <div className="text-xs leading-relaxed space-y-1">
          <strong className="text-foreground">WASM Artifact Verification sandbox</strong>
          <p className="text-on-surfaceVariant">
            All uploaded Rust crates are verified within our sandboxed WASM runtime. The platform enforces signature checks and verifies the SHA-256 package checksum integrity before dispatching approval tasks to Super Admins.
          </p>
        </div>
      </div>

      {/* Packages summary list */}
      <div className="bg-surface-container border border-outline-variant rounded-xl overflow-hidden shadow font-mono text-xs">
        <div className="p-4 bg-surface-container-high border-b border-outline-variant uppercase tracking-wider font-semibold text-on-surfaceVariant">
          Active Packages Catalog
        </div>
        <div className="divide-y divide-outline-variant/40">
          {isLoadingPkgs ? (
            <div className="p-6 text-center text-on-surfaceVariant">Loading catalog...</div>
          ) : packages.map(p => (
            <div key={p.id} className="p-4 flex justify-between items-center hover:bg-surface-container-high/20">
              <div>
                <span className="font-bold text-foreground">{p.name}</span>
                <span className="text-[10px] text-outline block">{p.id}</span>
              </div>
              <div className="text-right">
                <span className="font-bold text-primary block">v{p.latest_version || 'None'}</span>
                <span className="text-[10px] text-on-surfaceVariant block">Status: {p.status}</span>
              </div>
            </div>
          ))}
          {!isLoadingPkgs && packages.length === 0 && (
            <div className="p-6 text-center text-on-surfaceVariant">No packages cataloged.</div>
          )}
        </div>
      </div>

      {/* UPLOAD MODAL */}
      {activeModal && (
        <div className="fixed inset-0 bg-black/60 backdrop-blur-sm z-50 flex items-center justify-center p-4">
          <div className="bg-surface-container border border-outline-variant w-full max-w-md rounded-xl p-6 shadow-2xl space-y-6">
            <div className="flex justify-between items-center">
              <h2 className="text-sm font-semibold tracking-wide text-foreground uppercase font-mono">Upload Package Release</h2>
              <button onClick={() => setActiveModal(false)} className="text-outline hover:text-foreground">
                <X className="h-5 w-5" />
              </button>
            </div>
            <form onSubmit={handleUpload} className="space-y-4">
              <div className="space-y-1">
                <label className="text-[10px] font-semibold text-on-surfaceVariant uppercase font-mono block">Target Package</label>
                <select
                  value={uploadData.packageId}
                  onChange={(e) => setUploadData({...uploadData, packageId: e.target.value})}
                  className="w-full bg-surface-container-low border border-outline-variant rounded px-3 py-2 text-xs text-foreground focus:outline-none focus:border-primary cursor-pointer font-mono"
                >
                  {packages.map(p => (
                    <option key={p.id} value={p.id}>{p.name} ({p.id})</option>
                  ))}
                </select>
              </div>

              <div className="space-y-1">
                <label className="text-[10px] font-semibold text-on-surfaceVariant uppercase font-mono block">Release Version (SemVer)</label>
                <input
                  type="text"
                  placeholder="1.2.5"
                  value={uploadData.version}
                  onChange={(e) => setUploadData({...uploadData, version: e.target.value})}
                  className="w-full bg-surface-container-low border border-outline-variant rounded px-3 py-2 text-xs text-foreground focus:outline-none focus:border-primary font-mono"
                />
                {errors.version && <span className="text-[10px] text-error-dim font-mono">{errors.version}</span>}
              </div>

              <div className="space-y-1">
                <label className="text-[10px] font-semibold text-on-surfaceVariant uppercase font-mono block">SHA-256 Hex Checksum</label>
                <input
                  type="text"
                  placeholder="6a89c0b11fe287aa0921cb9118e47fbc..."
                  value={uploadData.checksum_sha256}
                  onChange={(e) => setUploadData({...uploadData, checksum_sha256: e.target.value})}
                  className="w-full bg-surface-container-low border border-outline-variant rounded px-3 py-2 text-xs text-foreground focus:outline-none focus:border-primary font-mono text-[10px]"
                />
                {errors.checksum_sha256 && <span className="text-[10px] text-error-dim font-mono">{errors.checksum_sha256}</span>}
              </div>

              <div className="space-y-1">
                <label className="text-[10px] font-semibold text-on-surfaceVariant uppercase font-mono block">Ed25519 Hex Signature</label>
                <input
                  type="text"
                  placeholder="Hex signature of checksum..."
                  value={uploadData.signature}
                  onChange={(e) => setUploadData({...uploadData, signature: e.target.value})}
                  className="w-full bg-surface-container-low border border-outline-variant rounded px-3 py-2 text-xs text-foreground focus:outline-none focus:border-primary font-mono text-[10px]"
                />
                {errors.signature && <span className="text-[10px] text-error-dim font-mono">{errors.signature}</span>}
              </div>

              <div className="space-y-1">
                <label className="text-[10px] font-semibold text-on-surfaceVariant uppercase font-mono block">Binary URL</label>
                <input
                  type="text"
                  placeholder="https://storage.moatageex.com/binaries/..."
                  value={uploadData.binary_url}
                  onChange={(e) => setUploadData({...uploadData, binary_url: e.target.value})}
                  className="w-full bg-surface-container-low border border-outline-variant rounded px-3 py-2 text-xs text-foreground focus:outline-none focus:border-primary font-mono text-[10px]"
                />
                {errors.binary_url && <span className="text-[10px] text-error-dim font-mono">{errors.binary_url}</span>}
              </div>

              <button
                type="submit"
                disabled={createVersionMutation.isPending}
                className="w-full bg-primary text-primary-foreground font-semibold py-2.5 rounded text-xs hover:bg-primary/95 transition-all shadow cursor-pointer disabled:opacity-50 flex items-center justify-center space-x-2"
              >
                {createVersionMutation.isPending ? (
                  <>
                    <RefreshCw className="h-4.5 w-4.5 animate-spin" />
                    <span>Uploading Crate...</span>
                  </>
                ) : (
                  <span>Submit Crate release</span>
                )}
              </button>
            </form>
          </div>
        </div>
      )}
    </div>
  );
}

export default withRoleProtection(PublisherPage, ['Super Admin', 'Developer']);
