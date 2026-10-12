import { useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { useData, useMutateData } from '@/lib/api';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardFooter, CardHeader, CardTitle } from '@/components/ui/card';
import { useToast } from '@/components/ui/use-toast';
import { Eye, Shield, Activity, Database, CheckCircle, Info } from 'lucide-react';

interface MarketplacePackage {
  id: string;
  name: string;
  description: string;
  version: string;
  routes: { path: string; component: string }[];
  navigation: { group: string; items: { label: string; path: string }[] }[];
  permissions: string[];
  commands: string[];
  events: string[];
  projections: string[];
}

const AVAILABLE_PACKAGES: MarketplacePackage[] = [
  {
    id: 'sales-core-v1',
    name: 'Sales Core',
    description: 'Provides lead scoring, customer relationship management, and sales opportunity tracking.',
    version: '1.0.0',
    routes: [
      { path: 'sales/leads', component: 'Leads' },
      { path: 'sales/opportunities', component: 'Opportunities' }
    ],
    navigation: [
      {
        group: 'Sales',
        items: [
          { label: 'Leads', path: '/sales/leads' },
          { label: 'Opportunities', path: '/sales/opportunities' }
        ]
      }
    ],
    permissions: ['sales.leads.read', 'sales.leads.write'],
    commands: ['CreateLead', 'QualifyLead', 'CreateOpportunity', 'TransitionPipelineStage'],
    events: ['LeadCreated', 'LeadQualified', 'OpportunityCreated', 'PipelineStageTransitioned'],
    projections: ['LeadList', 'OpportunityList']
  },
  {
    id: 'real-estate-v1',
    name: 'Real Estate',
    description: 'Handles property listings, offer management, and deal closures.',
    version: '1.0.0',
    routes: [
      { path: 'realestate/properties', component: 'Properties' },
      { path: 'realestate/offers', component: 'Offers' },
      { path: 'realestate/deals', component: 'Deals' }
    ],
    navigation: [
      {
        group: 'Real Estate',
        items: [
          { label: 'Properties', path: '/realestate/properties' },
          { label: 'Offers', path: '/realestate/offers' },
          { label: 'Deals', path: '/realestate/deals' }
        ]
      }
    ],
    permissions: ['realestate.properties.read', 'realestate.properties.write'],
    commands: ['CreateProperty', 'CreateOffer', 'CloseDeal'],
    events: ['PropertyCreated', 'OfferCreated', 'DealWon'],
    projections: ['PropertyList', 'OfferList', 'DealList']
  },
  {
    id: 'notification-package-v1',
    name: 'Notifications',
    description: 'Notification Center, Templates, and history dispatch logging.',
    version: '1.0.0',
    routes: [
      { path: 'notifications', component: 'Notifications' },
      { path: 'notifications/templates', component: 'Templates' },
      { path: 'notifications/history', component: 'History' }
    ],
    navigation: [
      {
        group: 'Notifications',
        items: [
          { label: 'Center', path: '/notifications' },
          { label: 'Templates', path: '/notifications/templates' },
          { label: 'History', path: '/notifications/history' }
        ]
      }
    ],
    permissions: ['notifications.send'],
    commands: ['SendNotification'],
    events: ['NotificationSent'],
    projections: ['NotificationList']
  }
];

const PACKAGE_ACTIONS = [
  { label: 'Upgrade', suffix: '/upgrade', method: 'POST', msg: 'Package upgraded successfully', variant: 'outline' as const, className: 'text-xs font-medium px-3' },
  { label: 'Rollback', suffix: '/rollback', method: 'POST', msg: 'Package rolled back', variant: 'outline' as const, className: 'text-xs font-medium px-3' },
  { label: 'Uninstall', suffix: '', method: 'DELETE', msg: 'Package uninstalled', variant: 'destructive' as const, className: 'text-xs font-semibold px-3' },
];

