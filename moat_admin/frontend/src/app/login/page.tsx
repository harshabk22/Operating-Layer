'use client';

import React, { useState } from 'react';
import { useRBAC } from '@/lib/rbac';

export default function LoginPage() {
  const { login } = useRBAC();
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [isSubmitting, setIsSubmitting] = useState(false);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (username.trim() && password.trim()) {
      setIsSubmitting(true);
      try {
        await login(username, password);
      } catch (err) {
        // Error already alerted in login hook
      } finally {
        setIsSubmitting(false);
      }
    }
  };

  return (
    <div className="min-h-screen flex items-center justify-center bg-background px-4">
      <div className="w-full max-w-md bg-surface-container border border-outline-variant rounded-xl p-8 shadow-2xl">
        <div className="text-center space-y-2 mb-8">
          <div className="inline-flex w-12 h-12 rounded bg-primary items-center justify-center font-bold text-primary-foreground text-xl shadow-lg shadow-primary/20">
            Ω
          </div>
          <h1 className="text-2xl font-bold tracking-tight text-foreground">MoatAgeex Control Plane</h1>
          <p className="text-xs text-on-surfaceVariant leading-relaxed">
            Authorized Personnel Administrative Console
          </p>
        </div>

        <form onSubmit={handleSubmit} className="space-y-6">
          <div className="space-y-2">
            <label className="text-xs font-semibold text-on-surfaceVariant uppercase tracking-wider block">
              Username
            </label>
            <input
              type="text"
              value={username}
              onChange={(e) => setUsername(e.target.value)}
              className="w-full bg-surface-container-low border border-outline-variant rounded-lg px-4 py-3 text-sm text-foreground focus:outline-none focus:border-primary transition-colors font-mono"
              placeholder="operator_1"
              required
            />
          </div>

          <div className="space-y-2">
            <label className="text-xs font-semibold text-on-surfaceVariant uppercase tracking-wider block">
              Password
            </label>
            <input
              type="password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              className="w-full bg-surface-container-low border border-outline-variant rounded-lg px-4 py-3 text-sm text-foreground focus:outline-none focus:border-primary transition-colors font-mono"
              placeholder="••••••••"
              required
            />
          </div>

          <button
            type="submit"
            disabled={isSubmitting}
            className="w-full bg-primary text-primary-foreground font-semibold rounded-lg py-3 hover:bg-primary/95 transition-all text-sm shadow-md active:scale-[0.99] cursor-pointer disabled:opacity-50"
          >
            {isSubmitting ? 'Authenticating...' : 'Authenticate & Open Console'}
          </button>
        </form>

        <div className="mt-8 pt-6 border-t border-outline-variant/50 text-center text-[10px] text-outline font-mono">
          SECURE CONNECTION AES-256-GCM
        </div>
      </div>
    </div>
  );
}
