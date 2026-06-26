import { useState, useEffect } from 'react';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { useToast } from '@/components/ui/use-toast';
import { Link2, Unlink, CheckCircle2, AlertCircle, RefreshCw, Smartphone, Settings } from 'lucide-react';

interface GoogleStatus {
  status: string;
  health: string;
  account: string;
  last_sync: string;
  config?: {
    client_id?: string;
    selected_spreadsheet_id?: string;
  };
}

interface WhatsAppStatus {
  status: string;
  message?: string;
  phone_number_id?: string;
  port?: number;
  custom_ports?: string;
  retry_settings?: string;
}

export default function Connections() {
  const { toast } = useToast();
  const [googleStatus, setGoogleStatus] = useState<GoogleStatus | null>(null);
  const [whatsappStatus, setWhatsappStatus] = useState<WhatsAppStatus | null>(null);
  const [qrTimestamp, setQrTimestamp] = useState(Date.now());
  const [calendars, setCalendars] = useState<any[]>([]);
  const [spreadsheets, setSpreadsheets] = useState<any[]>([]);
  const [selectedCalendar, setSelectedCalendar] = useState('');
  const [selectedSpreadsheet, setSelectedSpreadsheet] = useState('');
  const [selectedWorksheet, setSelectedWorksheet] = useState('Sheet1');
  const [isSaving, setIsSaving] = useState(false);

  // WhatsApp states
  const [showWhatsAppSettings, setShowWhatsAppSettings] = useState(false);
  const [phoneNumberId, setPhoneNumberId] = useState('');
  const [whatsappPort, setWhatsappPort] = useState(3003);
  const [customPorts, setCustomPorts] = useState('');
  const [retrySettings, setRetrySettings] = useState('default');
  const [isSavingWhatsAppSettings, setIsSavingWhatsAppSettings] = useState(false);
  const [whatsappSettingsLoaded, setWhatsappSettingsLoaded] = useState(false);

  // Google OAuth Credentials states
  const [showCredentials, setShowCredentials] = useState(false);
  const [clientId, setClientId] = useState('');
  const [clientSecret, setClientSecret] = useState('');
  const [redirectUri, setRedirectUri] = useState('');
  const [isSavingCreds, setIsSavingCreds] = useState(false);
  const [oauthStatus, setOauthStatus] = useState<string | null>(null);

  const fetchGoogleStatus = async () => {
    try {
      const res = await fetch('/api/connections/google/status');
      if (res.ok) {
        const data = await res.json();
        setGoogleStatus(data);
        if (data.status === 'Connected') {
          fetchResources();
        }
      }
    } catch (err) {
      console.error(err);
    }
  };

  const fetchWhatsAppStatus = async () => {
    try {
      const res = await fetch('/api/connectors/whatsapp/status');
      if (res.ok) {
        const data = await res.json();
        setWhatsappStatus(data);
        if (data.status === 'pairing') {
          setQrTimestamp(Date.now());
        }
        if (!whatsappSettingsLoaded && data.phone_number_id !== undefined) {
          setPhoneNumberId(data.phone_number_id);
          setWhatsappPort(data.port || 3003);
          setCustomPorts(data.custom_ports || '');
          setRetrySettings(data.retry_settings || 'default');
          setWhatsappSettingsLoaded(true);
        }
      }
    } catch (err) {
      console.error(err);
    }
  };

  const fetchOAuthConfig = async () => {
    try {
      const res = await fetch('/api/connections/google/oauth_config');
      if (res.ok) {
        const data = await res.json();
        setClientId(data.client_id || '');
        setRedirectUri(data.redirect_uri || '');
        if (data.client_secret_configured) {
          setClientSecret('********');
        }
      }
    } catch (err) {
      console.error('Error fetching OAuth config:', err);
    }
  };

  const fetchResources = async () => {
    try {
      const res = await fetch('/api/connections/google/resources');
      if (res.ok) {
        const data = await res.json();
        setCalendars(data.calendars?.items || []);
        setSpreadsheets(data.spreadsheets?.items || []);
      }
    } catch (err) {
      console.error(err);
    }
  };

  useEffect(() => {
    fetchGoogleStatus();
    fetchWhatsAppStatus();
    fetchOAuthConfig();

    const interval = setInterval(() => {
      fetchWhatsAppStatus();
    }, 5000);

    return () => clearInterval(interval);
  }, []);

  const handleGoogleConnect = async () => {
    setOauthStatus('OAuth redirecting to Google...');
    try {
      const res = await fetch('/api/connections/google/connect', { method: 'POST' });
      if (res.ok) {
        const data = await res.json();
        if (data.url) {
          window.location.href = data.url;
        } else {
          setOauthStatus('Failed to get authorization URL.');
        }
      } else {
        setOauthStatus('Failed to initiate login.');
      }
    } catch (err) {
      console.error(err);
      setOauthStatus('Network error.');
    }
  };

  const handleGoogleDisconnect = async () => {
    try {
      const res = await fetch('/api/connections/google/disconnect', { method: 'POST' });
      if (res.ok) {
        toast({ title: 'Disconnected', description: 'Google Workspace disconnected.' });
        fetchGoogleStatus();
        setCalendars([]);
        setSpreadsheets([]);
      }
    } catch (err) {
      console.error(err);
    }
  };

  const handleSaveGoogleConfig = async () => {
    setIsSaving(true);
    try {
      const res = await fetch('/api/connections/google/config', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          calendar_id: selectedCalendar || null,
          spreadsheet_id: selectedSpreadsheet || null,
          worksheet_id: selectedWorksheet || null,
        }),
      });
      if (res.ok) {
        toast({ title: 'Success', description: 'Google configuration saved.' });
        fetchGoogleStatus();
      } else {
        toast({ title: 'Error', description: 'Failed to save configuration', variant: 'destructive' });
      }
    } catch (err) {
      console.error(err);
    } finally {
      setIsSaving(false);
    }
  };

  const handleSaveCredentials = async () => {
    setIsSavingCreds(true);
    try {
      const res = await fetch('/api/connections/google/oauth_config', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          client_id: clientId,
          client_secret: clientSecret === '********' ? null : clientSecret,
          redirect_uri: redirectUri,
        }),
      });
      if (res.ok) {
        toast({ title: 'Success', description: 'Production credentials saved. Existing Google link was cleared, please re-authenticate.' });
        fetchOAuthConfig();
        fetchGoogleStatus();
      } else {
        toast({ title: 'Error', description: 'Failed to save credentials.', variant: 'destructive' });
      }
    } catch (err) {
      console.error(err);
      toast({ title: 'Error', description: 'Unexpected error saving credentials.', variant: 'destructive' });
    } finally {
      setIsSavingCreds(false);
    }
  };

  const handleWhatsAppDisconnect = async () => {
    try {
      const res = await fetch('/api/connectors/whatsapp/disconnect', { method: 'POST' });
      if (res.ok) {
        toast({ title: 'Disconnected', description: 'WhatsApp session cleared.' });
        fetchWhatsAppStatus();
      } else {
        toast({ title: 'Error', description: 'Failed to disconnect WhatsApp.', variant: 'destructive' });
      }
    } catch (err) {
      console.error(err);
    }
  };

  const handleWhatsAppConnect = async () => {
    try {
      const res = await fetch('/api/connectors/whatsapp/connect', { method: 'POST' });
      if (res.ok) {
        toast({ title: 'Connecting', description: 'Starting WhatsApp connection/pairing loop...' });
        fetchWhatsAppStatus();
      } else {
        toast({ title: 'Error', description: 'Failed to trigger connection.', variant: 'destructive' });
      }
    } catch (err) {
      console.error(err);
    }
  };

  const handleSaveWhatsAppSettings = async () => {
    setIsSavingWhatsAppSettings(true);
    try {
      const res = await fetch('/api/connectors/whatsapp/settings', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          phone_number_id: phoneNumberId,
          port: Number(whatsappPort),
          custom_ports: customPorts,
          retry_settings: retrySettings,
        }),
      });
      if (res.ok) {
        toast({ title: 'Success', description: 'WhatsApp settings saved.' });
        fetchWhatsAppStatus();
      } else {
        toast({ title: 'Error', description: 'Failed to save settings.', variant: 'destructive' });
      }
    } catch (err) {
      console.error(err);
    } finally {
      setIsSavingWhatsAppSettings(false);
    }
  };

  return (
    <div className="space-y-8 max-w-4xl mx-auto p-4 text-foreground">
      <div className="flex justify-between items-center">
        <h1 className="text-3xl font-bold">Workspace Connections</h1>
        <Button variant="outline" size="sm" onClick={() => {
          fetchGoogleStatus();
          fetchWhatsAppStatus();
        }}>
          <RefreshCw className="h-4 w-4 mr-1" /> Refresh
        </Button>
      </div>

      <div className="grid gap-6 md:grid-cols-2">
        {/* WhatsApp Personal Section */}
        <Card className="flex flex-col bg-card">
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Smartphone className="h-5 w-5 text-green-500" />
              WhatsApp Personal Account
            </CardTitle>
          </CardHeader>
          <CardContent className="flex-1 flex flex-col justify-between space-y-4">
            <p className="text-sm text-muted-foreground">
              Link your personal WhatsApp account by scanning the QR code below using your mobile app.
            </p>

            <div className="flex flex-col items-center justify-center p-4 border rounded-lg bg-accent/10 min-h-[300px]">
              {whatsappStatus ? (
                <>
                  <div className="font-semibold text-sm mb-4 px-3 py-1.5 rounded-full border bg-accent/40">
                    Status: {whatsappStatus.status.toUpperCase()}
                  </div>
                  {whatsappStatus.message && (
                    <div className="text-xs text-muted-foreground mb-4 text-center italic">
                      {whatsappStatus.message}
                    </div>
                  )}

                  {whatsappStatus.status === 'pairing' && (
                    <div className="bg-white p-3 rounded-lg shadow-md mb-4 border-2 border-primary/20">
                      <img 
                        src={`/api/connectors/whatsapp/qr?t=${qrTimestamp}`} 
                        alt="WhatsApp QR Code" 
                        className="w-48 h-48 block" 
                      />
                    </div>
                  )}

                  {whatsappStatus.status === 'connected' && (
                    <div className="text-green-500 flex flex-col items-center gap-2 my-8">
                      <CheckCircle2 className="h-16 w-16" />
                      <span className="font-bold text-lg">Connected successfully</span>
                    </div>
                  )}

                  {whatsappStatus.status !== 'pairing' && whatsappStatus.status !== 'connected' && (
                    <div className="text-muted-foreground flex flex-col items-center gap-2 my-8">
                      <AlertCircle className="h-16 w-16" />
                      <span>Disconnected or Initializing...</span>
                    </div>
                  )}
                </>
              ) : (
                <div className="text-muted-foreground text-sm">Checking WhatsApp Status...</div>
              )}
            </div>

            {whatsappStatus && (
              <div className="flex gap-2">
                {whatsappStatus.status !== 'unpaired' && (
                  <Button variant="destructive" size="sm" className="flex-1" onClick={handleWhatsAppDisconnect}>
                    <Unlink className="h-4 w-4 mr-1" /> Disconnect
                  </Button>
                )}
                {(whatsappStatus.status === 'unpaired' || whatsappStatus.status === 'reconnecting' || whatsappStatus.status === 'disconnected') && (
                  <Button variant="default" size="sm" className="flex-1" onClick={handleWhatsAppConnect}>
                    <RefreshCw className="h-4 w-4 mr-1" /> Connect / Retry
                  </Button>
                )}
              </div>
            )}

            <div className="pt-2 border-t">
              <Button 
                variant="outline" 
                size="sm" 
                className="w-full text-xs flex items-center justify-center gap-1"
                onClick={() => setShowWhatsAppSettings(!showWhatsAppSettings)}
              >
                <Settings className="h-3 w-3" />
                {showWhatsAppSettings ? 'Hide Configurations' : 'Configure Settings'}
              </Button>
            </div>

            {showWhatsAppSettings && (
              <div className="space-y-3 pt-2 border-t text-left">
                <h4 className="text-xs font-semibold text-foreground uppercase tracking-wider">WhatsApp Connector Settings</h4>
                
                <div>
                  <label className="text-xs text-muted-foreground font-medium">Phone Number ID</label>
                  <input 
                    type="text" 
                    value={phoneNumberId} 
                    onChange={e => setPhoneNumberId(e.target.value)}
                    className="w-full p-2 mt-1 border rounded bg-background text-sm text-foreground focus:outline-none focus:ring-1 focus:ring-primary" 
                    placeholder="e.g. 10423472948719"
                  />
                </div>

                <div className="grid grid-cols-2 gap-2">
                  <div>
                    <label className="text-xs text-muted-foreground font-medium">Bridge Port</label>
                    <input 
                      type="number" 
                      value={whatsappPort} 
                      onChange={e => setWhatsappPort(Number(e.target.value))}
                      className="w-full p-2 mt-1 border rounded bg-background text-sm text-foreground focus:outline-none focus:ring-1 focus:ring-primary" 
                      placeholder="3003"
                    />
                  </div>
                  <div>
                    <label className="text-xs text-muted-foreground font-medium">Retry Strategy</label>
                    <select
                      value={retrySettings}
                      onChange={e => setRetrySettings(e.target.value)}
                      className="w-full p-2 mt-1 border rounded bg-background text-sm text-foreground focus:outline-none focus:ring-1 focus:ring-primary"
                    >
                      <option value="default">Default</option>
                      <option value="aggressive">Aggressive</option>
                      <option value="exponential">Exponential Backoff</option>
                    </select>
                  </div>
                </div>

                <div>
                  <label className="text-xs text-muted-foreground font-medium">Custom Ports (Fallback)</label>
                  <input 
                    type="text" 
                    value={customPorts} 
                    onChange={e => setCustomPorts(e.target.value)}
                    className="w-full p-2 mt-1 border rounded bg-background text-sm text-foreground focus:outline-none focus:ring-1 focus:ring-primary" 
                    placeholder="e.g. 3004, 3005"
                  />
                </div>

                <Button className="w-full mt-2" size="sm" onClick={handleSaveWhatsAppSettings} disabled={isSavingWhatsAppSettings}>
                  {isSavingWhatsAppSettings ? 'Saving...' : 'Save Settings'}
                </Button>
              </div>
            )}

            <div className="text-[11px] text-muted-foreground leading-relaxed">
              Open WhatsApp on your phone, go to **Settings &gt; Linked Devices**, and tap **Link a Device** to scan the QR code.
            </div>
          </CardContent>
        </Card>

        {/* Google Workspace Section */}
        <Card className="flex flex-col bg-card">
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Link2 className="h-5 w-5 text-blue-500" />
              Google Workspace Integration
            </CardTitle>
          </CardHeader>
          <CardContent className="flex-1 flex flex-col justify-between space-y-4">
            <p className="text-sm text-muted-foreground">
              Connect Google Calendar and Sheets to enable Moat's scheduling and analytics capabilities.
            </p>

            {/* Collapsible Credentials Form */}
            <div className="border rounded-lg p-3 bg-accent/5">
              <button
                type="button"
                onClick={() => setShowCredentials(!showCredentials)}
                className="flex items-center justify-between w-full text-xs font-semibold uppercase tracking-wider text-muted-foreground hover:text-foreground"
              >
                <span>Google Production Credentials</span>
                <span>{showCredentials ? '▲' : '▼'}</span>
              </button>
              
              {showCredentials && (
                <div className="mt-3 space-y-3">
                  <div>
                    <label className="text-xs font-medium text-muted-foreground">Client ID</label>
                    <input
                      type="text"
                      value={clientId}
                      onChange={e => setClientId(e.target.value)}
                      placeholder="Enter Google Client ID"
                      className="w-full p-2 mt-1 border rounded bg-background text-sm text-foreground focus:outline-none focus:ring-1 focus:ring-primary"
                    />
                  </div>
                  <div>
                    <label className="text-xs font-medium text-muted-foreground">Client Secret</label>
                    <input
                      type="password"
                      value={clientSecret}
                      onChange={e => setClientSecret(e.target.value)}
                      placeholder="Enter Google Client Secret"
                      className="w-full p-2 mt-1 border rounded bg-background text-sm text-foreground focus:outline-none focus:ring-1 focus:ring-primary"
                    />
                  </div>
                  <div>
                    <label className="text-xs font-medium text-muted-foreground">Redirect URI</label>
                    <input
                      type="text"
                      value={redirectUri}
                      onChange={e => setRedirectUri(e.target.value)}
                      placeholder="e.g. http://localhost:8080/oauth/callback"
                      className="w-full p-2 mt-1 border rounded bg-background text-sm text-foreground focus:outline-none focus:ring-1 focus:ring-primary"
                    />
                  </div>
                  <Button 
                    className="w-full text-xs" 
                    size="sm" 
                    onClick={handleSaveCredentials} 
                    disabled={isSavingCreds}
                  >
                    {isSavingCreds ? 'Saving...' : 'Save Credentials'}
                  </Button>
                </div>
              )}
            </div>

            <div className="p-4 border rounded-lg bg-accent/10 min-h-[300px] flex flex-col justify-center">
              {oauthStatus && (
                <div className="text-xs text-blue-500 animate-pulse font-medium text-center py-2 italic">
                  {oauthStatus}
                </div>
              )}
              {googleStatus ? (
                googleStatus.status === 'Connected' ? (
                  <div className="space-y-4">
                    <div className="flex justify-between items-center border-b pb-2">
                      <div>
                        <span className="text-xs text-muted-foreground block">Linked Account</span>
                        <span className="font-medium text-sm">{googleStatus.account}</span>
                      </div>
                      <span className={`text-xs px-2.5 py-1 rounded-full font-semibold border ${
                        googleStatus.health === 'Good' ? 'bg-green-500/10 text-green-500 border-green-500/20' : 'bg-yellow-500/10 text-yellow-500 border-yellow-500/20'
                      }`}>
                        {googleStatus.health}
                      </span>
                    </div>

                    <div className="space-y-3">
                      <div>
                        <label className="text-xs text-muted-foreground font-medium">Calendar</label>
                        <select 
                          value={selectedCalendar} 
                          onChange={e => setSelectedCalendar(e.target.value)}
                          className="w-full p-2 mt-1 border rounded bg-background text-sm text-foreground"
                        >
                          <option value="">Select a calendar</option>
                          {calendars.map(cal => (
                            <option key={cal.id} value={cal.id}>{cal.summary || cal.id}</option>
                          ))}
                        </select>
                      </div>

                      <div>
                        <label className="text-xs text-muted-foreground font-medium">Spreadsheet</label>
                        <select 
                          value={selectedSpreadsheet} 
                          onChange={e => setSelectedSpreadsheet(e.target.value)}
                          className="w-full p-2 mt-1 border rounded bg-background text-sm text-foreground"
                        >
                          <option value="">Select a spreadsheet</option>
                          {spreadsheets.map(sheet => (
                            <option key={sheet.id} value={sheet.id}>{sheet.summary || sheet.id}</option>
                          ))}
                        </select>
                      </div>

                      <div>
                        <label className="text-xs text-muted-foreground font-medium">Worksheet Title</label>
                        <input 
                          type="text" 
                          value={selectedWorksheet} 
                          onChange={e => setSelectedWorksheet(e.target.value)}
                          className="w-full p-2 mt-1 border rounded bg-background text-sm text-foreground focus:outline-none focus:ring-1 focus:ring-primary" 
                          placeholder="e.g. Sheet1"
                        />
                      </div>

                      <Button className="w-full mt-2" size="sm" onClick={handleSaveGoogleConfig} disabled={isSaving}>
                        {isSaving ? 'Saving...' : 'Save Configuration'}
                      </Button>
                    </div>
                  </div>
                ) : (
                  <div className="text-center py-8">
                    <Button onClick={handleGoogleConnect} className="flex items-center justify-center gap-1.5 mx-auto">
                      <Link2 className="h-4 w-4" /> Link Google Account
                    </Button>
                  </div>
                )
              ) : (
                <div className="text-center text-muted-foreground text-sm">Checking Google connection status...</div>
              )}
            </div>

            {googleStatus && googleStatus.status === 'Connected' && (
              <Button variant="destructive" size="sm" onClick={handleGoogleDisconnect} className="w-full flex items-center justify-center gap-1">
                <Unlink className="h-4 w-4" /> Disconnect Account
              </Button>
            )}
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
