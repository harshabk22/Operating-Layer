import { useState } from 'react';
import { useData } from '@/lib/api';
import { Card, CardHeader, CardTitle, CardContent } from '@/components/ui/card';

interface Role {
  name: string;
  description: string;
  defaultPermissions: string[];
}

const DEFAULT_ROLES: Role[] = [
  {
    name: 'Owner',
    description: 'Full administrative access to all features and packages.',
    defaultPermissions: ['*']
  },
  {
    name: 'Operator',
    description: 'Can manage daily transactions and operations.',
    defaultPermissions: ['sales.leads.read', 'sales.leads.write', 'realestate.properties.read', 'notifications.send']
  },
  {
    name: 'Viewer',
    description: 'Read-only access to all active business data.',
    defaultPermissions: ['sales.leads.read', 'realestate.properties.read']
  }
];

export default function Roles() {
  const { data: installedPackages, isLoading } = useData('/api/platform/packages');
  const [roles] = useState<Role[]>(DEFAULT_ROLES);

  // Dynamically extract all available package permissions
  const dynamicPermissions: string[] = installedPackages
    ? (installedPackages as Array<{ permissions?: string[] }>).flatMap((pkg) => pkg.permissions || [])
    : [];

  const isPermissionActive = (perm: string) => {
    if (perm === '*') return true;
    return dynamicPermissions.includes(perm);
  };

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-3xl font-bold tracking-tight">Role Management</h1>
        <p className="text-muted-foreground text-sm">
          Define access control roles and dynamically assign package permissions.
        </p>
      </div>

      {isLoading ? (
        <div className="p-8 text-center text-muted-foreground">Loading role registry...</div>
      ) : (
        <div className="grid gap-6 md:grid-cols-2 lg:grid-cols-3">
          {roles.map(role => (
            <Card key={role.name} className="flex flex-col justify-between">
              <CardHeader>
                <CardTitle className="text-lg">{role.name}</CardTitle>
                <p className="text-xs text-muted-foreground pt-1">{role.description}</p>
              </CardHeader>
              <CardContent className="space-y-4 flex-1">
                <div>
                  <h4 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground mb-2">
                    Default Capabilities
                  </h4>
                  <div className="flex flex-wrap gap-1.5">
                    {role.defaultPermissions.filter(isPermissionActive).map(perm => (
                      <span key={perm} className="text-[10px] px-2 py-0.5 rounded bg-muted border font-mono">
                        {perm}
                      </span>
                    ))}
                    {role.defaultPermissions.filter(isPermissionActive).length === 0 && (
                      <span className="text-[10px] text-muted-foreground italic">No default capabilities active</span>
                    )}
                  </div>
                </div>

                <div>
                  <h4 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground mb-2">
                    Dynamic Package Permissions
                  </h4>
                  {dynamicPermissions.length === 0 ? (
                    <p className="text-xs text-muted-foreground italic">No package permissions registered.</p>
                  ) : (
                    <div className="space-y-2">
                      {dynamicPermissions.map((perm: string) => {
                        const isGranted = role.name === 'Owner' || role.defaultPermissions.includes(perm);
                        return (
                          <div key={perm} className="flex items-center gap-2">
                            <input 
                              type="checkbox" 
                              checked={isGranted} 
                              disabled 
                              className="rounded border-gray-300 text-primary focus:ring-primary h-3.5 w-3.5" 
                            />
                            <span className="text-xs font-mono text-foreground/80">{perm}</span>
                          </div>
                        );
                      })}
                    </div>
                  )}
                </div>
              </CardContent>
            </Card>
          ))}
        </div>
      )}
    </div>
  );
}
