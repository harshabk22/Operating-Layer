import { useState } from 'react';
import { useData, useMutateData } from '@/lib/api';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import {
  Activity,
  Database,
  Users,
  Tag,
  Layers,
  Clock,
  AlertTriangle,
  Mail,
  RefreshCw,
  Briefcase,
  Send,
  Shield,
  Radio,
} from 'lucide-react';

type Tab = 'health' | 'jobs' | 'outbox' | 'audit' | 'events';

function StatusBadge({ status }: { status: string }) {
  const normalized = status?.toLowerCase() ?? '';
  let classes = 'text-xs font-medium px-2.5 py-1 rounded-full border ';
  if (normalized === 'completed' || normalized === 'sent' || normalized === 'healthy') {
    classes += 'bg-green-500/10 text-green-500 border-green-500/20';
  } else if (normalized === 'pending' || normalized === 'queued' || normalized === 'scheduled') {
    classes += 'bg-yellow-500/10 text-yellow-500 border-yellow-500/20';
  } else if (normalized === 'failed' || normalized === 'error') {
    classes += 'bg-red-500/10 text-red-500 border-red-500/20';
  } else {
    classes += 'bg-accent text-muted-foreground border-border';
  }
  return <span className={classes}>{status}</span>;
}

function formatTimestamp(value: string | number | null | undefined): string {
  if (!value) return '—';
  const date = typeof value === 'number' ? new Date(value * 1000) : new Date(value);
  if (isNaN(date.getTime())) return '—';
  return date.toISOString().replace('T', ' ').slice(0, 19);
}

function MetricCard({
  icon,
  label,
  value,
}: {
  icon: React.ReactNode;
  label: string;
  value: string | number;
}) {
  return (
    <Card>
      <CardHeader className="pb-2">
        <CardTitle className="text-sm font-medium flex items-center gap-2">
          {icon}
          {label}
        </CardTitle>
      </CardHeader>
      <CardContent>
        <div className="text-2xl font-bold">{value}</div>
      </CardContent>
    </Card>
  );
}

