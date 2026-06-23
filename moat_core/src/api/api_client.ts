/**
 * Moat Control Plane - Typed API Client
 * Generated for every dashboard view screen.
 */

// --- Types ---
export type UserRole = "Owner" | "Operator";

export interface LoginResponse {
    username: string;
    role: UserRole;
}

export interface Thread {
    id: string;
    contact_id: string;
    status: string;
    opportunity_status: string;
    created_at: number;
    updated_at: number;
    control_mode: "Ai" | "Human" | "Hybrid";
    assigned_operator: string | null;
    is_paused: boolean;
}

export interface Contact {
    id: string;
    name: string;
    email: string;
    phone: string;
}

export interface Message {
    id: string;
    thread_id: string;
    sender: "Business" | "Contact" | "System";
    content: string;
    timestamp: number;
}

export interface Suggestion {
    suggested_reply: string;
    confidence: number;
}

export interface ThreadDetailResponse {
    thread: Thread;
    contact: Contact;
    messages: Message[];
    suggestion?: Suggestion;
}

export interface Authorization {
    id: string;
    thread_id: string;
    engagement_id: string;
    proposed_by: string;
    created_at: number;
}

export interface Opportunity {
    id: string;
    contact_id: string;
    thread_id: string;
    status: string;
    value: number;
    notes: string;
    pipeline_stage: string;
}

export interface Booking {
    id: string;
    contact_id: string;
    thread_id: string;
    title: string;
    status: string;
    start_time: number;
    end_time: number;
}

export interface Escalation {
    id: string;
    thread_id: string;
    reason: string;
    status: string;
    created_at: number;
}

export interface AnalyticsOverview {
    active_threads: number;
    pending_authorizations: number;
    pending_escalations: number;
    bookings_today: number;
    conversion_rate: number;
    human_takeover_rate: number;
}

export interface GoogleConfig {
    selected_calendar_id?: string;
    selected_spreadsheet_id?: string;
    selected_worksheet_id?: string;
}

// --- API Client ---

export class ApiClient {
    private async request<T>(endpoint: string, options: RequestInit = {}): Promise<T> {
        const response = await fetch(endpoint, {
            ...options,
            headers: {
                'Content-Type': 'application/json',
                ...options.headers,
            },
        });
        
        if (!response.ok) {
            throw new Error(`API Error: ${response.status} on ${endpoint}`);
        }
        
        // Handle empty responses
        const text = await response.text();
        return text ? JSON.parse(text) : {} as T;
    }

    // Auth
    async login(username: string, password: string):Promise<LoginResponse> {
        return this.request<LoginResponse>('/login', {
            method: 'POST',
            body: JSON.stringify({ username, password })
        });
    }

    async logout():Promise<void> {
        return this.request<void>('/logout', { method: 'POST' });
    }

    // Threads (Inbox)
    async listThreads(): Promise<Thread[]> {
        return this.request<Thread[]>('/threads');
    }

    async getThread(id: string): Promise<ThreadDetailResponse> {
        return this.request<ThreadDetailResponse>(`/threads/${id}`);
    }

    async pauseThread(id: string): Promise<void> {
        return this.request<void>(`/threads/${id}/pause`, { method: 'POST' });
    }

    async resumeThread(id: string): Promise<void> {
        return this.request<void>(`/threads/${id}/resume`, { method: 'POST' });
    }

    async takeoverThread(id: string): Promise<void> {
        return this.request<void>(`/threads/${id}/takeover`, { method: 'POST' });
    }

    async releaseThread(id: string): Promise<void> {
        return this.request<void>(`/threads/${id}/release`, { method: 'POST' });
    }

    async sendMessage(id: string, body: string): Promise<void> {
        return this.request<void>(`/threads/${id}/message`, {
            method: 'POST',
            body: JSON.stringify({ body })
        });
    }

    async approveSuggestion(id: string): Promise<void> {
        return this.request<void>(`/threads/${id}/approve_suggestion`, { method: 'POST' });
    }

    // Authorizations
    async listAuthorizations(): Promise<Authorization[]> {
        return this.request<Authorization[]>('/authorizations');
    }

    async approveAuthorization(id: string, reason: string): Promise<void> {
        return this.request<void>(`/authorizations/${id}/approve`, {
            method: 'POST',
            body: JSON.stringify({ reason })
        });
    }

    async rejectAuthorization(id: string, reason: string): Promise<void> {
        return this.request<void>(`/authorizations/${id}/reject`, {
            method: 'POST',
            body: JSON.stringify({ reason })
        });
    }

    // Opportunities (Leads)
    async listOpportunities(): Promise<Opportunity[]> {
        return this.request<Opportunity[]>('/opportunities');
    }

    async getOpportunity(id: string): Promise<Opportunity> {
        return this.request<Opportunity>(`/opportunities/${id}`);
    }

    async updateOpportunityStage(id: string, stage: string, reason?: string): Promise<void> {
        return this.request<void>(`/opportunities/${id}/stage`, {
            method: 'POST',
            body: JSON.stringify({ stage, reason })
        });
    }

    // Bookings (Appointments)
    async listBookings(): Promise<Booking[]> {
        return this.request<Booking[]>('/bookings');
    }

    async getBooking(id: string): Promise<Booking> {
        return this.request<Booking>(`/bookings/${id}`);
    }

    // Escalations
    async listEscalations(): Promise<Escalation[]> {
        return this.request<Escalation[]>('/escalations');
    }

    async resolveEscalation(id: string, reason: string): Promise<void> {
        return this.request<void>(`/escalations/${id}/resolve`, {
            method: 'POST',
            body: JSON.stringify({ reason })
        });
    }

    // Analytics
    async getAnalyticsOverview(): Promise<AnalyticsOverview> {
        return this.request<AnalyticsOverview>('/analytics');
    }

    // WhatsApp Connection
    async getWhatsAppStatus(): Promise<{status: string}> {
        return this.request<{status: string}>('/connectors/whatsapp/status');
    }

    // Google Workspace
    async connectGoogle(): Promise<{url: string}> {
        return this.request<{url: string}>('/connections/google/connect', { method: 'POST' });
    }

    async disconnectGoogle(): Promise<void> {
        return this.request<void>('/connections/google/disconnect', { method: 'POST' });
    }

    async getGoogleStatus(): Promise<any> {
        return this.request<any>('/connections/google/status');
    }

    async getGoogleResources(): Promise<any> {
        return this.request<any>('/connections/google/resources');
    }

    async getGoogleWorksheets(spreadsheetId: string): Promise<any> {
        return this.request<any>(`/connections/google/worksheets/${spreadsheetId}`);
    }

    async saveGoogleConfig(config: GoogleConfig): Promise<void> {
        return this.request<void>('/connections/google/config', {
            method: 'POST',
            body: JSON.stringify(config)
        });
    }
}
