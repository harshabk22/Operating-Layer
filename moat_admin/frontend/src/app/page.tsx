'use client';

import React from 'react';
import { useRBAC } from '@/lib/rbac';
import { useAdminQuery } from '@/lib/api';
import { 
  Users, 
  Cpu, 
  ShieldAlert, 
  Key, 
  Activity, 
  TrendingUp,
  Package,
  Layers
} from 'lucide-react';
import Link from 'next/link';

interface Incident {
  id: string;
  runtime_id: string;
  incident_type: string;
  severity: string;
  summary: string;
  status: string;
  reported_at: string;
}

export default function DashboardPage() {
  const { user } = useRBAC();

  // Load live database stats
  const { data: customers = [] } = useAdminQuery<any[]>(['customers'], '/customers');
  const { data: runtimes = [] } = useAdminQuery<any[]>(['runtimes'], '/runtimes');
  const { data: pending = [] } = useAdminQuery<any[]>(['pending-packages'], '/packages/pending');
  const { data: incidents = [] } = useAdminQuery<Incident[]>(['incidents'], '/incidents');

  // Filter open incidents
  const openIncidents = incidents.filter(i => i.status === 'Open');

  return (
    <div className="space-y-8">
      {/* Welcome Banner */}
      <div className="bg-surface-container-high border border-outline-variant rounded-xl p-6 flex flex-col md:flex-row md:items-center md:justify-between gap-4">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">System Status Dashboard</h1>
          <p className="text-sm text-on-surfaceVariant mt-1">
            Logged in as <strong className="text-primary font-mono">{user?.username}</strong> ({user?.role})
          </p>
        </div>
        <div className="flex gap-2">
          <Link href="/analytics" className="px-4 py-2 bg-primary/10 hover:bg-primary/15 text-primary text-xs font-semibold rounded-lg border border-primary/20 transition-all">
            System Performance
          </Link>
          <Link href="/audit" className="px-4 py-2 bg-surface-container-highest hover:bg-surface-container-high text-on-surface text-xs font-semibold rounded-lg border border-outline-variant transition-all">
            Security Logs
          </Link>
        </div>
      </div>

      {/* KPI Cards Grid */}
      <div className="grid gap-6 md:grid-cols-2 lg:grid-cols-4 font-mono text-xs">
        {/* Organizations / Customers */}
        <div className="bg-surface-container border border-outline-variant rounded-xl p-6 space-y-4">
          <div className="flex justify-between items-start">
            <span className="text-xs font-semibold text-on-surfaceVariant uppercase tracking-wider">Organizations</span>
            <Users className="h-5 w-5 text-primary" />
          </div>
          <div>
            <h3 className="text-3xl font-bold font-mono tracking-tight text-foreground">{customers.length}</h3>
            <p className="text-[10px] text-green-400 mt-1 flex items-center space-x-1 font-sans">
              <span>Managed tenant domains</span>
            </p>
          </div>
        </div>

        {/* Runtimes */}
        <div className="bg-surface-container border border-outline-variant rounded-xl p-6 space-y-4">
          <div className="flex justify-between items-start">
            <span className="text-xs font-semibold text-on-surfaceVariant uppercase tracking-wider">Active Runtimes</span>
            <Cpu className="h-5 w-5 text-primary" />
          </div>
          <div>
            <h3 className="text-3xl font-bold font-mono tracking-tight text-foreground">
              {runtimes.filter(r => r.status === 'Active' || r.status === 'Online').length}
            </h3>
            <p className="text-[10px] text-on-surfaceVariant mt-1 font-sans">
              Cluster nodes executing WASM events
            </p>
          </div>
        </div>

        {/* Pending Approvals */}
        <div className="bg-surface-container border border-outline-variant rounded-xl p-6 space-y-4">
          <div className="flex justify-between items-start">
            <span className="text-xs font-semibold text-on-surfaceVariant uppercase tracking-wider">Pending Approvals</span>
            <ShieldAlert className="h-5 w-5 text-error" />
          </div>
          <div>
            <h3 className="text-3xl font-bold font-mono tracking-tight text-foreground">{pending.length}</h3>
            <p className="text-[10px] text-error-dim mt-1 flex items-center space-x-1 font-sans">
              <span>Requires Administrator action</span>
            </p>
          </div>
        </div>

        {/* Credentials / Keys */}
        <div className="bg-surface-container border border-outline-variant rounded-xl p-6 space-y-4">
          <div className="flex justify-between items-start">
            <span className="text-xs font-semibold text-on-surfaceVariant uppercase tracking-wider">Active Keys</span>
            <Key className="h-5 w-5 text-primary" />
          </div>
          <div>
            <h3 className="text-3xl font-bold font-mono tracking-tight text-foreground">{runtimes.length}</h3>
            <p className="text-[10px] text-green-400 mt-1 font-sans">
              Compliance rate: <span className="font-semibold font-mono">100%</span>
            </p>
          </div>
        </div>
      </div>

      {/* Main Panel layout */}
      <div className="grid gap-6 lg:grid-cols-3">
        {/* Core Event Sourcing Pipeline Status */}
        <div className="bg-surface-container border border-outline-variant rounded-xl p-6 lg:col-span-2 space-y-6">
          <div className="flex items-center justify-between">
            <h2 className="text-sm font-semibold tracking-wide text-foreground uppercase font-mono">
              Event Store Throughput
            </h2>
            <div className="flex items-center space-x-2 text-[10px] font-mono text-on-surfaceVariant bg-surface-container-low px-2.5 py-1 rounded border border-outline-variant">
              <span className="h-2 w-2 rounded-full bg-green-400 animate-pulse"></span>
              <span>182 EPS</span>
            </div>
          </div>

          {/* Simple Premium Custom Visual Chart */}
          <div className="h-48 flex items-end justify-between gap-1.5 pt-4">
            {[45, 52, 60, 48, 65, 80, 75, 90, 85, 110, 125, 95, 115, 130, 145, 120, 150, 172, 160, 182].map((value, idx) => (
              <div key={idx} className="flex-1 flex flex-col items-center group relative h-full justify-end">
                {/* Tooltip */}
                <span className="absolute bottom-full mb-1 scale-0 transition-all rounded bg-surface-container-highest border border-outline-variant px-1.5 py-0.5 text-[10px] font-mono text-primary group-hover:scale-100 whitespace-nowrap z-10">
                  {value} EPS
                </span>
                {/* Bar */}
                <div 
                  style={{ height: `${(value / 200) * 100}%` }}
                  className="w-full rounded-t bg-primary/20 group-hover:bg-primary transition-all duration-150 border-t border-primary/40"
                ></div>
                {/* Time Indicator */}
                <span className="text-[8px] text-outline mt-1.5 font-mono select-none">
                  {idx * 3}m
                </span>
              </div>
            ))}
          </div>

          <div className="grid grid-cols-3 gap-4 pt-4 border-t border-outline-variant">
            <div className="text-center">
              <span className="text-[10px] text-on-surfaceVariant uppercase font-mono block">Events Logged</span>
              <span className="text-lg font-bold font-mono text-foreground">12,489,182</span>
            </div>
            <div className="text-center">
              <span className="text-[10px] text-on-surfaceVariant uppercase font-mono block">Replay Latency</span>
              <span className="text-lg font-bold font-mono text-foreground">0.8ms</span>
            </div>
            <div className="text-center">
              <span className="text-[10px] text-on-surfaceVariant uppercase font-mono block">Snapshots built</span>
              <span className="text-lg font-bold font-mono text-foreground">1,829</span>
            </div>
          </div>
        </div>

        {/* Active Incident Supervisor Feed */}
        <div className="bg-surface-container border border-outline-variant rounded-xl p-6 space-y-6">
          <div className="flex items-center justify-between">
            <h2 className="text-sm font-semibold tracking-wide text-foreground uppercase font-mono">
              Operational Supervisor
            </h2>
            <span className="px-2 py-0.5 rounded text-[10px] font-mono font-bold bg-error-container/10 text-error-dim border border-error-container/20">
              {openIncidents.length} Open Incidents
            </span>
          </div>

          <div className="space-y-4">
            {openIncidents.map(inc => (
              <div key={inc.id} className="p-3.5 bg-error-container/5 border border-error-container/20 rounded-lg space-y-2">
                <div className="flex justify-between items-center">
                  <span className="text-xs font-semibold text-error-dim font-mono">{inc.id.substring(0, 8)}</span>
                  <span className="text-[9px] font-mono text-outline">{inc.reported_at}</span>
                </div>
                <p className="text-xs text-on-surface font-sans">
                  {inc.summary} on runtime <strong className="text-primary font-mono">{inc.runtime_id}</strong>.
                </p>
              </div>
            ))}
            {openIncidents.length === 0 && (
              <div className="p-3.5 bg-surface-container-high border border-outline-variant rounded-lg space-y-2">
                <div className="flex justify-between items-center">
                  <span className="text-xs font-semibold text-on-surfaceVariant font-mono font-bold">SYS-OK</span>
                </div>
                <p className="text-xs text-on-surfaceVariant font-sans">
                  All systems operating normally. No active incidents detected.
                </p>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
