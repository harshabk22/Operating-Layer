import { BrowserRouter, Routes, Route, Navigate } from 'react-router-dom';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { Toaster } from './components/ui/toaster';
import TenantLayout from './layouts/TenantLayout';
import Dashboard from './pages/Dashboard';
import Inbox from './pages/Inbox';
import Marketplace from './pages/Marketplace';
import Studio from './pages/Studio';
import Settings from './pages/Settings';
import Users from './pages/Users';
import Roles from './pages/Roles';
import Connections from './pages/Connections';
import Observability from './pages/Observability';
import Login from './pages/Login';
import { useData } from './lib/api';
import { componentMap } from './packages/registry';
import { WifiOff } from 'lucide-react';

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      retry: (failureCount, error: any) => {
        // Do not retry on authentication errors or after 1 failure
        if (error?.message === 'Unauthorized' || failureCount >= 1) {
          return false;
        }
        return true;
      },
      refetchOnWindowFocus: false,
    },
  },
});

function DynamicRoutesWrapper() {
  const { data: installedPackages, isLoading, error, refetch } = useData('/api/platform/packages');

  if (isLoading) {
    return (
      <div className="min-h-screen flex items-center justify-center bg-background text-muted-foreground text-sm font-sans">
        Initializing Moat Platform...
      </div>
    );
  }

  if (error) {
    return (
      <div className="min-h-screen flex flex-col items-center justify-center bg-background text-foreground font-sans p-6 text-center space-y-4">
        <div className="p-4 bg-destructive/10 text-destructive rounded-full border border-destructive/20 shadow-inner">
          <WifiOff className="h-8 w-8" />
        </div>
        <h2 className="text-2xl font-bold tracking-tight">Backend Service Offline</h2>
        <p className="text-sm text-muted-foreground max-w-md leading-relaxed">
          The Moat Platform API server is currently unavailable or returned a gateway error. Please verify the backend is running.
        </p>
        <button 
          onClick={() => refetch()}
          className="px-5 py-2.5 bg-primary text-primary-foreground rounded-lg text-sm font-semibold hover:bg-primary/90 transition-all duration-200 shadow-sm active:scale-95 cursor-pointer"
        >
          Retry Connection
        </button>
      </div>
    );
  }

  const dynamicRoutes = (installedPackages || []).flatMap((pkg: any) => 
    (pkg.routes || []).map((route: any) => {
      const Component = componentMap[route.component];
      if (!Component) return null;
      return (
        <Route 
          key={route.path} 
          path={route.path} 
          element={<Component />} 
        />
      );
    })
  ).filter(Boolean);

  return (
    <Routes>
      <Route path="/" element={<TenantLayout />}>
        <Route index element={<Dashboard />} />
        <Route path="inbox" element={<Inbox />} />
        <Route path="marketplace" element={<Marketplace />} />
        <Route path="studio" element={<Studio />} />
        <Route path="settings" element={<Settings />} />
        <Route path="users" element={<Users />} />
        <Route path="roles" element={<Roles />} />
        <Route path="integrations" element={<Connections />} />
        <Route path="observability" element={<Observability />} />
        
        {/* Dynamically Injected Package Routes */}
        {dynamicRoutes}
      </Route>
      <Route path="*" element={<Navigate to="/" />} />
    </Routes>
  );
}

export default function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <BrowserRouter>
        <Routes>
          <Route path="/login" element={<Login />} />
          <Route path="*" element={<DynamicRoutesWrapper />} />
        </Routes>
      </BrowserRouter>
      <Toaster />
    </QueryClientProvider>
  );
}
