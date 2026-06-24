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
import { useData } from "@/lib/api";

export default function TenantLayout() {
  const navigate = useNavigate();
  const location = useLocation();
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
    navigate('/login');
  };

  const getDynamicIcon = (name: string) => {
    const IconComponent = (Icons as any)[name];
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
            <Link 
              to="/" 
              className={`flex items-center gap-3 px-3 py-2 rounded-lg text-sm font-medium transition-colors ${
                isActive('/') ? 'bg-primary text-primary-foreground' : 'hover:bg-accent hover:text-accent-foreground'
              }`}
            >
              <LayoutDashboard className="h-4 w-4" /> Dashboard
            </Link>
            <Link 
              to="/inbox" 
              className={`flex items-center gap-3 px-3 py-2 rounded-lg text-sm font-medium transition-colors ${
                isActive('/inbox') ? 'bg-primary text-primary-foreground' : 'hover:bg-accent hover:text-accent-foreground'
              }`}
            >
              <MessageSquare className="h-4 w-4" /> Inbox
            </Link>
            <Link 
              to="/marketplace" 
              className={`flex items-center gap-3 px-3 py-2 rounded-lg text-sm font-medium transition-colors ${
                isActive('/marketplace') ? 'bg-primary text-primary-foreground' : 'hover:bg-accent hover:text-accent-foreground'
              }`}
            >
              <Package className="h-4 w-4" /> Marketplace
            </Link>
            <Link 
              to="/studio" 
              className={`flex items-center gap-3 px-3 py-2 rounded-lg text-sm font-medium transition-colors ${
                isActive('/studio') ? 'bg-primary text-primary-foreground' : 'hover:bg-accent hover:text-accent-foreground'
              }`}
            >
              <Workflow className="h-4 w-4" /> Package Studio
            </Link>
            <Link 
              to="/approvals" 
              className={`flex items-center gap-3 px-3 py-2 rounded-lg text-sm font-medium transition-colors ${
                isActive('/approvals') ? 'bg-primary text-primary-foreground' : 'hover:bg-accent hover:text-accent-foreground'
              }`}
            >
              <CheckSquare className="h-4 w-4" /> Approvals
            </Link>
            <Link 
              to="/calendar" 
              className={`flex items-center gap-3 px-3 py-2 rounded-lg text-sm font-medium transition-colors ${
                isActive('/calendar') ? 'bg-primary text-primary-foreground' : 'hover:bg-accent hover:text-accent-foreground'
              }`}
            >
              <Calendar className="h-4 w-4" /> Calendar
            </Link>
          </div>

          {/* Dynamic Business Packages Section */}
          {!isLoading && installedPackages && (installedPackages as any[]).map((pkg: any) => (
            <div key={pkg.id} className="space-y-1">
              {pkg.navigation_entries && pkg.navigation_entries.map((group: any, idx: number) => (
                <div key={idx} className="space-y-1">
                  <div className="px-3 py-1 text-xs font-semibold text-muted-foreground uppercase tracking-wider">
                    {group.label}
                  </div>
                  {group.items && group.items.map((item: any) => (
                    <Link
                      key={item.path}
                      to={item.path}
                      className={`flex items-center gap-3 px-3 py-2 rounded-lg text-sm font-medium transition-colors ${
                        isActive(item.path) ? 'bg-primary text-primary-foreground' : 'hover:bg-accent hover:text-accent-foreground'
                      }`}
                    >
                      {getDynamicIcon(item.icon)} {item.label}
                    </Link>
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
            <Link 
              to="/settings" 
              className={`flex items-center gap-3 px-3 py-2 rounded-lg text-sm font-medium transition-colors ${
                isActive('/settings') ? 'bg-primary text-primary-foreground' : 'hover:bg-accent hover:text-accent-foreground'
              }`}
            >
              <SettingsIcon className="h-4 w-4" /> Settings
            </Link>
            <Link 
              to="/users" 
              className={`flex items-center gap-3 px-3 py-2 rounded-lg text-sm font-medium transition-colors ${
                isActive('/users') ? 'bg-primary text-primary-foreground' : 'hover:bg-accent hover:text-accent-foreground'
              }`}
            >
              <UsersIcon className="h-4 w-4" /> Users
            </Link>
            <Link 
              to="/roles" 
              className={`flex items-center gap-3 px-3 py-2 rounded-lg text-sm font-medium transition-colors ${
                isActive('/roles') ? 'bg-primary text-primary-foreground' : 'hover:bg-accent hover:text-accent-foreground'
              }`}
            >
              <ShieldAlert className="h-4 w-4" /> Roles
            </Link>
            <Link 
              to="/integrations" 
              className={`flex items-center gap-3 px-3 py-2 rounded-lg text-sm font-medium transition-colors ${
                isActive('/integrations') ? 'bg-primary text-primary-foreground' : 'hover:bg-accent hover:text-accent-foreground'
              }`}
            >
              <FolderSync className="h-4 w-4" /> Integrations
            </Link>
            <Link 
              to="/observability" 
              className={`flex items-center gap-3 px-3 py-2 rounded-lg text-sm font-medium transition-colors ${
                isActive('/observability') ? 'bg-primary text-primary-foreground' : 'hover:bg-accent hover:text-accent-foreground'
              }`}
            >
              <Activity className="h-4 w-4" /> Observability
            </Link>
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
          <Outlet />
        </div>
      </main>
    </div>
  );
}
