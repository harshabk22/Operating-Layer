import React, { createContext, useContext, useEffect, useState } from 'react';
import { useRouter } from 'next/navigation';
import axios from 'axios';
import { apiClient } from './api';

export type UserRole = 'Super Admin' | 'Developer' | 'Support Engineer' | 'Finance' | 'Read Only';

export interface User {
  username: string;
  role: UserRole;
  token?: string;
}

interface RBACContextType {
  user: User | null;
  login: (username: string, password: string) => Promise<void>;
  logout: () => void;
  hasRole: (allowedRoles: UserRole[]) => boolean;
  isLoading: boolean;
}

const RBACContext = createContext<RBACContextType | undefined>(undefined);

export function RBACProvider({ children }: { children: React.ReactNode }) {
  const [user, setUser] = useState<User | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const router = useRouter();

  useEffect(() => {
    if (typeof window !== 'undefined') {
      const stored = localStorage.getItem('moat_admin_user');
      if (stored) {
        try {
          const parsed = JSON.parse(stored);
          setUser(parsed);
          if (parsed.token) {
            apiClient.defaults.headers.common['Authorization'] = `Bearer ${parsed.token}`;
          }
        } catch (e) {
          console.error(e);
        }
      }
      setIsLoading(false);
    }
  }, []);

  const login = async (username: string, password: string) => {
    try {
      const response = await axios.post('/api/admin/auth/login', { username, password });
      const { token, user: userData } = response.data;
      const newUser: User = {
        username: userData.username,
        role: userData.role as UserRole,
        token
      };
      setUser(newUser);
      localStorage.setItem('moat_admin_user', JSON.stringify(newUser));
      apiClient.defaults.headers.common['Authorization'] = `Bearer ${token}`;
      router.push('/');
    } catch (e: any) {
      const errorMsg = e.response?.data?.error || e.message;
      alert(`Authentication failed: ${errorMsg}`);
      throw e;
    }
  };

  const logout = async () => {
    try {
      await axios.post('/api/admin/auth/logout');
    } catch (e) {
      console.error('Logout request failed', e);
    }
    setUser(null);
    localStorage.removeItem('moat_admin_user');
    delete apiClient.defaults.headers.common['Authorization'];
    router.push('/login');
  };

  const hasRole = (allowedRoles: UserRole[]) => {
    if (!user) return false;
    return allowedRoles.includes(user.role);
  };

  return (
    <RBACContext.Provider value={{ user, login, logout, hasRole, isLoading }}>
      {children}
    </RBACContext.Provider>
  );
}

export function useRBAC() {
  const context = useContext(RBACContext);
  if (!context) {
    throw new Error('useRBAC must be used within an RBACProvider');
  }
  return context;
}

export function RoleGuard({
  allowedRoles,
  fallback = null,
  children,
}: {
  allowedRoles: UserRole[];
  fallback?: React.ReactNode;
  children: React.ReactNode;
}) {
  const { user } = useRBAC();
  if (!user || !allowedRoles.includes(user.role)) {
    return <>{fallback}</>;
  }
  return <>{children}</>;
}

export function withRoleProtection(Component: React.ComponentType, allowedRoles: UserRole[]) {
  return function ProtectedRoute(props: any) {
    const { user, isLoading } = useRBAC();
    const router = useRouter();

    useEffect(() => {
      if (!isLoading && !user) {
        router.push('/login');
      }
    }, [user, isLoading, router]);

    if (isLoading || !user) {
      return (
        <div className="min-h-screen flex items-center justify-center bg-background text-muted-foreground text-sm">
          Loading Security Policies...
        </div>
      );
    }

    if (!allowedRoles.includes(user.role)) {
      return (
        <div className="min-h-screen flex flex-col items-center justify-center bg-background text-foreground text-center p-6 space-y-4">
          <div className="p-4 bg-error-container/10 text-error rounded-full border border-error-container/20 shadow-inner">
            <svg className="w-12 h-12" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 15v2m0-6v.01M5.93 19.36A9.003 9.003 0 0112 3a9.003 9.003 0 016.07 16.36M12 21a9 9 0 110-18 9 9 0 010 18z" />
            </svg>
          </div>
          <h2 className="text-2xl font-bold tracking-tight">Access Denied</h2>
          <p className="text-sm text-on-surfaceVariant max-w-md">
            Your current security role (<span className="font-semibold text-primary">{user.role}</span>) does not permit access to this module.
          </p>
          <button
            onClick={() => router.push('/')}
            className="px-4 py-2 bg-primary text-primary-foreground rounded-lg text-sm font-semibold hover:bg-primary/95 transition-all"
          >
            Return to Safety
          </button>
        </div>
      );
    }

    return <Component {...props} />;
  };
}
