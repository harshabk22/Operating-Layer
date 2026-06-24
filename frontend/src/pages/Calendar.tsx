import { useQuery } from '@tanstack/react-query';
import { fetcher } from '@/lib/api';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Calendar as CalendarIcon, Clock, MessageSquare, CheckCircle2, AlertCircle, HelpCircle } from 'lucide-react';

interface Booking {
  id: string;
  contact_id: string;
  thread_id: string;
  title: string;
  status: string;
  start_time: number;
  end_time: number;
  created_at: number;
  updated_at: number;
}

export default function Calendar() {
  const { data: bookings, isLoading, error } = useQuery<Booking[]>({
    queryKey: ['/api/bookings'],
    queryFn: () => fetcher('/api/bookings')
  });

  const getStatusBadge = (status: string) => {
    switch (status.toLowerCase()) {
      case 'confirmed':
      case 'approved':
        return (
          <span className="inline-flex items-center gap-1 text-xs px-2.5 py-0.5 rounded-full bg-green-500/10 text-green-500 font-medium border border-green-500/20">
            <CheckCircle2 className="h-3 w-3" /> Confirmed
          </span>
        );
      case 'pending_authorization':
      case 'pending':
        return (
          <span className="inline-flex items-center gap-1 text-xs px-2.5 py-0.5 rounded-full bg-amber-500/10 text-amber-500 font-medium border border-amber-500/20">
            <AlertCircle className="h-3 w-3 animate-pulse" /> Pending Operator Approval
          </span>
        );
      default:
        return (
          <span className="inline-flex items-center gap-1 text-xs px-2.5 py-0.5 rounded-full bg-muted text-muted-foreground font-medium border border-border">
            <HelpCircle className="h-3 w-3" /> {status}
          </span>
        );
    }
  };

  // Sort bookings: pending_authorization first, then by start_time ascending
  const sortedBookings = bookings
    ? [...bookings].sort((a, b) => {
        if (a.status === 'pending_authorization' && b.status !== 'pending_authorization') return -1;
        if (a.status !== 'pending_authorization' && b.status === 'pending_authorization') return 1;
        return a.start_time - b.start_time;
      })
    : [];

  return (
    <div className="space-y-6 max-w-4xl font-sans">
      <div className="flex flex-col gap-2">
        <h1 className="text-3xl font-bold tracking-tight">Calendar & Engagements</h1>
        <p className="text-muted-foreground">
          View and manage scheduled business consultation slots and client appointments.
        </p>
      </div>

      {isLoading && (
        <div className="flex items-center justify-center p-12 text-sm text-muted-foreground">
          Loading calendar events...
        </div>
      )}

      {error && (
        <div className="p-4 bg-destructive/10 text-destructive border border-destructive/20 rounded-lg text-sm text-center">
          Failed to load calendar events. Please check your network connection.
        </div>
      )}

      {!isLoading && !error && sortedBookings.length === 0 && (
        <div className="p-12 border border-dashed rounded-lg text-center text-muted-foreground flex flex-col items-center justify-center gap-3">
          <CalendarIcon className="h-10 w-10 text-muted-foreground/50 border rounded-full p-2" />
          <div>
            <h3 className="font-semibold text-foreground">No Scheduled Appointments</h3>
            <p className="text-sm">There are no upcoming meetings or consultation slots scheduled.</p>
          </div>
        </div>
      )}

      {!isLoading && !error && sortedBookings.length > 0 && (
        <div className="grid gap-4">
          {sortedBookings.map(booking => {
            const startDate = new Date(booking.start_time * 1000);
            const endDate = new Date(booking.end_time * 1000);
            
            // Format time range (e.g. 10:00 AM - 11:00 AM)
            const timeRange = `${startDate.toLocaleTimeString(undefined, { timeStyle: 'short' })} - ${endDate.toLocaleTimeString(undefined, { timeStyle: 'short' })}`;
            const dateStr = startDate.toLocaleDateString(undefined, { dateStyle: 'full' });

            return (
              <Card key={booking.id} className="border border-border bg-card hover:bg-accent/5 transition-colors">
                <CardHeader className="pb-3">
                  <div className="flex items-start justify-between">
                    <div className="space-y-1">
                      <CardTitle className="text-lg font-semibold flex items-center gap-2">
                        {booking.title || "Scheduled Slot"}
                      </CardTitle>
                      <CardDescription className="text-xs">
                        Booking ID: {booking.id}
                      </CardDescription>
                    </div>
                    {getStatusBadge(booking.status)}
                  </div>
                </CardHeader>

                <CardContent className="space-y-3 text-sm text-muted-foreground">
                  <div className="flex items-center gap-2">
                    <CalendarIcon className="h-4 w-4 text-primary shrink-0" />
                    <span className="text-foreground font-medium">{dateStr}</span>
                  </div>

                  <div className="flex items-center gap-2">
                    <Clock className="h-4 w-4 text-primary shrink-0" />
                    <span>{timeRange}</span>
                  </div>

                  <div className="flex items-center gap-2">
                    <MessageSquare className="h-4 w-4 text-primary shrink-0" />
                    <span className="truncate">
                      <strong>Thread: </strong>
                      <a href={`/inbox?thread=${booking.thread_id}`} className="text-primary hover:underline">
                        {booking.thread_id}
                      </a>
                    </span>
                  </div>
                </CardContent>
              </Card>
            );
          })}
        </div>
      )}
    </div>
  );
}
