# Engagement Decision Flow

This document outlines the evaluation routing logic applied by the Business Policy Layer.

## Flow Schema Diagram

```
Inbound Message
│
▼
Is intent 'refund'? ───────────────► ESCALATE
│ No
▼
Is intent 'support'? ──────────────► ESCALATE
│ No
▼
Is intent 'booking' or 'schedule'? ─► REQUEST_AUTHORIZATION
│ No
▼
Is intent 'pricing'? ──────────────► ANSWER_ONLY
│ No
▼
Is rating 'critical'? ─────────────► ESCALATE
│ No
▼
Is rating 'hot' && objective is
qualify/schedule? ─────────────────► PROPOSE_ENGAGEMENT
│ No
▼
Is intent 'close'? ────────────────► CLOSE_THREAD
│ No
▼
Default ───────────────────────────► ANSWER_ONLY
```
