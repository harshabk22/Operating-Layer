import { useState } from 'react';
import { Button } from '@/components/ui/button';

export default function Settings() {
  const [theme, setTheme] = useState('dark');
  const [tenantName, setTenantName] = useState('Moat Default Tenant');

  return (
    <div className="space-y-6 max-w-xl">
      <div>
        <h1 className="text-3xl font-bold tracking-tight">System Settings</h1>
        <p className="text-muted-foreground text-sm">Configure core tenant options, dashboard preferences, and display settings.</p>
      </div>

      <div className="border rounded-md bg-card p-6 space-y-6">
        <div className="space-y-2">
          <label className="text-sm font-medium">Tenant Display Name</label>
          <input 
            className="w-full p-2 border rounded-md bg-background text-sm" 
            value={tenantName} 
            onChange={e => setTenantName(e.target.value)} 
          />
        </div>

        <div className="space-y-2">
          <label className="text-sm font-medium">Display Theme</label>
          <select 
            className="w-full p-2 border rounded-md bg-background text-sm" 
            value={theme} 
            onChange={e => setTheme(e.target.value)}
          >
            <option value="light">Light Mode</option>
            <option value="dark">Dark Mode (Standard)</option>
            <option value="glass">Glassmorphic Accent</option>
          </select>
        </div>

        <Button onClick={() => alert('Settings saved successfully.')}>
          Save Configuration
        </Button>
      </div>
    </div>
  );
}
