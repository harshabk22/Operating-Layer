# Architecture Laws

**Law 1:**
Events create State. State never creates Events.

**Law 2:**
Command → Event → Event Store → Projection

**Law 3:**
All external work is asynchronous.

**Law 4:**
All executions require idempotency.

**Law 5:**
Packages are immutable.

**Law 6:**
Active cases never migrate versions.

**Law 7:**
Event Store is source of truth.

**Law 8:**
Projections are disposable.

**Law 9:**
Packages cannot define runtime primitives.

**Law 10:**
Every capability requires authorization.
