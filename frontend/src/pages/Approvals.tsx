import { useState } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { fetcher } from '@/lib/api';
import { useToast } from '@/components/ui/use-toast';
import { Card, CardContent, CardDescription, CardFooter, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { 
  Check, X, Calendar as CalendarIcon, User, MessageSquare, 
  Clock, CheckCircle2, XCircle, AlertCircle, ExternalLink, Phone, History, Info 
} from 'lucide-react';

interface AuthorizationDetailResponse {
  id: string;
  thread_id: string;
  engagement_id: string;
  proposed_by: string;
  status: 'pending' | 'approved' | 'rejected' | 'expired';
  created_at: number;
  contact_name?: string | null;
  contact_phone?: string | null;
  booking_title?: string | null;
  booking_start_time?: number | null;
  booking_end_time?: number | null;
  decision_reason?: string | null;
  decision_by?: string | null;
  decision_at?: number | null;
}

export default function Approvals() {
  const queryClient = useQueryClient();
  const { toast } = useToast();
  const [reasons, setReasons] = useState<Record<string, string>>({});
  const [activeTab, setActiveTab] = useState<'pending' | 'history'>('pending');

  const { data: authorizations, isLoading, error } = useQuery<AuthorizationDetailResponse[]>({
    queryKey: ['/api/authorizations'],
    queryFn: () => fetcher('/api/authorizations')
  });

  const handleReasonChange = (authId: string, value: string) => {
    setReasons(prev => ({ ...prev, [authId]: value }));
  };

  const actionMutation = useMutation({
    mutationFn: async ({ id, action, reason }: { id: string; action: 'approve' | 'reject'; reason: string }) => {
      const res = await fetch(`/api/authorizations/${id}/${action}`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ reason })
      });
      if (res.status === 401 || res.status === 403) {
        localStorage.removeItem('auth');
        localStorage.removeItem('role');
        window.location.href = '/login';
        throw new Error('Unauthorized');
      }
      if (!res.ok) throw new Error(`${action} failed`);
      return res.ok;
    },
    onSuccess: (_, variables) => {
      toast({
        title: "Success",
        description: `Authorization request has been ${variables.action === 'approve' ? 'approved' : 'rejected'}.`
      });
      setReasons(prev => {
        const copy = { ...prev };
        delete copy[variables.id];
        return copy;
      });
      queryClient.invalidateQueries();
    },
    onError: (_, variables) => {
      toast({
        title: "Error",
        description: `Failed to ${variables.action} authorization request.`,
        variant: "destructive"
      });
    }
  });

  const pendingAuths = authorizations?.filter(auth => auth.status === 'pending') || [];
  const historyAuths = authorizations?.filter(auth => auth.status !== 'pending') || [];

  return (
    <div className="space-y-6 max-w-5xl font-sans">
      <div className="flex flex-col md:flex-row md:items-center md:justify-between gap-4">
        <div className="space-y-1">
          <h1 className="text-3xl font-bold tracking-tight">Approvals & Operations Hub</h1>
          <p className="text-muted-foreground">
            Review live client booking requests, view past decisions, and audit automated assistant proposal suggestions.
          </p>
        </div>
      </div>

      {/* Tabs Selection */}
      <div className="flex border-b border-border">
        <button
          onClick={() => setActiveTab('pending')}
          className={`flex items-center gap-2 px-5 py-3 border-b-2 text-sm font-medium transition-all ${
            activeTab === 'pending'
              ? 'border-primary text-primary'
              : 'border-transparent text-muted-foreground hover:text-foreground'
          }`}
        >
          <Clock className="h-4 w-4" />
          Pending Approvals
          {pendingAuths.length > 0 && (
            <span className="ml-1 px-2 py-0.5 text-xs rounded-full bg-primary/10 text-primary font-bold">
              {pendingAuths.length}
            </span>
          )}
        </button>
        <button
          onClick={() => setActiveTab('history')}
          className={`flex items-center gap-2 px-5 py-3 border-b-2 text-sm font-medium transition-all ${
            activeTab === 'history'
              ? 'border-primary text-primary'
              : 'border-transparent text-muted-foreground hover:text-foreground'
          }`}
        >
          <History className="h-4 w-4" />
          Approvals History
          {historyAuths.length > 0 && (
            <span className="ml-1 px-2 py-0.5 text-xs rounded-full bg-muted text-muted-foreground font-semibold">
              {historyAuths.length}
            </span>
          )}
        </button>
      </div>

      {isLoading && (
        <div className="flex items-center justify-center p-16 text-sm text-muted-foreground">
          Loading approvals and contact profiles...
        </div>
      )}

      {error && (
        <div className="p-4 bg-destructive/10 text-destructive border border-destructive/20 rounded-lg text-sm text-center">
          Failed to load authorization requests. Please verify the backend service is running.
        </div>
      )}

      {!isLoading && !error && activeTab === 'pending' && (
        <div className="space-y-4">
          {pendingAuths.length === 0 ? (
            <div className="p-16 border border-dashed rounded-lg text-center text-muted-foreground flex flex-col items-center justify-center gap-3 bg-muted/10">
              <CheckCircle2 className="h-12 w-12 text-primary/50 border rounded-full p-2 bg-primary/5" />
              <div>
                <h3 className="font-semibold text-foreground text-base">Inbox Zero</h3>
                <p className="text-sm max-w-sm mt-1">No booking proposals are currently awaiting operator decision approval.</p>
              </div>
            </div>
          ) : (
            pendingAuths.map(auth => {
              const reasonText = reasons[auth.id] || '';
              const isSubmitting = actionMutation.isPending && actionMutation.variables?.id === auth.id;

              return (
                <Card key={auth.id} className="overflow-hidden border border-border bg-card hover:shadow-md transition-shadow">
                  <CardHeader className="bg-muted/30 border-b pb-4">
                    <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2">
                      <div className="space-y-1">
                        <CardTitle className="text-lg font-semibold text-foreground flex items-center gap-2">
                          {auth.booking_title || "Scheduled Appointment Slot"}
                        </CardTitle>
                        <CardDescription className="text-xs font-mono">
                          Request ID: {auth.id}
                        </CardDescription>
                      </div>
                      <span className="self-start sm:self-center text-xs px-2.5 py-1 rounded-full bg-amber-500/10 text-amber-500 font-medium border border-amber-500/20 uppercase tracking-wider">
                        Awaiting Review
                      </span>
                    </div>
                  </CardHeader>
                  
                  <CardContent className="pt-6 space-y-4">
                    {/* User profile & details */}
                    <div className="p-4 bg-muted/40 rounded-lg border border-border/55 grid gap-4 sm:grid-cols-2">
                      <div className="space-y-2">
                        <h4 className="text-xs font-semibold text-muted-foreground uppercase tracking-wider">Client / User Profile</h4>
                        <div className="space-y-1 text-sm">
                          <div className="flex items-center gap-2">
                            <User className="h-4 w-4 text-primary shrink-0" />
                            <span className="font-medium text-foreground">
                              {auth.contact_name || "Unknown Name"}
                            </span>
                          </div>
                          <div className="flex items-center gap-2 text-muted-foreground">
                            <Phone className="h-3.5 w-3.5 shrink-0" />
                            <span>{auth.contact_phone || "No Phone Registered"}</span>
                          </div>
                        </div>
                      </div>

                      <div className="space-y-2">
                        <h4 className="text-xs font-semibold text-muted-foreground uppercase tracking-wider">Proposed Appointment Details</h4>
                        <div className="space-y-1 text-sm">
                          <div className="flex items-center gap-2 text-muted-foreground">
                            <CalendarIcon className="h-4 w-4 text-primary shrink-0" />
                            <span>
                              <strong>Time: </strong> 
                              {auth.booking_start_time ? new Date(auth.booking_start_time * 1000).toLocaleString(undefined, {
                                dateStyle: 'medium',
                                timeStyle: 'short'
                              }) : "TBD"}
                            </span>
                          </div>
                          <div className="flex items-center gap-2 text-muted-foreground">
                            <Info className="h-4 w-4 text-primary shrink-0" />
                            <span>
                              <strong>Requested By: </strong> {auth.proposed_by}
                            </span>
                          </div>
                        </div>
                      </div>
                    </div>

                    <div className="flex items-center gap-2 text-sm text-muted-foreground pt-1">
                      <MessageSquare className="h-4 w-4 shrink-0 text-primary" />
                      <span className="truncate">
                        <strong>Conversation: </strong> 
                        <a href={`/inbox?thread=${auth.thread_id}`} className="text-primary hover:underline inline-flex items-center gap-1 font-medium">
                          Audit Chat Thread <ExternalLink className="h-3 w-3" />
                        </a>
                      </span>
                    </div>

                    <div className="pt-2">
                      <label htmlFor={`reason-${auth.id}`} className="text-xs font-semibold text-muted-foreground uppercase block mb-1">
                        Operator note to client (Optionally sent in WhatsApp/Chat)
                      </label>
                      <input
                        id={`reason-${auth.id}`}
                        placeholder="Add some details or reason for the client..."
                        value={reasonText}
                        onChange={(e: React.ChangeEvent<HTMLInputElement>) => handleReasonChange(auth.id, e.target.value)}
                        disabled={isSubmitting}
                        className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background file:border-0 file:bg-transparent file:text-sm file:font-medium placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50"
                      />
                    </div>
                  </CardContent>

                  <CardFooter className="bg-muted/10 border-t pt-4 pb-4 flex justify-end gap-3">
                    <Button
                      variant="outline"
                      size="sm"
                      disabled={isSubmitting}
                      onClick={() => actionMutation.mutate({ 
                        id: auth.id, 
                        action: 'reject', 
                        reason: reasonText.trim() || 'Rejected by operator' 
                      })}
                      className="border-destructive/30 text-destructive hover:bg-destructive/10 hover:text-destructive gap-1.5"
                    >
                      <X className="h-4 w-4" />
                      Reject Proposal
                    </Button>
                    
                    <Button
                      size="sm"
                      disabled={isSubmitting}
                      onClick={() => actionMutation.mutate({ 
                        id: auth.id, 
                        action: 'approve', 
                        reason: reasonText.trim() || 'Approved by operator' 
                      })}
                      className="bg-primary hover:bg-primary/90 text-primary-foreground gap-1.5"
                    >
                      <Check className="h-4 w-4" />
                      Approve & Book
                    </Button>
                  </CardFooter>
                </Card>
              );
            })
          )}
        </div>
      )}

      {!isLoading && !error && activeTab === 'history' && (
        <div className="space-y-4">
          {historyAuths.length === 0 ? (
            <div className="p-16 border border-dashed rounded-lg text-center text-muted-foreground flex flex-col items-center justify-center gap-3 bg-muted/10">
              <History className="h-12 w-12 text-muted-foreground/50 border rounded-full p-2" />
              <div>
                <h3 className="font-semibold text-foreground text-base">History Clear</h3>
                <p className="text-sm max-w-sm mt-1">No completed decisions or audit logs are registered in this database.</p>
              </div>
            </div>
          ) : (
            historyAuths.map(auth => {
              const isApproved = auth.status === 'approved';
              const isRejected = auth.status === 'rejected';

              return (
                <Card key={auth.id} className="overflow-hidden border border-border bg-card/60 relative">
                  <CardHeader className="bg-muted/20 border-b pb-4">
                    <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2">
                      <div className="space-y-1">
                        <CardTitle className="text-base font-semibold text-foreground flex items-center gap-2">
                          {auth.booking_title || "Scheduled Appointment Slot"}
                        </CardTitle>
                        <CardDescription className="text-xs font-mono">
                          ID: {auth.id}
                        </CardDescription>
                      </div>
                      <span className={`self-start sm:self-center text-xs px-2.5 py-1 rounded-full font-medium border uppercase tracking-wider ${
                        isApproved 
                          ? 'bg-green-500/10 text-green-600 border-green-500/20' 
                          : isRejected 
                            ? 'bg-red-500/10 text-red-600 border-red-500/20' 
                            : 'bg-muted text-muted-foreground border-border'
                      }`}>
                        {auth.status}
                      </span>
                    </div>
                  </CardHeader>

                  <CardContent className="pt-6 space-y-4 text-sm">
                    {/* User and scheduling details */}
                    <div className="grid gap-4 sm:grid-cols-2 p-4 bg-muted/20 rounded-lg border">
                      <div className="space-y-1.5">
                        <h4 className="text-xs font-semibold text-muted-foreground uppercase tracking-wider">Client Information</h4>
                        <div className="flex items-center gap-2">
                          <User className="h-4 w-4 text-muted-foreground" />
                          <span className="font-medium text-foreground">{auth.contact_name || "Unknown"}</span>
                        </div>
                        <div className="flex items-center gap-2 text-muted-foreground text-xs">
                          <Phone className="h-3.5 w-3.5" />
                          <span>{auth.contact_phone || "No Phone"}</span>
                        </div>
                      </div>

                      <div className="space-y-1.5">
                        <h4 className="text-xs font-semibold text-muted-foreground uppercase tracking-wider">Scheduling details</h4>
                        <div className="flex items-center gap-2 text-muted-foreground text-xs">
                          <CalendarIcon className="h-4 w-4" />
                          <span>
                            {auth.booking_start_time ? new Date(auth.booking_start_time * 1000).toLocaleString(undefined, {
                              dateStyle: 'medium',
                              timeStyle: 'short'
                            }) : "N/A"}
                          </span>
                        </div>
                        <div className="flex items-center gap-2 text-muted-foreground text-xs">
                          <Clock className="h-4 w-4" />
                          <span>Created at: {new Date(auth.created_at * 1000).toLocaleString()}</span>
                        </div>
                      </div>
                    </div>

                    {/* Decision Logs */}
                    <div className="p-4 bg-muted/40 rounded-lg border border-border/50 space-y-2">
                      <div className="flex items-center gap-1.5 text-xs font-semibold text-muted-foreground uppercase tracking-wider">
                        {isApproved ? (
                          <CheckCircle2 className="h-4 w-4 text-green-600" />
                        ) : isRejected ? (
                          <XCircle className="h-4 w-4 text-red-600" />
                        ) : (
                          <AlertCircle className="h-4 w-4" />
                        )}
                        Operator Decision Log
                      </div>
                      
                      <div className="text-sm space-y-1">
                        <div>
                          <strong>Note / Reason: </strong>
                          <span className="italic text-foreground">
                            "{auth.decision_reason || "No explanation logged."}"
                          </span>
                        </div>
                        {auth.decision_by && (
                          <div className="text-xs text-muted-foreground">
                            Decided by <strong>{auth.decision_by}</strong> on {auth.decision_at ? new Date(auth.decision_at * 1000).toLocaleString() : "TBD"}
                          </div>
                        )}
                      </div>
                    </div>

                    <div className="flex items-center justify-between text-xs text-muted-foreground pt-1">
                      <span className="flex items-center gap-1">
                        <MessageSquare className="h-3.5 w-3.5 text-muted-foreground" />
                        Thread: <a href={`/inbox?thread=${auth.thread_id}`} className="text-primary hover:underline font-mono">{auth.thread_id}</a>
                      </span>
                    </div>
                  </CardContent>
                </Card>
              );
            })
          )}
        </div>
      )}
    </div>
  );
}
