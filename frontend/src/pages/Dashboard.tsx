import { useData } from '@/lib/api';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';

function KPICard({ title, value }: { title: string; value: string | number }) {
  return (
    <Card>
      <CardHeader className="pb-2">
        <CardTitle className="text-sm font-medium">{title}</CardTitle>
      </CardHeader>
      <CardContent>
        <div className="text-2xl font-bold">{value}</div>
      </CardContent>
    </Card>
  );
}

export default function Dashboard() {
  const { data: installedPackages, isLoading: isPkgLoading } = useData('/api/platform/packages');
  const { data, isLoading, error } = useData('/api/dashboard/kpis');

  if (isLoading || isPkgLoading) return <div className="p-8 text-center text-muted-foreground">Loading Metrics...</div>;
  if (error) return <div className="p-8 text-center text-destructive">Failed to load metrics</div>;

  const packages = (installedPackages as Array<{ id: string }>) || [];
  const isSalesInstalled = packages.some((pkg) => pkg.id === 'sales-core-v1');
  const isRealEstateInstalled = packages.some((pkg) => pkg.id === 'real-estate-v1');

  return (
    <div className="space-y-6">
      <h1 className="text-3xl font-bold tracking-tight">Dashboard Overview</h1>
      <div className="grid gap-4 md:grid-cols-2 lg:grid-cols-4">
        <KPICard title="Installed Packages" value={packages.length} />
        <KPICard title="Active Now" value={data?.active_now || 0} />
        {isSalesInstalled && <KPICard title="Total Leads" value={data?.leads || 0} />}
        {isRealEstateInstalled && (
          <>
            <KPICard title="Total Sales" value={data?.sales || 0} />
            <KPICard title="Total Revenue" value={`$${(data?.revenue || 0).toLocaleString()}`} />
          </>
        )}
      </div>
    </div>
  );
}
