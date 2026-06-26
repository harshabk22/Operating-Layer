import { useState, useEffect } from 'react';
import { Button } from '@/components/ui/button';
import { useToast } from '@/components/ui/use-toast';

export default function Settings() {
  const { toast } = useToast();
  const [theme, setTheme] = useState(() => {
    return document.documentElement.classList.contains('dark') ? 'dark' : 'light';
  });
  const [tenantName, setTenantName] = useState('Moat Default Tenant');
  const [isSaving, setIsSaving] = useState(false);

  useEffect(() => {
    if (theme === 'dark') {
      document.documentElement.classList.add('dark');
    } else {
      document.documentElement.classList.remove('dark');
    }
  }, [theme]);

  const handleSave = async () => {
    setIsSaving(true);
    try {
      // Persist settings via API if available, otherwise save locally
      localStorage.setItem('moat_theme', theme);
      localStorage.setItem('moat_tenant_name', tenantName);
      toast({ title: 'Success', description: 'Settings saved successfully.' });
    } catch (err) {
      toast({ title: 'Error', description: 'Failed to save settings.', variant: 'destructive' });
    } finally {
      setIsSaving(false);
    }
  };

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
          </select>
        </div>

        <Button onClick={handleSave} disabled={isSaving}>
          {isSaving ? 'Saving...' : 'Save Configuration'}
        </Button>
      </div>
    </div>
  );
}

