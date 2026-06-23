import { useState, useEffect } from 'react';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { useToast } from '@/components/ui/use-toast';
import { Link2, Unlink, CheckCircle2, AlertCircle, RefreshCw, Smartphone } from 'lucide-react';

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
      }
    } catch (err) {
      console.error(err);
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

    const interval = setInterval(() => {
      fetchWhatsAppStatus();
    }, 5000);

    return () => clearInterval(interval);
  }, []);

  const handleGoogleConnect = async () => {
    try {
      const res = await fetch('/api/connections/google/connect', { method: 'POST' });
      if (res.ok) {
        const data = await res.json();
        if (data.url) {
          window.location.href = data.url;
        }
      }
    } catch (err) {
      console.error(err);
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

            <div className="p-4 border rounded-lg bg-accent/10 min-h-[300px] flex flex-col justify-center">
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
