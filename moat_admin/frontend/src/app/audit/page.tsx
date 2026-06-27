'use client';

import React, { useState } from 'react';
import { withRoleProtection } from '@/lib/rbac';
import { useAdminQuery } from '@/lib/api';
import { 
  ClipboardList, 
  Search, 
  Filter, 
  ShieldCheck, 
  Clock, 
  User, 
  Globe 
} from 'lucide-react';

interface AuditLog {
  id: string;
  username: string;
  action: string;
  target_type: string | null;
  target_id: string | null;
  details: string | null;
  ip_address: string | null;
  created_at: string;
}

interface AuditLogsResponse {
  logs: AuditLog[];
  total: number;
}

function AuditPage() {
  const [searchTerm, setSearchTerm] = useState('');
  const [filterOperator, setFilterOperator] = useState('ALL');

  // Query hook
  const { data, isLoading } = useAdminQuery<AuditLogsResponse>(['audit-logs'], '/audit-logs');
  const logs = data?.logs || [];

  const filteredLogs = logs.filter(log => {
    const actionMatch = log.action.toLowerCase().includes(searchTerm.toLowerCase());
    const detailsMatch = log.details?.toLowerCase().includes(searchTerm.toLowerCase()) || false;
    const targetMatch = log.target_id?.toLowerCase().includes(searchTerm.toLowerCase()) || false;
    const matchesSearch = actionMatch || detailsMatch || targetMatch;
    
    const matchesOperator = filterOperator === 'ALL' || log.username === filterOperator;
    return matchesSearch && matchesOperator;
  });

  const uniqueOperators = Array.from(new Set(logs.map(l => l.username)));

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-bold tracking-tight">Security Audit Logs</h1>
        <p className="text-xs text-on-surfaceVariant mt-1">
          Immutable audit trail logs mapping all administrative commands, tier upgrades, key revoking, and node pauses.
        </p>
      </div>

      {/* Filters and search */}
      <div className="bg-surface-container border border-outline-variant rounded-xl p-4 flex flex-col md:flex-row gap-4 items-center justify-between">
        <div className="relative w-full md:w-80">
          <Search className="absolute left-3 top-2.5 h-4 w-4 text-outline" />
          <input
            type="text"
            placeholder="Search by Action, Details or Target..."
            value={searchTerm}
            onChange={(e) => setSearchTerm(e.target.value)}
            className="pl-9 pr-4 py-2 w-full text-xs bg-surface-container-low border border-outline-variant rounded-lg text-foreground focus:outline-none focus:border-primary font-mono"
          />
        </div>
        <div className="flex items-center space-x-2 w-full md:w-auto">
          <span className="text-xs font-semibold text-on-surfaceVariant uppercase font-mono">Operator:</span>
          <select
            value={filterOperator}
            onChange={(e) => setFilterOperator(e.target.value)}
            className="bg-surface-container-high border border-outline-variant rounded px-2.5 py-1.5 text-xs font-mono text-foreground cursor-pointer focus:outline-none focus:border-primary"
          >
            <option value="ALL">ALL OPERATORS</option>
            {uniqueOperators.map(op => (
              <option key={op} value={op}>{op}</option>
            ))}
          </select>
        </div>
      </div>

      {/* Audit Log Table */}
      <div className="bg-surface-container border border-outline-variant rounded-xl overflow-hidden shadow">
        <div className="overflow-x-auto">
          <table className="w-full text-left border-collapse">
            <thead>
              <tr className="bg-surface-container-high border-b border-outline-variant text-xs text-on-surfaceVariant uppercase font-mono tracking-wider font-semibold">
                <th className="px-6 py-4">Log ID</th>
                <th className="px-6 py-4">Timestamp</th>
                <th className="px-6 py-4">Operator Actor</th>
                <th className="px-6 py-4">Administrative Action</th>
                <th className="px-6 py-4">Target Entity</th>
                <th className="px-6 py-4">Details</th>
                <th className="px-6 py-4 text-right">Client Source IP</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-outline-variant/40 text-xs font-mono">
              {isLoading ? (
                <tr>
                  <td colSpan={7} className="px-6 py-8 text-center text-on-surfaceVariant font-sans">
                    Loading security logs database...
                  </td>
                </tr>
              ) : filteredLogs.map((log) => (
                <tr key={log.id} className="hover:bg-surface-container-high/40 transition-colors">
                  <td className="px-6 py-4 text-outline font-bold truncate max-w-[120px]" title={log.id}>
                    {log.id.substring(0, 8)}...
                  </td>
                  <td className="px-6 py-4 text-foreground">
                    <span className="flex items-center space-x-1.5">
                      <Clock className="h-3.5 w-3.5 text-outline" />
                      <span>{log.created_at}</span>
                    </span>
                  </td>
                  <td className="px-6 py-4 text-primary font-bold">{log.username}</td>
                  <td className="px-6 py-4 text-foreground font-sans font-medium">{log.action}</td>
                  <td className="px-6 py-4 text-foreground font-bold">{log.target_id || 'System'}</td>
                  <td className="px-6 py-4 text-on-surfaceVariant font-sans max-w-xs truncate" title={log.details || ''}>
                    {log.details || '-'}
                  </td>
                  <td className="px-6 py-4 text-right text-on-surfaceVariant">
                    <span className="flex items-center justify-end space-x-1.5">
                      <Globe className="h-3.5 w-3.5 text-outline" />
                      <span>{log.ip_address || '127.0.0.1'}</span>
                    </span>
                  </td>
                </tr>
              ))}
              {!isLoading && filteredLogs.length === 0 && (
                <tr>
                  <td colSpan={7} className="px-6 py-8 text-center text-on-surfaceVariant font-sans">
                    No matching security logs found.
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

export default withRoleProtection(AuditPage, ['Super Admin']);
