import { useData } from '@/lib/api';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';

export default function Dashboard() {
  const { data: installedPackages, isLoading: isPkgLoading } = useData('/api/platform/packages');
  const { data, isLoading, error } = useData('/api/dashboard/kpis');

  if (isLoading || isPkgLoading) return <div className="p-8 text-center text-muted-foreground">Loading Metrics...</div>;
  if (error) return <div className="p-8 text-center text-destructive">Failed to load metrics</div>;

  const isSalesInstalled = (installedPackages || []).some((pkg: any) => pkg.id === 'sales-core-v1');
  const isRealEstateInstalled = (installedPackages || []).some((pkg: any) => pkg.id === 'real-estate-v1');

  return (
    <div className="space-y-6">
      <h1 className="text-3xl font-bold tracking-tight">Dashboard Overview</h1>
      <div className="grid gap-4 md:grid-cols-2 lg:grid-cols-4">
        {/* Core Platform KPIs */}
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium">Installed Packages</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">{installedPackages?.length || 0}</div>
          </CardContent>
        </Card>
        
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium">Active Now</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">{data?.active_now || 0}</div>
          </CardContent>
        </Card>

        {/* Dynamic business-specific KPIs */}
        {isSalesInstalled && (
          <Card>
            <CardHeader className="pb-2">
              <CardTitle className="text-sm font-medium">Total Leads</CardTitle>
            </CardHeader>
            <CardContent>
              <div className="text-2xl font-bold">{data?.leads || 0}</div>
            </CardContent>
          </Card>
        )}

        {isRealEstateInstalled && (
          <>
            <Card>
              <CardHeader className="pb-2">
                <CardTitle className="text-sm font-medium">Total Sales</CardTitle>
              </CardHeader>
              <CardContent>
                <div className="text-2xl font-bold">{data?.sales || 0}</div>
              </CardContent>
            </Card>

            <Card>
              <CardHeader className="pb-2">
                <CardTitle className="text-sm font-medium">Total Revenue</CardTitle>
              </CardHeader>
              <CardContent>
                <div className="text-2xl font-bold">${(data?.revenue || 0).toLocaleString()}</div>
              </CardContent>
            </Card>
          </>
        )}
      </div>
    </div>
  );
}
