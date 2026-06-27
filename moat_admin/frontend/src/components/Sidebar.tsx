import React from 'react';
import Link from 'next/link';
import { usePathname } from 'next/navigation';
import { useRBAC, UserRole } from '@/lib/rbac';
import {
  LayoutDashboard,
  Users,
  Rocket,
  ShoppingBag,
  ShieldCheck,
  Key,
  UploadCloud,
  Cpu,
  RefreshCw,
  CreditCard,
  LifeBuoy,
  BarChart3,
  ClipboardList,
  LogOut,
  ChevronDown
} from 'lucide-react';

interface SidebarItem {
  name: string;
  href: string;
  icon: React.ComponentType<any>;
  roles: UserRole[];
}

const navigationItems: SidebarItem[] = [
  { name: 'Dashboard', href: '/', icon: LayoutDashboard, roles: ['Super Admin', 'Developer', 'Support Engineer', 'Finance', 'Read Only'] },
  { name: 'Customers', href: '/customers', icon: Users, roles: ['Super Admin', 'Support Engineer', 'Finance', 'Read Only'] },
  { name: 'Deployments', href: '/deployments', icon: Rocket, roles: ['Super Admin', 'Developer'] },
  { name: 'Marketplace', href: '/marketplace', icon: ShoppingBag, roles: ['Super Admin', 'Developer', 'Support Engineer', 'Finance', 'Read Only'] },
  { name: 'Approvals', href: '/approvals', icon: ShieldCheck, roles: ['Super Admin', 'Developer', 'Finance'] },
  { name: 'License', href: '/license', icon: Key, roles: ['Super Admin', 'Developer', 'Finance'] },
  { name: 'Publisher', href: '/publisher', icon: UploadCloud, roles: ['Super Admin', 'Developer'] },
  { name: 'Runtime', href: '/runtime', icon: Cpu, roles: ['Super Admin', 'Developer'] },
  { name: 'Updates', href: '/updates', icon: RefreshCw, roles: ['Super Admin', 'Developer'] },
  { name: 'Billing', href: '/billing', icon: CreditCard, roles: ['Super Admin', 'Finance'] },
  { name: 'Support', href: '/support', icon: LifeBuoy, roles: ['Super Admin', 'Support Engineer', 'Developer'] },
  { name: 'Analytics', href: '/analytics', icon: BarChart3, roles: ['Super Admin', 'Developer', 'Support Engineer', 'Finance', 'Read Only'] },
  { name: 'Audit', href: '/audit', icon: ClipboardList, roles: ['Super Admin'] },
];

export default function Sidebar() {
  const pathname = usePathname();
  const { user, login, logout } = useRBAC();
  const [showRoleMenu, setShowRoleMenu] = React.useState(false);

  const availableRoles: UserRole[] = ['Super Admin', 'Developer', 'Support Engineer', 'Finance', 'Read Only'];

  const switchRole = (role: UserRole) => {
    if (user) {
      // Simulate switching role by calling login with mock password
      login(user.username, role);
      setShowRoleMenu(false);
    }
  };

  return (
    <aside className="w-64 bg-surface-container border-r border-outline-variant flex flex-col h-screen fixed left-0 top-0 z-20">
      {/* Brand Header */}
      <div className="h-16 flex items-center px-6 border-b border-outline-variant bg-surface-container-high">
        <div className="flex items-center space-x-3">
          <div className="w-8 h-8 rounded bg-primary flex items-center justify-center font-bold text-primary-foreground">
            Ω
          </div>
          <div>
            <h1 className="text-sm font-semibold tracking-wide text-foreground">MoatAgeex</h1>
            <p className="text-[10px] text-on-surfaceVariant uppercase tracking-widest font-mono">Control Plane</p>
          </div>
        </div>
      </div>

      {/* Navigation */}
      <nav className="flex-1 overflow-y-auto px-4 py-4 space-y-1">
        {navigationItems.map((item) => {
          const isActive = pathname === item.href;
          const hasAccess = user ? item.roles.includes(user.role) : false;

          if (!hasAccess) {
            // Disabled or hidden item based on RBAC
            return (
              <div
                key={item.name}
                className="flex items-center space-x-3 px-3 py-2 text-xs text-outline-variant cursor-not-allowed opacity-40 select-none"
                title="Access restricted by security policy"
              >
                <item.icon className="h-4 w-4" />
                <span>{item.name} (Restricted)</span>
              </div>
            );
          }

          return (
            <Link
              key={item.name}
              href={item.href}
              className={`flex items-center space-x-3 px-3 py-2 rounded-lg text-sm transition-all duration-150 ${
                isActive
                  ? 'bg-primary text-primary-foreground font-semibold shadow-sm shadow-primary/20'
                  : 'text-on-surfaceVariant hover:text-foreground hover:bg-surface-container-high'
              }`}
            >
              <item.icon className={`h-4 w-4 ${isActive ? 'text-primary-foreground' : 'text-outline'}`} />
              <span>{item.name}</span>
            </Link>
          );
        })}
      </nav>

      {/* User Session & Role Switcher */}
      <div className="p-4 border-t border-outline-variant bg-surface-container-low relative">
        <div className="flex items-center justify-between">
          <div className="flex flex-col min-w-0">
            <span className="text-xs font-semibold text-foreground truncate">{user?.username || 'Admin User'}</span>
            <button
              onClick={() => setShowRoleMenu(!showRoleMenu)}
              className="text-[10px] text-primary flex items-center space-x-1 hover:underline cursor-pointer focus:outline-none"
            >
              <span className="font-mono">{user?.role}</span>
              <ChevronDown className="h-3 w-3" />
            </button>
          </div>
          <button
            onClick={logout}
            className="p-1.5 text-outline hover:text-error rounded-lg hover:bg-surface-container-high transition-colors"
            title="Log Out"
          >
            <LogOut className="h-4 w-4" />
          </button>
        </div>

        {/* Mock Role Switcher Dropdown */}
        {showRoleMenu && (
          <div className="absolute bottom-16 left-4 right-4 bg-surface-container-highest border border-outline-variant rounded-lg shadow-xl py-2 z-30">
            <p className="px-3 py-1.5 text-[10px] font-semibold text-outline uppercase tracking-wider">
              Simulate Role
            </p>
            {availableRoles.map((role) => (
              <button
                key={role}
                onClick={() => switchRole(role)}
                className={`w-full text-left px-3 py-1.5 text-xs transition-colors hover:bg-primary hover:text-primary-foreground ${
                  user?.role === role ? 'text-primary font-semibold' : 'text-on-surface'
                }`}
              >
                {role}
              </button>
            ))}
          </div>
        )}
      </div>
    </aside>
  );
}
