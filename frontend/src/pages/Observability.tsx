import { useData, useMutateData } from '@/lib/api';
import { Button } from '@/components/ui/button';

export default function Observability() {

  const { data: events, isLoading } = useData('/api/system/events');
  const rebuildMutation = useMutateData('/api/system/rebuild_projections', 'Projections rebuilt successfully');

  return (
    <div className="space-y-6">
      <div className="flex justify-between items-center">
        <h1 className="text-3xl font-bold">Observability</h1>
        <Button onClick={() => rebuildMutation.mutate({})}>Rebuild All Projections</Button>
      </div>

      <div className="border rounded-md bg-card">
        <h2 className="p-4 font-semibold border-b">Live Event Stream</h2>
        {isLoading ? <div className="p-8 text-center text-muted-foreground">Loading...</div> : (
          <div className="p-4 space-y-2 h-[600px] overflow-y-auto font-mono text-sm">
            {events?.map((evt: any, i: number) => (
              <div key={i} className="flex gap-4 p-2 border-b border-border/50 hover:bg-accent/50">
                <span className="text-muted-foreground min-w-[150px]">{new Date(evt.timestamp * 1000).toISOString().replace('T', ' ').slice(0, 19)}</span>
                <span className="text-primary font-bold min-w-[150px]">{evt.event_type}</span>
                <span className="text-foreground">{evt.aggregate_id}</span>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
