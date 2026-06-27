'use client';

import React, { useState } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { RBACProvider, useRBAC } from '@/lib/rbac';
import Sidebar from '@/components/Sidebar';
import { usePathname, useRouter } from 'next/navigation';

function AppContent({ children }: { children: React.ReactNode }) {
  const pathname = usePathname();
  const router = useRouter();
  const { user, isLoading } = useRBAC();
  const isLoginPage = pathname === '/login';

  React.useEffect(() => {
    if (!isLoading && !user && !isLoginPage) {
      router.push('/login');
    }
  }, [user, isLoading, isLoginPage, router]);

  if (isLoading) {
    return (
      <div className="min-h-screen flex items-center justify-center bg-background text-muted-foreground text-sm">
        Initializing MoatAgeex Admin...
      </div>
    );
  }

  if (isLoginPage) {
    return <main className="min-h-screen bg-background">{children}</main>;
  }

  return (
    <div className="flex bg-background min-h-screen text-foreground">
      {/* Persistent Sidebar */}
      <Sidebar />
      
      {/* Main Content Area */}
      <div className="flex-1 pl-64 flex flex-col min-h-screen">
        {/* Global Control Header */}
        <header className="h-16 border-b border-outline-variant bg-surface-container flex items-center justify-between px-8 sticky top-0 z-10">
          <div className="flex items-center space-x-4">
            <span className="text-sm font-semibold tracking-wide text-foreground uppercase font-mono">
              System Console
            </span>
            <span className="px-2 py-0.5 rounded text-[10px] font-mono font-bold bg-green-500/10 text-green-400 border border-green-500/20">
              API ONLINE
            </span>
          </div>
          <div className="flex items-center space-x-3 text-xs text-on-surfaceVariant">
            <span>Cluster: <strong className="text-foreground font-mono">moat-core-restored</strong></span>
            <span className="text-outline">|</span>
            <span>Version: <strong className="text-foreground font-mono">v1.2.0-stable</strong></span>
          </div>
        </header>

        {/* Page Content Panel */}
        <main className="flex-1 p-8 overflow-y-auto">
          {children}
        </main>
      </div>
    </div>
  );
}

export default function ClientLayout({ children }: { children: React.ReactNode }) {
  const [queryClient] = useState(() => new QueryClient({
    defaultOptions: {
      queries: {
        retry: false,
        refetchOnWindowFocus: false,
        staleTime: 5000,
      },
    },
  }));

  return (
    <QueryClientProvider client={queryClient}>
      <RBACProvider>
        <AppContent>{children}</AppContent>
      </RBACProvider>
    </QueryClientProvider>
  );
}