function SchemaSection({ title, items }: { title: string; items: string[] }) {
  return (
    <div className="border p-4 rounded-lg bg-slate-50/50 dark:bg-slate-900/30 space-y-3">
      <div className="text-xs font-bold text-slate-600 border-b pb-2 uppercase tracking-wide">
        {title}
      </div>
      <div className="space-y-1.5">
        {items.map((item) => (
          <div key={item} className="text-[11px] font-mono bg-card px-2 py-1 rounded border shadow-sm text-slate-700 dark:text-slate-300">
            {item}
          </div>
        ))}
      </div>
    </div>
  );
}

export default function Marketplace() {
  const queryClient = useQueryClient();
  const { toast } = useToast();
  const { data: installedPackages, isLoading } = useData('/api/platform/packages');
  const installMutation = useMutateData('/api/platform/packages/install', 'Package installed successfully');
  const [actionLoading, setActionLoading] = useState<string | null>(null);
  const [selectedPkg, setSelectedPkg] = useState<MarketplacePackage | null>(null);
  const [modalTab, setModalTab] = useState<'overview' | 'capabilities' | 'registry'>('overview');

  const isInstalled = (packageId: string) => {
    if (!installedPackages) return false;
    return (Object.values(installedPackages) as Array<{ package_id?: string; id?: string }>).some(
      (pkg) => pkg.package_id === packageId || pkg.id === packageId
    );
  };

  const handleAction = async (packageId: string, endpoint: string, method: string = 'POST', successMsg: string) => {
    setActionLoading(packageId);
    try {
      const res = await fetch(endpoint, { method });
      if (res.status === 401 || res.status === 403) {
        localStorage.removeItem('auth');
        window.location.assign('/login');
        return;
      }
      if (!res.ok) throw new Error('Operation failed');
      toast({ title: 'Success', description: successMsg });
      queryClient.invalidateQueries();
    } catch {
      toast({ title: 'Error', description: 'Action failed', variant: 'destructive' });
    } finally {
      setActionLoading(null);
    }
  };

  return (
    <div className="space-y-8 max-w-6xl mx-auto font-sans">
      <div className="flex justify-between items-center border-b pb-5">
        <div>
          <h1 className="text-3xl font-extrabold tracking-tight bg-gradient-to-r from-primary to-blue-500 bg-clip-text text-transparent">
            Package Marketplace
          </h1>
          <p className="text-muted-foreground text-sm mt-1">Browse, install, and manage official Moat platform packages dynamically.</p>
        </div>
      </div>

      {isLoading ? (
        <div className="p-16 text-center text-muted-foreground text-sm font-medium">Loading Marketplace...</div>
      ) : (
        <div className="grid gap-6 md:grid-cols-2">
          {AVAILABLE_PACKAGES.map((pkg) => {
            const installed = isInstalled(pkg.id);
            return (
              <Card key={pkg.id} className="flex flex-col justify-between border-slate-200/80 dark:border-slate-800 hover:border-primary/50 transition-all duration-300 shadow-sm hover:shadow-md">
                <CardHeader className="pb-4">
                  <div className="flex justify-between items-start">
                    <div>
                      <CardTitle className="text-lg font-bold">{pkg.name}</CardTitle>
                      <span className="text-[10px] uppercase font-mono px-2 py-0.5 mt-1 inline-block rounded bg-slate-100 dark:bg-slate-800 text-slate-500 font-semibold">
                        {pkg.id}
                      </span>
                    </div>
                    <span className="text-[11px] px-2 py-1 rounded-md bg-secondary font-mono border font-medium text-secondary-foreground">
                      v{pkg.version}
                    </span>
                  </div>
                  <CardDescription className="pt-3 text-slate-500 dark:text-slate-400 text-sm leading-relaxed">
                    {pkg.description}
                  </CardDescription>
                </CardHeader>
                <CardContent className="pb-6">
                  {installed ? (
                    <div className="flex items-center gap-2 text-xs text-green-600 dark:text-green-400 font-semibold bg-green-50 dark:bg-green-950/30 px-3 py-1.5 rounded-lg w-fit border border-green-200/40">
                      <CheckCircle className="h-4.5 w-4.5" />
                      Active & Mounted
                    </div>
                  ) : (
                    <div className="flex items-center gap-2 text-xs text-slate-500 font-medium bg-slate-50 dark:bg-slate-900 px-3 py-1.5 rounded-lg w-fit border">
                      <span className="h-2.5 w-2.5 rounded-full bg-slate-400 animate-pulse" />
                      Available to Install
                    </div>
                  )}
                </CardContent>
                <CardFooter className="flex gap-2 justify-between border-t p-4 bg-slate-50/50 dark:bg-slate-900/30 rounded-b-lg">
                  <Button
                    variant="outline"
                    size="sm"
                    className="flex items-center gap-1.5 text-xs text-slate-600 hover:text-foreground"
                    onClick={() => {
                      setSelectedPkg(pkg);
                      setModalTab('overview');
                    }}
                  >
                    <Info className="h-3.5 w-3.5" /> Package Details
                  </Button>
                  <div className="flex gap-2">
                    {!installed ? (
                      <Button
                        size="sm"
                        className="text-xs font-semibold px-4 shadow-sm"
                        disabled={actionLoading === pkg.id}
                        onClick={() => installMutation.mutate({ package_id: pkg.id })}
                      >
                        Install Package
                      </Button>
                    ) : (
                      PACKAGE_ACTIONS.map((action) => (
                        <Button
                          key={action.label}
                          variant={action.variant}
                          size="sm"
                          className={action.className}
                          disabled={actionLoading === pkg.id}
                          onClick={() =>
                            handleAction(
                              pkg.id,
                              `/api/platform/packages/${pkg.id}${action.suffix}`,
                              action.method,
                              action.msg
                            )
                          }
                        >
                          {action.label}
                        </Button>
                      ))
                    )}
                  </div>
                </CardFooter>
              </Card>
            );
          })}
        </div>
      )}

      {/* Package Detail Drawer / Modal */}
      {selectedPkg && (
        <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-background/80 backdrop-blur-sm animate-in fade-in duration-200">
          <div className="relative w-full max-w-2xl bg-card border rounded-xl shadow-2xl overflow-hidden flex flex-col max-h-[85vh] animate-in zoom-in-95 duration-200">
            {/* Header */}
            <div className="p-6 border-b flex justify-between items-start bg-slate-50/50 dark:bg-slate-900/30">
              <div>
                <div className="flex items-center gap-3">
                  <h2 className="text-xl font-bold">{selectedPkg.name}</h2>
                  <span className="text-xs px-2 py-0.5 rounded bg-primary/10 text-primary font-mono border border-primary/20">
                    v{selectedPkg.version}
                  </span>
                </div>
                <p className="text-xs text-muted-foreground font-mono mt-1">{selectedPkg.id}</p>
              </div>
              <button
                onClick={() => setSelectedPkg(null)}
                className="text-slate-400 hover:text-foreground text-lg font-bold p-1 rounded-lg hover:bg-slate-100 dark:hover:bg-slate-800 transition-colors"
              >
                &times;
              </button>
            </div>

            {/* Tab Navigation */}
            <div className="flex border-b bg-card px-4">
              {(['overview', 'capabilities', 'registry'] as const).map((tab) => (
                <button
                  key={tab}
                  onClick={() => setModalTab(tab)}
                  className={`px-4 py-3 text-xs font-semibold border-b-2 capitalize transition-all ${
                    modalTab === tab
                      ? 'border-primary text-primary font-bold'
                      : 'border-transparent text-slate-500 hover:text-slate-800'
                  }`}
                >
                  {tab}
                </button>
              ))}
            </div>

            {/* Content Area */}
            <div className="flex-1 overflow-y-auto p-6 space-y-6">
              {modalTab === 'overview' && (
                <div className="space-y-6">
                  <div className="space-y-2">
                    <h3 className="text-sm font-semibold text-slate-700 dark:text-slate-300">Description</h3>
                    <p className="text-sm text-slate-500 dark:text-slate-400 leading-relaxed bg-slate-50 dark:bg-slate-900/50 p-3 rounded-lg border">
                      {selectedPkg.description}
                    </p>
                  </div>

                  <div className="space-y-3">
                    <h3 className="text-sm font-semibold text-slate-700 dark:text-slate-300 flex items-center gap-2">
                      <Shield className="h-4 w-4 text-primary" /> Required Permissions
                    </h3>
                    <div className="grid gap-2 sm:grid-cols-2">
                      {selectedPkg.permissions.map((perm) => (
                        <div key={perm} className="flex items-center gap-2.5 p-2.5 rounded-lg border bg-slate-50/50 dark:bg-slate-900/30">
                          <span className="h-1.5 w-1.5 rounded-full bg-primary" />
                          <span className="text-xs font-mono font-medium text-slate-700 dark:text-slate-300">{perm}</span>
                        </div>
                      ))}
                    </div>
                  </div>
                </div>
              )}

              {modalTab === 'capabilities' && (
                <div className="space-y-6">
                  {/* Dynamic Routes */}
                  <div className="space-y-3">
                    <h3 className="text-sm font-semibold text-slate-700 dark:text-slate-300 flex items-center gap-2">
                      <Eye className="h-4 w-4 text-blue-500" /> Injected React Routes
                    </h3>
                    <div className="border rounded-lg overflow-hidden bg-card">
                      <table className="w-full text-left text-xs border-collapse">
                        <thead>
                          <tr className="bg-slate-50 dark:bg-slate-900 border-b">
                            <th className="p-3 font-semibold text-slate-600">URI Path</th>
                            <th className="p-3 font-semibold text-slate-600">React Component</th>
                          </tr>
                        </thead>
                        <tbody>
                          {selectedPkg.routes.map((route) => (
                            <tr key={route.path} className="border-b last:border-b-0 hover:bg-slate-50/50 dark:hover:bg-slate-900/30">
                              <td className="p-3 font-mono font-bold text-slate-800 dark:text-slate-200">/{route.path}</td>
                              <td className="p-3 font-mono text-slate-500">{route.component}</td>
                            </tr>
                          ))}
                        </tbody>
                      </table>
                    </div>
                  </div>

                  {/* Sidebar Navigation */}
                  <div className="space-y-3">
                    <h3 className="text-sm font-semibold text-slate-700 dark:text-slate-300 flex items-center gap-2">
                      <Activity className="h-4 w-4 text-emerald-500" /> Sidebar Navigation Entries
                    </h3>
                    <div className="space-y-4">
                      {selectedPkg.navigation.map((group, idx) => (
                        <div key={idx} className="border p-4 rounded-lg bg-slate-50/50 dark:bg-slate-900/30 space-y-2.5">
                          <div className="text-xs font-bold uppercase tracking-wider text-muted-foreground">
                            Group: {group.group}
                          </div>
                          <div className="flex flex-wrap gap-2">
                            {group.items.map((item) => (
                              <span key={item.path} className="text-xs font-medium bg-card px-2.5 py-1.5 rounded-lg border shadow-sm">
                                {item.label} ({item.path})
                              </span>
                            ))}
                          </div>
                        </div>
                      ))}
                    </div>
                  </div>
                </div>
              )}

              {modalTab === 'registry' && (
                <div className="space-y-6">
                  <div className="space-y-3">
                    <h3 className="text-sm font-semibold text-slate-700 dark:text-slate-300 flex items-center gap-2">
                      <Database className="h-4 w-4 text-purple-500" /> Declared Event Sourcing Schema
                    </h3>
                    <div className="grid gap-4 md:grid-cols-3">
                      <SchemaSection title="Commands" items={selectedPkg.commands} />
                      <SchemaSection title="Events" items={selectedPkg.events} />
                      <SchemaSection title="Projections" items={selectedPkg.projections} />
                    </div>
                  </div>
                </div>
              )}
            </div>

            {/* Footer */}
            <div className="p-4 border-t bg-slate-50/50 dark:bg-slate-900/30 flex justify-end gap-2">
              <Button size="sm" variant="outline" className="text-xs" onClick={() => setSelectedPkg(null)}>
                Close Window
              </Button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
