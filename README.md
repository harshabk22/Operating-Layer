# MoatAgeex: Operating Layer

A production-grade, package-driven backend architecture built in Rust. This system acts as a central "Operating Layer" or control plane for business operations, utilizing an event-sourced architecture and dynamic package management for extensible business logic (Sales, Real Estate, Notifications).

## 🏗 Architecture Highlights

* **Package-Driven Architecture**: Business domains (Sales, Real Estate, Notifications) are isolated into independent Rust crates that can be dynamically installed/uninstalled at runtime.
* **Event Sourcing**: All state mutations are handled via event dispatching and stored immutably in a `business_event_store` SQLite table. Read models (`projections`) are derived from these events.
* **Multi-Tenant Ready**: Data boundaries are enforced via `business_id`.
* **Integrated Control Plane**: Includes a built-in, lightweight vanilla HTML/JS/CSS dashboard (`dashboard.html`) served directly by the Rust backend.
* **RBAC Security**: Enforces Role-Based Access Control (Owner, Operator, Viewer) with secure HTTP-only session cookies.

## 🚀 Getting Started

### Prerequisites
* [Rust & Cargo](https://rustup.rs/) (latest stable)
* [Node.js & npm](https://nodejs.org/) (for running Playwright E2E tests)

### 1. Clone the repository
```bash
git clone https://github.com/harshabk22/Operating-Layer.git
cd Operating-Layer



## for me: 
2. Environment Setup
Create a .env file in the root directory. This file is ignored by Git for security.

bash


touch .env
(Add any required external API keys or configuration variables to this file, e.g., WhatsApp sandbox tokens or Google OAuth client IDs).

3. Run the Application
bash


cargo run
On the first run, the system will automatically:

Download and compile all Rust dependencies.
Initialize a fresh SQLite database (moat_core_restored.db).
Run necessary schema migrations.
The application will start on http://localhost:8080.

4. Access the Dashboard
Navigate to http://localhost:8080 in your browser.

Username: owner
Password: admin (Default fallback password for local development)
