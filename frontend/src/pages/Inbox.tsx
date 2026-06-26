import { useState, useEffect, useRef } from 'react';
import { Button } from '@/components/ui/button';
import { Send, User, Phone, Bot, Check, ShieldAlert, Pause, Play } from 'lucide-react';
import { useToast } from '@/components/ui/use-toast';

interface Message {
  id: string;
  thread_id: string;
  sender: string;
  content: string;
  timestamp: number;
}

interface Thread {
  id: string;
  contact_id: string;
  status: string;
  opportunity_status: string;
  created_at: number;
  updated_at: number;
  control_mode: string;
  assigned_operator: string | null;
  is_paused: boolean | number;
}

interface Contact {
  id: string;
  name: string | null;
  email: string | null;
  phone: string | null;
}

interface Suggestion {
  suggested_reply: string;
  confidence: number;
  reasoning_summary: string;
}

interface ThreadDetail {
  thread: Thread;
  contact: Contact;
  messages: Message[];
  suggestion: Suggestion | null;
}

export default function Inbox() {
  const [threads, setThreads] = useState<Thread[]>([]);
  const [activeThreadId, setActiveThreadId] = useState<string | null>(null);
  const [threadDetail, setThreadDetail] = useState<ThreadDetail | null>(null);
  const [messageText, setMessageText] = useState('');
  const [isLoading, setIsLoading] = useState(true);
  const chatBodyRef = useRef<HTMLDivElement>(null);
  const { toast } = useToast();
  const activeThreadIdRef = useRef<string | null>(null);

  useEffect(() => {
    activeThreadIdRef.current = activeThreadId;
  }, [activeThreadId]);

  const fetchThreads = async () => {
    try {
      const res = await fetch('/api/threads');
      if (res.status === 401 || res.status === 403) {
        localStorage.removeItem('auth');
        window.location.href = '/login';
        return;
      }
      if (res.ok) {
        const data = await res.json();
        setThreads(data);
      }
    } catch (err) {
      console.error('Failed to fetch threads', err);
    } finally {
      setIsLoading(false);
    }
  };

  const fetchThreadDetail = async (id: string) => {
    try {
      const res = await fetch(`/api/threads/${id}`);
      if (res.ok) {
        const data = await res.json();
        setThreadDetail(data);
      }
    } catch (err) {
      console.error('Failed to fetch thread detail', err);
    }
  };

  useEffect(() => {
    fetchThreads();

    let socket: WebSocket | null = null;
    let reconnectTimeout: NodeJS.Timeout | null = null;
    let isMounted = true;

    const connectWs = () => {
      if (!isMounted) return;
      
      const wsProtocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
      const wsHost = window.location.port === '3000' 
        ? `${window.location.hostname}:8080` 
        : window.location.host;
      
      socket = new WebSocket(`${wsProtocol}//${wsHost}/ws`);

      socket.onmessage = (event) => {
        try {
          const evt = JSON.parse(event.data);
          if (evt.type === 'new_message') {
            fetchThreads();
            const currentActiveId = activeThreadIdRef.current;
            if (currentActiveId && evt.data && evt.data.thread_id === currentActiveId) {
              fetchThreadDetail(currentActiveId);
            }
          }
        } catch (err) {
          console.error('Error parsing WS message', err);
        }
      };

      socket.onerror = (err) => {
        console.error('WebSocket connection error:', err);
      };

      socket.onclose = () => {
        console.log('WebSocket connection closed. Retrying in 3 seconds...');
        if (isMounted) {
          reconnectTimeout = setTimeout(connectWs, 3000);
        }
      };
    };

    connectWs();

    return () => {
      isMounted = false;
      if (socket) {
        socket.close();
      }
      if (reconnectTimeout) {
        clearTimeout(reconnectTimeout);
      }
    };
  }, []);

  useEffect(() => {
    if (activeThreadId) {
      fetchThreadDetail(activeThreadId);
    } else {
      setThreadDetail(null);
    }
  }, [activeThreadId]);

  useEffect(() => {
    if (chatBodyRef.current) {
      chatBodyRef.current.scrollTop = chatBodyRef.current.scrollHeight;
    }
  }, [threadDetail?.messages]);

  const handlePauseToggle = async () => {
    if (!activeThreadId || !threadDetail) return;
    const action = threadDetail.thread.is_paused ? 'resume' : 'pause';
    try {
      const res = await fetch(`/api/threads/${activeThreadId}/${action}`, { method: 'POST' });
      if (res.ok) {
        toast({ title: 'Success', description: `AI automated response ${action}d.` });
        fetchThreadDetail(activeThreadId);
        fetchThreads();
      }
    } catch (err) {
      console.error(err);
    }
  };

  const handleTakeoverToggle = async () => {
    if (!activeThreadId || !threadDetail) return;
    const isHuman = threadDetail.thread.control_mode.toLowerCase() === 'human';
    const action = isHuman ? 'release' : 'takeover';
    try {
      const res = await fetch(`/api/threads/${activeThreadId}/${action}`, { method: 'POST' });
      if (res.ok) {
        toast({ title: 'Success', description: isHuman ? 'Control released back to AI.' : 'Human operator takeover active.' });
        fetchThreadDetail(activeThreadId);
        fetchThreads();
      }
    } catch (err) {
      console.error(err);
    }
  };

  const handleSendMessage = async (e?: React.FormEvent) => {
    if (e) e.preventDefault();
    if (!activeThreadId || !messageText.trim()) return;

    try {
      const res = await fetch(`/api/threads/${activeThreadId}/message`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ body: messageText.trim() })
      });
      if (res.ok) {
        setMessageText('');
        fetchThreadDetail(activeThreadId);
      }
    } catch (err) {
      console.error(err);
    }
  };

  const handleApproveSuggestion = async () => {
    if (!activeThreadId) return;
    try {
      const res = await fetch(`/api/threads/${activeThreadId}/approve_suggestion`, { method: 'POST' });
      if (res.ok) {
        toast({ title: 'Approved', description: 'AI suggestion sent successfully.' });
        fetchThreadDetail(activeThreadId);
      }
    } catch (err) {
      console.error(err);
    }
  };

  return (
    <div className="h-[calc(100vh-140px)] flex border rounded-lg overflow-hidden bg-card text-foreground">
      {/* Threads Sidebar */}
      <div className="w-80 border-r flex flex-col bg-card/50">
        <div className="p-4 border-b">
          <h2 className="font-semibold text-lg">Conversations</h2>
        </div>
        <div className="flex-1 overflow-y-auto divide-y">
          {isLoading ? (
            <div className="p-4 text-center text-muted-foreground text-sm">Loading chats...</div>
          ) : threads.length === 0 ? (
            <div className="p-4 text-center text-muted-foreground text-sm">No active conversations</div>
          ) : (
            threads.map((t) => (
              <div
                key={t.id}
                onClick={() => setActiveThreadId(t.id)}
                className={`p-4 cursor-pointer hover:bg-accent/40 transition-colors ${
                  activeThreadId === t.id ? 'bg-accent/60' : ''
                }`}
              >
                <div className="flex justify-between items-start mb-1">
                  <span className="font-medium text-sm">Thread {t.id.slice(0, 8)}...</span>
                  <span className="text-[10px] text-muted-foreground">
                    {new Date(t.updated_at * 1000).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}
                  </span>
                </div>
                <div className="text-xs text-muted-foreground mb-2">Status: {t.status}</div>
                <div className="flex flex-wrap gap-1">
                  <span className={`text-[10px] px-1.5 py-0.5 rounded font-medium uppercase ${
                    t.control_mode.toLowerCase() === 'human' ? 'bg-red-500/10 text-red-500' : 'bg-green-500/10 text-green-500'
                  }`}>
                    {t.control_mode}
                  </span>
                  {(t.is_paused === true || t.is_paused === 1) && (
                    <span className="text-[10px] bg-yellow-500/10 text-yellow-500 px-1.5 py-0.5 rounded font-medium">
                      PAUSED
                    </span>
                  )}
                  <span className="text-[10px] bg-secondary text-secondary-foreground px-1.5 py-0.5 rounded font-medium">
                    {t.opportunity_status}
                  </span>
                </div>
              </div>
            ))
          )}
        </div>
      </div>

      {/* Chat Area */}
      <div className="flex-1 flex flex-col bg-background/30">
        {threadDetail ? (
          <>
            {/* Header */}
            <div className="p-4 border-b flex justify-between items-center bg-card">
              <div>
                <h3 className="font-bold flex items-center gap-2">
                  <User className="h-4 w-4 text-muted-foreground" />
                  {threadDetail.contact.name || `User (${threadDetail.contact.id.slice(0, 8)})`}
                </h3>
                <span className="text-xs text-muted-foreground flex items-center gap-1 mt-0.5">
                  <Phone className="h-3 w-3" /> {threadDetail.contact.phone || 'No phone info'}
                </span>
              </div>
              <div className="flex gap-2">
                <Button
                  size="sm"
                  variant={threadDetail.thread.is_paused ? 'default' : 'outline'}
                  onClick={handlePauseToggle}
                  className="flex items-center gap-1.5"
                >
                  {threadDetail.thread.is_paused ? (
                    <><Play className="h-3.5 w-3.5" /> Resume AI</>
                  ) : (
                    <><Pause className="h-3.5 w-3.5" /> Pause AI</>
                  )}
                </Button>
                <Button
                  size="sm"
                  variant={threadDetail.thread.control_mode.toLowerCase() === 'human' ? 'destructive' : 'outline'}
                  onClick={handleTakeoverToggle}
                  className="flex items-center gap-1.5"
                >
                  <Bot className="h-3.5 w-3.5" />
                  {threadDetail.thread.control_mode.toLowerCase() === 'human' ? 'Release (AI)' : 'Takeover (Human)'}
                </Button>
              </div>
            </div>

            {/* Chat Body */}
            <div ref={chatBodyRef} className="flex-1 p-6 overflow-y-auto space-y-4 bg-accent/5">
              {threadDetail.messages.map((msg) => {
                const isOperator = msg.sender.toLowerCase() === 'operator';
                const isSystem = msg.sender.toLowerCase() === 'system';
                return (
                  <div
                    key={msg.id}
                    className={`flex ${isOperator ? 'justify-end' : isSystem ? 'justify-center' : 'justify-start'}`}
                  >
                    {isSystem ? (
                      <div className="text-[11px] bg-secondary text-secondary-foreground px-2.5 py-1 rounded-full border">
                        {msg.content}
                      </div>
                    ) : (
                      <div className={`max-w-[70%] rounded-2xl px-4 py-2 text-sm shadow-sm ${
                        isOperator
                          ? 'bg-primary text-primary-foreground rounded-tr-none'
                          : 'bg-card border text-foreground rounded-tl-none'
                      }`}>
                        {msg.content}
                      </div>
                    )}
                  </div>
                );
              })}
            </div>

            {/* Suggestions Panel */}
            {threadDetail.suggestion && (
              <div className="p-4 border-t bg-yellow-500/5 border-yellow-500/10 flex flex-col gap-2">
                <div className="flex justify-between items-center text-xs">
                  <span className="font-semibold text-yellow-600 dark:text-yellow-400 flex items-center gap-1">
                    <ShieldAlert className="h-3.5 w-3.5" /> AI Suggested Reply
                  </span>
                  <span className="text-muted-foreground">Confidence: {Math.round(threadDetail.suggestion.confidence * 100)}%</span>
                </div>
                <p className="text-sm border border-yellow-500/20 bg-yellow-500/5 p-3 rounded-md italic">
                  "{threadDetail.suggestion.suggested_reply}"
                </p>
                <div className="flex justify-end gap-2">
                  <Button size="sm" variant="outline" onClick={() => setMessageText(threadDetail.suggestion!.suggested_reply)}>
                    Copy to Input
                  </Button>
                  <Button size="sm" className="bg-yellow-600 hover:bg-yellow-700 text-white flex items-center gap-1" onClick={handleApproveSuggestion}>
                    <Check className="h-3.5 w-3.5" /> Approve & Send
                  </Button>
                </div>
              </div>
            )}

            {/* Footer Form */}
            <form onSubmit={handleSendMessage} className="p-4 border-t bg-card flex gap-2">
              <input
                type="text"
                placeholder={
                  threadDetail.thread.control_mode.toLowerCase() === 'human'
                    ? "Type a message to reply..."
                    : "Takeover control to message directly..."
                }
                disabled={threadDetail.thread.control_mode.toLowerCase() !== 'human'}
                value={messageText}
                onChange={(e) => setMessageText(e.target.value)}
                className="flex-1 p-2 border rounded-md bg-background text-sm focus:outline-none focus:ring-1 focus:ring-primary disabled:opacity-50"
              />
              <Button type="submit" size="icon" disabled={threadDetail.thread.control_mode.toLowerCase() !== 'human' || !messageText.trim()}>
                <Send className="h-4 w-4" />
              </Button>
            </form>
          </>
        ) : (
          <div className="flex-1 flex items-center justify-center text-muted-foreground text-sm">
            Select a conversation thread to view chat history and take action.
          </div>
        )}
      </div>
    </div>
  );
}
