# ARCHITECTURE REVIEW v1.2

## 1. Applied Corrections

1. **Event Sourcing Law**: Updated `01_Architecture.mmd` and `03_Runtime.mmd` to strictly enforce that commands produce events, events enter the Event Store, and projection workers derive State from the Event Store. State is never mutated directly.
2. **Command vs Action Separation**: Updated `02_DataModel.mmd` and `03_Runtime.mmd`. Implemented explicit pipeline: Trigger → Command (intent) → Event → Action Creation (work item) → Execution (physical attempt) → Capability.
3. **Availability Engine Completion**: Wired Availability in `01_Architecture.mmd`, `02_DataModel.mmd`, and `03_Runtime.mmd`. Schedulers now query Actor and Asset Availability prior to creating Scheduled Events (Participants, Reserved Resources).
4. **Evidence Invalidation Loop**: Added `EvidenceInvalidated Event` to `02_DataModel.mmd`, and integrated constraint re-evaluation flows causing a "Suspended" or "Non-Compliant" case state into `03_Runtime.mmd` and `07_BusinessFlow.mmd`. Ensured history is never rewound.
5. **Budget Ownership Model**: Added relationships Organization → Budget, Package → Budget Policy, and Case → Budget Consumption in `02_DataModel.mmd`. Verified that Execution Engine checks budget beforehand (`01_Architecture.mmd`, `03_Runtime.mmd`).
6. **Incident Feedback Loop**: Integrated Incident outputs as operational controls in `01_Architecture.mmd`, `02_DataModel.mmd`, and `03_Runtime.mmd`. Incidents now map to the Operational Supervisor, Command Bus, Approval Engine, and handle pausing/resuming executions.
7. **Package Version Pinning**: Added `PackageVersion` tying directly to Case (`02_DataModel.mmd`), ensuring cases never migrate versions. New cases automatically pull the latest approved version (`04_Package.mmd`).
8. **Snapshot Architecture**: Brought Snapshot models into the runtime layer. Event Store → Snapshot Builder → Snapshot Store, where Case Load reads the latest Snapshot and replays only remaining events (`01_Architecture.mmd`, `03_Runtime.mmd`).

## 2. Updated Primitive Inventory

- **Structural**: Organization, Actor, OrganizationActorRole, Asset.
- **Operational**: Case, StateMachine, State, Transition, Constraint, Evidence, Waiver, PackageVersion.
- **Execution**: Trigger, Command, Action, Capability, Assignment, Approval, Execution, IdempotencyKey.
- **Scheduling**: Availability, ScheduledEvent, Participant, ReservedResource.
- **Governance**: Budget, BudgetPolicy, BudgetConsumption, CircuitBreaker, Incident.
- **Persistence**: Event, EvidenceInvalidatedEvent, Snapshot, Projection.
- **Package**: Package, PackageVersion, PackageDependency.

## 3. Cross-Diagram Consistency Report

- Every primitive listed above is correctly typed in the Data Model (`02_DataModel.mmd`).
- Interactions between primitives map 1:1 with flows in Runtime and Architecture (`01_Architecture.mmd`, `03_Runtime.mmd`).
- No orphan concepts exist; all entities (e.g., Snapshots, Incidents, Budgets) flow structurally, behaviorally, and strictly through the correct pipeline.
- Packages clearly orchestrate policies and do not touch runtime logic, confirmed via explicit boundaries in `04_Package.mmd`.

## 4. Remaining Risks

- **Idempotency Edge Cases**: Handled via `IdempotencyKey` tracking, but concurrent event replay edge cases might require tight snapshot-version bounding.
- **Incident Escalation Delays**: Asynchronous pausing of Execution Queue might race against already-running tasks at external providers.
- **Complex Budgeting**: Cross-case budget checking could encounter high read-latency at the projection layer if rate limits are extremely tight.

## 5. Freeze Recommendation

The specification has been comprehensively corrected to ensure the Event Sourcing Law, clear Action bounds, properly wired Scheduling & Budgets, operational Incident loops, and optimal Snapshot replay. It is completely implementation-ready.

**ARCHITECTURE FREEZE v1.2 APPROVED**
