import { useData } from '@/lib/api';

interface NotificationRecord {
  id: string;
  name: string;
  version: number;
  timestamp: number;
}

export default function History() {
  const { data: notifications, isLoading, error } = useData('/api/operations/notifications');

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-3xl font-bold tracking-tight">Notification History</h1>
        <p className="text-muted-foreground text-sm">Review dispatch logs and message history.</p>
      </div>

      <div className="border rounded-md bg-card">
        {isLoading && <div className="p-8 text-center text-muted-foreground">Loading history...</div>}
        {error && <div className="p-8 text-center text-red-500">Error fetching history</div>}
        
        {!isLoading && !error && (!notifications || notifications.length === 0) && (
          <div className="p-8 text-center text-muted-foreground">
            No notifications sent yet.
          </div>
        )}

        {!isLoading && !error && notifications && notifications.length > 0 && (
          <div className="p-4 space-y-4">
            {notifications.map((notif: NotificationRecord) => (
              <div key={notif.id} className="flex justify-between items-center p-4 border rounded-lg hover:bg-accent/50 transition-colors">
                <div className="flex items-center gap-3">
                  <div className="w-2.5 h-2.5 rounded-full bg-blue-500"></div>
                  <div>
                    <p className="font-mono text-sm font-medium">{notif.id}</p>
                    <p className="text-xs text-muted-foreground">{notif.name}</p>
                  </div>
                </div>
                <div className="text-xs text-muted-foreground">
                  {notif.timestamp ? new Date(notif.timestamp).toLocaleString() : 'N/A'}
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
