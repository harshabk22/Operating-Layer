import { Outlet, Link, useNavigate, useLocation } from "react-router-dom";
import { 
  LayoutDashboard, 
  MessageSquare, 
  Package, 
  Workflow, 
  Settings as SettingsIcon, 
  Users as UsersIcon, 
  ShieldAlert, 
  FolderSync, 
  Activity, 
  LogOut, 
  HelpCircle, 
  CheckSquare, 
  Calendar 
} from "lucide-react";
import * as Icons from "lucide-react";
import { useEffect, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useData } from "@/lib/api";
import { ErrorBoundary } from "@/components/ErrorBoundary";

interface NavItem {
  path: string;
  label: string;
  icon: string;
}

interface NavGroup {
  label: string;
  items?: NavItem[];
}

interface InstalledPackage {
  id: string;
  navigation_entries?: NavGroup[];
}

const CORE_LINKS = [
  { path: '/', label: 'Dashboard', icon: LayoutDashboard },
  { path: '/inbox', label: 'Inbox', icon: MessageSquare },
  { path: '/marketplace', label: 'Marketplace', icon: Package },
  { path: '/studio', label: 'Package Studio', icon: Workflow },
  { path: '/approvals', label: 'Approvals', icon: CheckSquare },
  { path: '/calendar', label: 'Calendar', icon: Calendar },
];

const GOVERNANCE_LINKS = [
  { path: '/settings', label: 'Settings', icon: SettingsIcon },
  { path: '/users', label: 'Users', icon: UsersIcon },
  { path: '/roles', label: 'Roles', icon: ShieldAlert },
  { path: '/integrations', label: 'Integrations', icon: FolderSync },
  { path: '/observability', label: 'Observability', icon: Activity },
];

function NavLinkItem({
  to,
  label,
  icon,
  isActive,
}: {
  to: string;
  label: string;
  icon: React.ReactNode;
  isActive: boolean;
}) {
  return (
    <Link
      to={to}
      className={`flex items-center gap-3 px-3 py-2 rounded-lg text-sm font-medium transition-colors ${
        isActive ? 'bg-primary text-primary-foreground' : 'hover:bg-accent hover:text-accent-foreground'
      }`}
    >
      {icon}
      {label}
    </Link>
  );
}

export default function TenantLayout() {
  const navigate = useNavigate();
  const location = useLocation();
  const queryClient = useQueryClient();
  const [role] = useState(localStorage.getItem('role') || 'viewer');
  const { data: installedPackages, isLoading } = useData('/api/platform/packages');

  useEffect(() => {
    if (!localStorage.getItem('auth')) {
      navigate('/login');
    }
  }, [navigate]);

  const handleLogout = async () => {
    try {
      await fetch('/api/logout', { method: 'POST' });
    } catch (err) {
      console.error('Logout error', err);
    }
    localStorage.removeItem('auth');
    localStorage.removeItem('role');
    queryClient.clear();
    navigate('/login');
  };

  const getDynamicIcon = (name: string) => {
    const iconMap = Icons as unknown as Record<string, React.ComponentType<{ className?: string }>>;
    const IconComponent = iconMap[name];
    return IconComponent ? <IconComponent className="h-4 w-4" /> : <HelpCircle className="h-4 w-4" />;
  };

  const isActive = (path: string) => location.pathname === path;

  return (
    <div id="app-layout" className="min-h-screen bg-background text-foreground flex flex-col md:flex-row">
      <aside className="w-full md:w-64 border-r bg-card/50 backdrop-blur-xl flex flex-col font-sans">
        <div className="p-6 border-b">
          <h1 className="text-xl font-bold bg-gradient-to-r from-primary to-blue-500 bg-clip-text text-transparent">
            MoatAgeex
          </h1>
          <span className="text-xs text-muted-foreground uppercase tracking-wider">{role}</span>
        </div>
        
        <nav className="flex-1 p-4 space-y-6 overflow-y-auto">
          {/* Core Platform Sidebar Section */}
          <div className="space-y-1">
            <div className="px-3 py-1 text-xs font-semibold text-muted-foreground uppercase tracking-wider">
              Core Platform
            </div>
            {CORE_LINKS.map(({ path, label, icon: Icon }) => (
              <NavLinkItem
                key={path}
                to={path}
                label={label}
                icon={<Icon className="h-4 w-4" />}
                isActive={isActive(path)}
              />
            ))}
          </div>

          {/* Dynamic Business Packages Section */}
          {!isLoading && installedPackages && (installedPackages as InstalledPackage[]).map((pkg) => (
            <div key={pkg.id} className="space-y-1">
              {pkg.navigation_entries?.map((group, idx) => (
                <div key={idx} className="space-y-1">
                  <div className="px-3 py-1 text-xs font-semibold text-muted-foreground uppercase tracking-wider">
                    {group.label}
                  </div>
                  {group.items?.map((item) => (
                    <NavLinkItem
                      key={item.path}
                      to={item.path}
                      label={item.label}
                      icon={getDynamicIcon(item.icon)}
                      isActive={isActive(item.path)}
                    />
                  ))}
                </div>
              ))}
            </div>
          ))}

          {/* Management & Governance Section */}
          <div className="space-y-1">
            <div className="px-3 py-1 text-xs font-semibold text-muted-foreground uppercase tracking-wider">
              Governance
            </div>
            {GOVERNANCE_LINKS.map(({ path, label, icon: Icon }) => (
              <NavLinkItem
                key={path}
                to={path}
                label={label}
                icon={<Icon className="h-4 w-4" />}
                isActive={isActive(path)}
              />
            ))}
          </div>
        </nav>

        <div className="p-4 border-t bg-card/80">
          <button 
            onClick={handleLogout} 
            className="w-full flex items-center justify-between px-3 py-2 rounded-lg hover:bg-accent hover:text-accent-foreground text-sm font-medium transition-colors"
          >
            <span className="flex items-center gap-3"><LogOut className="h-4 w-4" /> Logout</span>
          </button>
        </div>
      </aside>

      <main className="flex-1 flex flex-col min-h-0 overflow-hidden">
        <header className="h-16 border-b bg-background/80 backdrop-blur-xl flex items-center justify-between px-6">
          <div className="flex items-center gap-4 flex-1">
            {/* Search feature temporarily hidden due to missing backend connection */}
          </div>
          <div className="flex items-center gap-4">
            {/* Notification bell temporarily hidden due to missing backend connection */}
          </div>
        </header>
        <div className="flex-1 overflow-auto p-8">
          <ErrorBoundary fallbackTitle="Page Error">
            <Outlet />
          </ErrorBoundary>
        </div>
      </main>
    </div>
  );
}
