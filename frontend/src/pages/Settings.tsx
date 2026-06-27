import { useState, useEffect } from 'react';
import { useData } from '@/lib/api';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { useToast } from '@/components/ui/use-toast';
import { Link } from 'react-router-dom';
import {
  Settings2,
  Brain,
  Plug,
  Sun,
  Moon,
  Calendar,
  Sheet,
  Phone,
  Send,
  CheckCircle2,
  XCircle,
  ExternalLink,
} from 'lucide-react';

type Tab = 'general' | 'ai' | 'integrations';

export default function Settings() {
  const { toast } = useToast();
  const { data: settings, isLoading } = useData('/api/system/settings');

  const [activeTab, setActiveTab] = useState<Tab>('general');

  // General tab state
  const [tenantName, setTenantName] = useState('');
  const [theme, setTheme] = useState(() =>
    document.documentElement.classList.contains('dark') ? 'dark' : 'light'
  );

  // AI Provider tab state
  const [provider, setProvider] = useState('google');
  const [chatModel, setChatModel] = useState('');
  const [plannerModel, setPlannerModel] = useState('');
  const [workerModel, setWorkerModel] = useState('');

  const [isSaving, setIsSaving] = useState(false);

  // Populate form state when API data loads
  useEffect(() => {
    if (settings) {
      setTenantName(settings.tenant_name || '');
      setProvider(settings.provider || 'google');
      setChatModel(settings.chat_model || '');
      setPlannerModel(settings.planner_model || '');
      setWorkerModel(settings.worker_model || '');
    }
  }, [settings]);

  // Apply theme to DOM and localStorage
  useEffect(() => {
    if (theme === 'dark') {
      document.documentElement.classList.add('dark');
    } else {
      document.documentElement.classList.remove('dark');
    }
    localStorage.setItem('moat_theme', theme);
  }, [theme]);

  const handleSave = async (payload: Record<string, unknown>) => {
    setIsSaving(true);
    try {
      const res = await fetch('/api/system/settings', {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload),
      });
      if (!res.ok) throw new Error('Failed');
      toast({ title: 'Success', description: 'Settings saved successfully.' });
    } catch {
      toast({ title: 'Error', description: 'Failed to save settings.', variant: 'destructive' });
    } finally {
      setIsSaving(false);
    }
  };

  const tabs: { key: Tab; label: string; icon: React.ReactNode }[] = [
    { key: 'general', label: 'General', icon: <Settings2 className="h-4 w-4" /> },
    { key: 'ai', label: 'AI Provider', icon: <Brain className="h-4 w-4" /> },
    { key: 'integrations', label: 'Integrations', icon: <Plug className="h-4 w-4" /> },
  ];

  if (isLoading) {
    return <div className="p-8 text-center text-muted-foreground">Loading settings...</div>;
  }

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-3xl font-bold tracking-tight">System Settings</h1>
        <p className="text-muted-foreground text-sm mt-1">
          Configure tenant, AI provider, and integration settings.
        </p>
      </div>

      {/* Tab navigation */}
      <div className="flex gap-1 border-b">
        {tabs.map((tab) => (
          <button
            key={tab.key}
            onClick={() => setActiveTab(tab.key)}
            className={`flex items-center gap-2 px-4 py-2.5 text-sm font-medium transition-colors border-b-2 -mb-px ${
              activeTab === tab.key
                ? 'border-primary text-primary'
                : 'border-transparent text-muted-foreground hover:text-foreground hover:border-border'
            }`}
          >
            {tab.icon}
            {tab.label}
          </button>
        ))}
      </div>

      {/* Tab: General */}
      {activeTab === 'general' && (
        <div className="space-y-6 max-w-2xl">
          <div className="border rounded-md bg-card p-6 space-y-6">
            <div className="space-y-2">
              <label className="text-sm font-medium">Tenant Display Name</label>
              <input
                className="w-full p-2 border rounded-md bg-background text-sm focus:outline-none focus:ring-1 focus:ring-primary"
                value={tenantName}
                onChange={(e) => setTenantName(e.target.value)}
                placeholder="Enter tenant name"
              />
            </div>

            <div className="space-y-2">
              <label className="text-sm font-medium">Display Theme</label>
              <div className="flex gap-2">
                <button
                  onClick={() => setTheme('light')}
                  className={`flex-1 flex items-center justify-center gap-2 p-3 border rounded-md text-sm font-medium transition-colors ${
                    theme === 'light'
                      ? 'border-primary bg-primary/5 text-primary'
                      : 'border-border bg-background text-muted-foreground hover:bg-accent'
                  }`}
                >
                  <Sun className="h-4 w-4" />
                  Light
                </button>
                <button
                  onClick={() => setTheme('dark')}
                  className={`flex-1 flex items-center justify-center gap-2 p-3 border rounded-md text-sm font-medium transition-colors ${
                    theme === 'dark'
                      ? 'border-primary bg-primary/5 text-primary'
                      : 'border-border bg-background text-muted-foreground hover:bg-accent'
                  }`}
                >
                  <Moon className="h-4 w-4" />
                  Dark
                </button>
              </div>
            </div>

            <Button
              onClick={() => handleSave({ tenant_name: tenantName })}
              disabled={isSaving}
            >
              {isSaving ? 'Saving...' : 'Save Changes'}
            </Button>
          </div>

          {/* System stats */}
          {settings?.stats && (
            <div>
              <h2 className="text-sm font-semibold text-muted-foreground uppercase tracking-wider mb-3">
                System Statistics
              </h2>
              <div className="grid gap-4 md:grid-cols-3">
                <Card>
                  <CardHeader className="pb-2">
                    <CardTitle className="text-sm font-medium">Total Threads</CardTitle>
                  </CardHeader>
                  <CardContent>
                    <div className="text-2xl font-bold">{settings.stats.total_threads ?? 0}</div>
                  </CardContent>
                </Card>
                <Card>
                  <CardHeader className="pb-2">
                    <CardTitle className="text-sm font-medium">Contacts</CardTitle>
                  </CardHeader>
                  <CardContent>
                    <div className="text-2xl font-bold">{settings.stats.contacts ?? 0}</div>
                  </CardContent>
                </Card>
                <Card>
                  <CardHeader className="pb-2">
                    <CardTitle className="text-sm font-medium">Events</CardTitle>
                  </CardHeader>
                  <CardContent>
                    <div className="text-2xl font-bold">{settings.stats.events ?? 0}</div>
                  </CardContent>
                </Card>
              </div>
            </div>
          )}
        </div>
      )}

      {/* Tab: AI Provider */}
      {activeTab === 'ai' && (
        <div className="space-y-6 max-w-2xl">
          <div className="border rounded-md bg-card p-6 space-y-6">
            <div className="space-y-2">
              <label className="text-sm font-medium">AI Provider</label>
              <select
                className="w-full p-2 border rounded-md bg-background text-sm focus:outline-none focus:ring-1 focus:ring-primary"
                value={provider}
                onChange={(e) => setProvider(e.target.value)}
              >
                <option value="google">Google (Gemini)</option>
                <option value="deepseek">DeepSeek</option>
              </select>
            </div>

            <div className="space-y-2">
              <label className="text-sm font-medium">Chat Model</label>
              <input
                className="w-full p-2 border rounded-md bg-background text-sm focus:outline-none focus:ring-1 focus:ring-primary"
                value={chatModel}
                onChange={(e) => setChatModel(e.target.value)}
                placeholder="e.g. gemini-2.0-flash"
              />
              <p className="text-xs text-muted-foreground">
                Model used for conversational responses.
              </p>
            </div>

            <div className="space-y-2">
              <label className="text-sm font-medium">Planner Model</label>
              <input
                className="w-full p-2 border rounded-md bg-background text-sm focus:outline-none focus:ring-1 focus:ring-primary"
                value={plannerModel}
                onChange={(e) => setPlannerModel(e.target.value)}
                placeholder="e.g. gemini-2.0-flash"
              />
              <p className="text-xs text-muted-foreground">
                Model used for planning multi-step workflows.
              </p>
            </div>

            <div className="space-y-2">
              <label className="text-sm font-medium">Worker Model</label>
              <input
                className="w-full p-2 border rounded-md bg-background text-sm focus:outline-none focus:ring-1 focus:ring-primary"
                value={workerModel}
                onChange={(e) => setWorkerModel(e.target.value)}
                placeholder="e.g. gemini-2.0-flash"
              />
              <p className="text-xs text-muted-foreground">
                Model used for executing individual tasks.
              </p>
            </div>

            <Button
              onClick={() =>
                handleSave({
                  provider,
                  chat_model: chatModel,
                  planner_model: plannerModel,
                  worker_model: workerModel,
                })
              }
              disabled={isSaving}
            >
              {isSaving ? 'Saving...' : 'Save AI Configuration'}
            </Button>
          </div>
        </div>
      )}

      {/* Tab: Integrations */}
      {activeTab === 'integrations' && (
        <div className="space-y-6">
          <div className="grid gap-4 md:grid-cols-2">
            <IntegrationCard
              icon={<Calendar className="h-5 w-5 text-blue-500" />}
              title="Google Calendar"
              description="Sync calendar events and manage bookings."
              connected={settings?.has_google ?? false}
            />
            <IntegrationCard
              icon={<Phone className="h-5 w-5 text-green-500" />}
              title="WhatsApp"
              description="Receive and send WhatsApp messages."
              connected={settings?.has_whatsapp ?? false}
            />
            <IntegrationCard
              icon={<Send className="h-5 w-5 text-sky-500" />}
              title="Telegram"
              description="Connect a Telegram bot for messaging."
              connected={settings?.has_telegram ?? false}
            />
            <IntegrationCard
              icon={<Sheet className="h-5 w-5 text-emerald-500" />}
              title="Google Sheets"
              description="Sync data to and from Google Sheets."
              connected={settings?.has_google ?? false}
            />
          </div>

          <div className="border rounded-md bg-card p-4 flex items-center justify-between">
            <div>
              <p className="text-sm font-medium">Manage Connections</p>
              <p className="text-xs text-muted-foreground">
                Configure, connect, or disconnect integrations on the Connections page.
              </p>
            </div>
            <Link to="/integrations">
              <Button variant="outline" size="sm" className="gap-1.5">
                <ExternalLink className="h-3.5 w-3.5" />
                Go to Integrations
              </Button>
            </Link>
          </div>
        </div>
      )}
    </div>
  );
}

function IntegrationCard({
  icon,
  title,
  description,
  connected,
}: {
  icon: React.ReactNode;
  title: string;
  description: string;
  connected: boolean;
}) {
  return (
    <Card>
      <CardContent className="p-5">
        <div className="flex items-start justify-between">
          <div className="flex items-center gap-3">
            <div className="p-2 rounded-md bg-accent/50">{icon}</div>
            <div>
              <p className="text-sm font-medium">{title}</p>
              <p className="text-xs text-muted-foreground mt-0.5">{description}</p>
            </div>
          </div>
          <div className="flex items-center gap-1.5">
            {connected ? (
              <>
                <CheckCircle2 className="h-4 w-4 text-green-500" />
                <span className="text-xs font-medium text-green-500">Connected</span>
              </>
            ) : (
              <>
                <XCircle className="h-4 w-4 text-muted-foreground" />
                <span className="text-xs font-medium text-muted-foreground">Disconnected</span>
              </>
            )}
          </div>
        </div>
      </CardContent>
    </Card>
  );
}