export default function Observability() {
  const [activeTab, setActiveTab] = useState<Tab>('health');

  const { data: health, isLoading: healthLoading } = useData('/api/system/health');
  const { data: metrics, isLoading: metricsLoading } = useData('/api/system/metrics');
  const { data: jobs, isLoading: jobsLoading } = useData('/api/system/jobs');
  const { data: outbox, isLoading: outboxLoading } = useData('/api/system/outbox');
  const { data: audit, isLoading: auditLoading } = useData('/api/system/audit');
  const { data: events, isLoading: eventsLoading } = useData('/api/system/events');
  const rebuildMutation = useMutateData('/api/system/rebuild_projections', 'Projections rebuilt successfully');

  const tabs: { key: Tab; label: string; icon: React.ReactNode }[] = [
    { key: 'health', label: 'Health', icon: <Activity className="h-4 w-4" /> },
    { key: 'jobs', label: 'Jobs', icon: <Briefcase className="h-4 w-4" /> },
    { key: 'outbox', label: 'Outbox', icon: <Send className="h-4 w-4" /> },
    { key: 'audit', label: 'Audit Trail', icon: <Shield className="h-4 w-4" /> },
    { key: 'events', label: 'Events', icon: <Radio className="h-4 w-4" /> },
  ];

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-3xl font-bold tracking-tight">Observability</h1>
        <p className="text-muted-foreground text-sm mt-1">
          Monitor system health, background jobs, and audit activity.
        </p>
      </div>

      {/* Tab navigation */}
      <div className="flex gap-1 border-b">
        {tabs.map((tab) => (
          <button
            key={tab.key}
            onClick={() => setActiveTab(tab.key)}
            className={`flex items-center gap-2 px-4 py-2.5 text-sm font-medium transition-colors border-b-2 -mb-px ${
              activeTab === tab.key
                ? 'border-primary text-primary'
                : 'border-transparent text-muted-foreground hover:text-foreground hover:border-border'
            }`}
          >
            {tab.icon}
            {tab.label}
          </button>
        ))}
      </div>

      {/* Tab: Health */}
      {activeTab === 'health' && (
        <div className="space-y-6">
          {healthLoading || metricsLoading ? (
            <div className="p-8 text-center text-muted-foreground">Loading health data...</div>
          ) : (
            <>
              {/* Health status indicator */}
              <div className="flex items-center gap-3 border rounded-md bg-card p-4">
                <span className="relative flex h-3 w-3">
                  <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-green-400 opacity-75" />
                  <span className="relative inline-flex rounded-full h-3 w-3 bg-green-500" />
                </span>
                <span className="text-sm font-medium">
                  System Status: <span className="text-green-500">{health?.status || 'Healthy'}</span>
                </span>
              </div>

              {/* System metrics cards */}
              <div>
                <h2 className="text-sm font-semibold text-muted-foreground uppercase tracking-wider mb-3">
                  System Overview
                </h2>
                <div className="grid gap-4 md:grid-cols-2 lg:grid-cols-4">
                  <MetricCard
                    icon={<Database className="h-4 w-4 text-muted-foreground" />}
                    label="DB Size"
                    value={health?.db_size_mb ? `${health.db_size_mb} MB` : '—'}
                  />
                  <MetricCard
                    icon={<Users className="h-4 w-4 text-muted-foreground" />}
                    label="Active Sessions"
                    value={health?.active_sessions ?? 0}
                  />
                  <MetricCard
                    icon={<Tag className="h-4 w-4 text-muted-foreground" />}
                    label="Version"
                    value={health?.version || '—'}
                  />
                  <MetricCard
                    icon={<Layers className="h-4 w-4 text-muted-foreground" />}
                    label="Total Events"
                    value={health?.total_events ?? 0}
                  />
                  <MetricCard
                    icon={<Layers className="h-4 w-4 text-muted-foreground" />}
                    label="Total Projections"
                    value={health?.total_projections ?? 0}
                  />
                  <MetricCard
                    icon={<Clock className="h-4 w-4 text-muted-foreground" />}
                    label="Pending Jobs"
                    value={health?.pending_jobs ?? 0}
                  />
                  <MetricCard
                    icon={<AlertTriangle className="h-4 w-4 text-muted-foreground" />}
                    label="Failed Jobs"
                    value={health?.failed_jobs ?? 0}
                  />
                  <MetricCard
                    icon={<Mail className="h-4 w-4 text-muted-foreground" />}
                    label="Pending Outbox"
                    value={health?.pending_outbox ?? 0}
                  />
                </div>
              </div>

              {/* Reliability metrics */}
              {metrics && (
                <div>
                  <h2 className="text-sm font-semibold text-muted-foreground uppercase tracking-wider mb-3">
                    Reliability Metrics
                  </h2>
                  <div className="grid gap-4 md:grid-cols-2 lg:grid-cols-3">
                    <ReliabilityMetric
                      label="Avg Message Latency"
                      value={metrics.average_message_latency != null ? `${metrics.average_message_latency}ms` : '—'}
                    />
                    <ReliabilityMetric
                      label="Avg Booking Latency"
                      value={metrics.average_booking_latency != null ? `${metrics.average_booking_latency}ms` : '—'}
                    />
                    <ReliabilityMetric
                      label="Calendar Failures"
                      value={metrics.calendar_failures ?? 0}
                      warn={(metrics.calendar_failures ?? 0) > 0}
                    />
                    <ReliabilityMetric
                      label="Sheets Failures"
                      value={metrics.sheets_failures ?? 0}
                      warn={(metrics.sheets_failures ?? 0) > 0}
                    />
                    <ReliabilityMetric
                      label="Notification Failures"
                      value={metrics.notification_failures ?? 0}
                      warn={(metrics.notification_failures ?? 0) > 0}
                    />
                    <ReliabilityMetric
                      label="Total Retries"
                      value={metrics.total_retry_count ?? 0}
                    />
                  </div>
                </div>
              )}
            </>
          )}
        </div>
      )}

      {/* Tab: Jobs */}
      {activeTab === 'jobs' && (
        <div className="border rounded-md bg-card">
          <h2 className="p-4 font-semibold border-b">Background Jobs</h2>
          {jobsLoading ? (
            <div className="p-8 text-center text-muted-foreground">Loading jobs...</div>
          ) : !jobs?.length ? (
            <div className="p-8 text-center text-muted-foreground">No background jobs found.</div>
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full text-sm">
                <thead>
                  <tr className="border-b bg-accent/30">
                    <th className="text-left p-3 font-medium text-muted-foreground">Type</th>
                    <th className="text-left p-3 font-medium text-muted-foreground">Status</th>
                    <th className="text-left p-3 font-medium text-muted-foreground">Retries</th>
                    <th className="text-left p-3 font-medium text-muted-foreground">Scheduled</th>
                    <th className="text-left p-3 font-medium text-muted-foreground">Completed</th>
                    <th className="text-left p-3 font-medium text-muted-foreground">Error</th>
                  </tr>
                </thead>
                <tbody>
                  {jobs.map((job: any) => (
                    <tr key={job.id} className="border-b border-border/50 hover:bg-accent/20">
                      <td className="p-3 font-medium">{job.job_type}</td>
                      <td className="p-3">
                        <StatusBadge status={job.status} />
                      </td>
                      <td className="p-3 text-muted-foreground">{job.retry_count}</td>
                      <td className="p-3 text-muted-foreground font-mono text-xs">
                        {formatTimestamp(job.scheduled_at)}
                      </td>
                      <td className="p-3 text-muted-foreground font-mono text-xs">
                        {formatTimestamp(job.completed_at)}
                      </td>
                      <td className="p-3 text-red-400 text-xs max-w-[200px] truncate" title={job.error_message || ''}>
                        {job.error_message || '—'}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </div>
      )}

      {/* Tab: Outbox */}
      {activeTab === 'outbox' && (
        <div className="border rounded-md bg-card">
          <h2 className="p-4 font-semibold border-b">Outbox Messages</h2>
          {outboxLoading ? (
            <div className="p-8 text-center text-muted-foreground">Loading outbox...</div>
          ) : !outbox?.length ? (
            <div className="p-8 text-center text-muted-foreground">No outbox messages found.</div>
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full text-sm">
                <thead>
                  <tr className="border-b bg-accent/30">
                    <th className="text-left p-3 font-medium text-muted-foreground">Channel</th>
                    <th className="text-left p-3 font-medium text-muted-foreground">Recipient</th>
                    <th className="text-left p-3 font-medium text-muted-foreground">Status</th>
                    <th className="text-left p-3 font-medium text-muted-foreground">Created</th>
                    <th className="text-left p-3 font-medium text-muted-foreground">Sent</th>
                    <th className="text-left p-3 font-medium text-muted-foreground">Error</th>
                  </tr>
                </thead>
                <tbody>
                  {outbox.map((msg: any) => (
                    <tr key={msg.id} className="border-b border-border/50 hover:bg-accent/20">
                      <td className="p-3 font-medium">{msg.channel}</td>
                      <td className="p-3 text-muted-foreground font-mono text-xs">{msg.recipient}</td>
                      <td className="p-3">
                        <StatusBadge status={msg.status} />
                      </td>
                      <td className="p-3 text-muted-foreground font-mono text-xs">
                        {formatTimestamp(msg.created_at)}
                      </td>
                      <td className="p-3 text-muted-foreground font-mono text-xs">
                        {formatTimestamp(msg.sent_at)}
                      </td>
                      <td className="p-3 text-red-400 text-xs max-w-[200px] truncate" title={msg.error_message || ''}>
                        {msg.error_message || '—'}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </div>
      )}

      {/* Tab: Audit Trail */}
      {activeTab === 'audit' && (
        <div className="border rounded-md bg-card">
          <h2 className="p-4 font-semibold border-b">Audit Trail</h2>
          {auditLoading ? (
            <div className="p-8 text-center text-muted-foreground">Loading audit log...</div>
          ) : !audit?.length ? (
            <div className="p-8 text-center text-muted-foreground">No audit entries found.</div>
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full text-sm">
                <thead>
                  <tr className="border-b bg-accent/30">
                    <th className="text-left p-3 font-medium text-muted-foreground">Time</th>
                    <th className="text-left p-3 font-medium text-muted-foreground">Actor</th>
                    <th className="text-left p-3 font-medium text-muted-foreground">Action</th>
                    <th className="text-left p-3 font-medium text-muted-foreground">Entity</th>
                    <th className="text-left p-3 font-medium text-muted-foreground">Details</th>
                  </tr>
                </thead>
                <tbody>
                  {audit.map((entry: any) => (
                    <tr key={entry.id} className="border-b border-border/50 hover:bg-accent/20">
                      <td className="p-3 text-muted-foreground font-mono text-xs whitespace-nowrap">
                        {formatTimestamp(entry.created_at)}
                      </td>
                      <td className="p-3 font-medium">{entry.actor}</td>
                      <td className="p-3">
                        <span className="text-xs font-medium px-2 py-1 rounded bg-accent text-foreground">
                          {entry.action}
                        </span>
                      </td>
                      <td className="p-3 text-muted-foreground text-xs">
                        {entry.entity_type}
                        {entry.entity_id ? ` #${entry.entity_id}` : ''}
                      </td>
                      <td className="p-3 text-muted-foreground text-xs max-w-[250px] truncate" title={entry.details || ''}>
                        {entry.details || '—'}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </div>
      )}

      {/* Tab: Events */}
      {activeTab === 'events' && (
        <div className="space-y-4">
          <div className="flex justify-end">
            <Button onClick={() => rebuildMutation.mutate({})} className="gap-1.5">
              <RefreshCw className="h-4 w-4" />
              Rebuild All Projections
            </Button>
          </div>

          <div className="border rounded-md bg-card">
            <h2 className="p-4 font-semibold border-b">Live Event Stream</h2>
            {eventsLoading ? (
              <div className="p-8 text-center text-muted-foreground">Loading events...</div>
            ) : !events?.length ? (
              <div className="p-8 text-center text-muted-foreground">No events recorded yet.</div>
            ) : (
              <div className="p-4 space-y-2 h-[600px] overflow-y-auto font-mono text-sm">
                {events.map((evt: any, i: number) => (
                  <div key={i} className="flex gap-4 p-2 border-b border-border/50 hover:bg-accent/50">
                    <span className="text-muted-foreground min-w-[150px]">
                      {new Date(evt.timestamp * 1000).toISOString().replace('T', ' ').slice(0, 19)}
                    </span>
                    <span className="text-primary font-bold min-w-[150px]">{evt.event_type}</span>
                    <span className="text-foreground">{evt.aggregate_id}</span>
                  </div>
                ))}
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

function ReliabilityMetric({
  label,
  value,
  warn = false,
}: {
  label: string;
  value: string | number;
  warn?: boolean;
}) {
  return (
    <div className="border rounded-md bg-card p-4">
      <p className="text-xs font-medium text-muted-foreground">{label}</p>
      <p className={`text-xl font-bold mt-1 ${warn ? 'text-red-400' : ''}`}>{value}</p>
    </div>
  );
}
