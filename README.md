# Cloakd 🛡️

[![Version](https://img.shields.io/badge/version-1.0.0-blue.svg)](Cargo.toml)
[![License](https://img.shields.io/badge/license-Apache%202.0%20%2F%20MIT-green.svg)](LICENSE)
[![Memory Footprint](https://img.shields.io/badge/memory-%3C10%20MB%20RAM-brightgreen.svg)]()
[![Binary Size](https://img.shields.io/badge/binary-5.2%20MB-purple.svg)]()
[![Rust](https://img.shields.io/badge/rust-2021%20edition-orange.svg)]()

> **The ultra-high-performance, zero-persistence privacy & FinOps gateway for LLMs.**  
> Sub-millisecond reversible pseudonymization, instant in-memory prompt caching, and multi-provider automatic failover packaged in a single standalone binary.

---

## ⚡ Why Cloakd?

Companies hesitate to unleash distant LLM APIs (Google Gemini, OpenAI, Anthropic) due to fears of leaking **PII, trade secrets, and infrastructure credentials** (RGPD, HIPAA, EU Cyber Resilience Act / NIS 2). 

Existing solutions have critical flaws:
* **Destructive masking (`[REDACTED]`)** ruins reasoning, breaks syntactical context, and degrades LLM response quality.
* **Heavy Python / Node.js proxies** consume 200–500 MB of RAM, add unacceptable latency, and choke on high-throughput server-sent event (SSE) streams.

### The Cloakd Breakthrough
* 🔒 **Reversible Local Pseudonymization (*Vaulted Tokenization*)** : Sensitive data is replaced by contextual typed tokens on ingress (`{{__VAR_EMAIL_1__}}`, `{{__VAR_IBAN_1__}}`) and seamlessly restored on egress—**even across fragmented network chunks in real-time SSE streaming**.
* ⚡ **Ultra-Low Resource Footprint** : Consumes **~6.5 MB of RAM** under production load and adds **sub-millisecond (< 1 ms) overhead**. Single 5.2 MB static Rust binary with pure Rustls (zero external C dependencies).
* 💰 **FinOps Prompt Caching** : Identical prompt structures across different users trigger instant cache hits (**< 10 ms latency, $0 token cost**) while guaranteeing **total privacy isolation** (each user only sees their own sensitive data).
* 🛡️ **Zero-Persistence Guarantee** : The session vault is strictly in-memory and dropped immediately upon request termination. Zero disk writes, zero PII logging.
* 🔄 **Transparent Multi-Provider Failover** : Automatic fallback on `HTTP 429` (Rate limits) or `5xx` server outages (e.g. Gemini $\rightarrow$ OpenAI $\rightarrow$ Anthropic $\rightarrow$ Local Ollama/vLLM).
* 🔌 **100% Drop-In OpenAI Compatibility** : Works instantly with official OpenAI Python/Node SDKs, LangChain, LlamaIndex, Cursor, and any OpenAI-compatible client.

---

## 🏗️ Architecture & Data Flow

```
                      Client Request (e.g. via OpenAI SDK)
                                      │
                                      ▼
                        ┌───────────────────────────┐
                        │   Cloakd Reverse Proxy    │
                        └─────────────┬─────────────┘
                                      │
                                      ▼
                        ┌───────────────────────────┐
                        │    Ingress DLP Masking    │ ──> Ephemeral SessionVault
                        └─────────────┬─────────────┘     (Zero persistence, dropped at end)
                                      │ (Pseudonymized Payload)
                                      ▼
                      ┌───────────────────────────────┐
                      │    FinOps Cache (SHA-256)     │
                      └───────┬───────────────┬───────┘
                     HIT (6ms)│               │ MISS
                              │               ▼
                              │   ┌───────────────────────┐
                              │   │  Upstream Dispatcher  │
                              │   │   & Failover Chain    │
                              │   └───────────┬───────────┘
                              │               │ (Error 429 or 5xx ?)
                              │               ├──> Fallback 1 (OpenAI)
                              │               └──> Fallback 2 (Anthropic)
                              │               │
                              │               ▼
                              │        Store in Cache
                              │               │
                              ▼               ▼
                        ┌───────────────────────────┐
                        │   Egress De-Tokenization  │ ──> Restores real user data
                        └─────────────┬─────────────┘     (SSE sliding window FSM)
                                      │
                                      ▼
                         Client Response (Transparent)
                         x-cloakd-cache: HIT / MISS
                         x-cloakd-fallback: true / false
```

---

## 🚀 Quickstart (Under 60 Seconds)

### 1. Build and Run from Source
```bash
# Clone and build optimized binary
git clone https://github.com/cloakd-gateway/core.git
cd core
cargo build --release

# Configure your keys
cp .env.example .env
# Edit .env and set your GEMINI_API_KEY or OPENAI_API_KEY

# Start Cloakd gateway
./target/release/cloakd
```

### 2. Docker Deployment
```bash
docker run -d \
  --name cloakd \
  -p 8080:8080 \
  -e GEMINI_API_KEY="your-gemini-api-key" \
  -e CLOAKD_ENABLED_RULES="default" \
  cloakd:latest
```

### 3. Test with cURL
```bash
curl -X POST http://127.0.0.1:8080/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "messages": [
      {
        "role": "user",
        "content": "Confirm my IBAN DE89 3704 0044 0532 0130 00 and email ceo@company.com."
      }
    ]
  }'
```
**Headers returned:**
```http
HTTP/1.1 200 OK
x-cloakd-masked-count: 2
x-cloakd-provider: gemini
x-cloakd-model: gemini-3.5-flash-lite
x-cloakd-cache: MISS
```
*(The LLM only saw `{{__VAR_IBAN_1__}}` and `{{__VAR_EMAIL_1__}}`. The response you receive contains the restored real values).*

---

## 💻 Client Integration (Drop-in Replacement)

### Python (Official OpenAI SDK)
Just change `base_url`. **Zero other code changes required:**
```python
from openai import OpenAI

client = OpenAI(
    base_url="http://localhost:8080/v1",
    api_key="not-needed", # Cloakd manages upstream credentials
)

response = client.chat.completions.create(
    model="gemini-3.5-flash-lite", # Or gpt-4o-mini, claude-3-5-haiku
    messages=[
        {"role": "user", "content": "Please verify user account for john@corp.com."}
    ],
    stream=True # Streaming SSE works out-of-the-box!
)

for chunk in response:
    if chunk.choices[0].delta.content:
        print(chunk.choices[0].delta.content, end="", flush=True)
```

### TypeScript / JavaScript
```typescript
import OpenAI from "openai";

const client = new OpenAI({
  baseURL: "http://localhost:8080/v1",
  apiKey: "not-needed",
});

const completion = await client.chat.completions.create({
  model: "gemini-3.5-flash-lite",
  messages: [{ role: "user", content: "Check card 4532-0150-1808-1114." }],
});

console.log(completion.choices[0].message.content);
```

---

## 📋 Comprehensive DLP Rules Catalogue

Cloakd filters sensitive entities using zero-allocation compiled regexes validated by mathematical checksum algorithms:

| Rule | Scope | Algorithm & Standards |
| :--- | :--- | :--- |
| **`iban`** | Global (75+ countries) | ISO 13616 + **ISO 7064 Modulo 97-10** (Zero false-positive guarantee) |
| **`secrets`** | CRA & NIS 2 | AWS (`AKIA...`), GitHub (`ghp_...`, fine-grained PAT), Google (`AIzaSy...`), OpenAI/Anthropic (`sk-...`), JWT (`eyJ...`), PEM Private Keys, Database Connection Strings |
| **`card`** | Global | Visa, MasterCard, Amex, Discover with **Luhn Checksum** validation |
| **`phone`** | Global | International **E.164** (`+1`, `+33`, `+44`, `+49`...) & national formats |
| **`email`** | Global | RFC 5322 standard email address format |
| **`ip`** | Infrastructure | RFC 1918 Private IPv4 (`10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`) & loopbacks |
| **`fr_nir`** | France | French Social Security Number (13/15 digits with **Modulo 97** key verification) |
| **`us_ssn`** | United States | Social Security Number with strict SSA restrictions (no 000, 666, 900+ areas) |
| **`uk_nino`** | United Kingdom | National Insurance Number with HMRC prefix and suffix letter validation |
| **`es_dni`** | Spain | DNI & NIE with official **Modulo 23** check algorithm |

### Granular Rule Selection (`CLOAKD_ENABLED_RULES`)
Activate only the rules or regional packs you need:
```env
# Built-in packs: default, all, banking, secrets, pii, eu, fr, us, uk, es
CLOAKD_ENABLED_RULES=default,us,uk

# Or a comma-separated list of individual rules
CLOAKD_ENABLED_RULES=email,card,iban,secrets,us_ssn
```
*Inactive rules are never instantiated and have **zero CPU/memory overhead**.*

---

## 🗺️ Product Roadmap & Architecture Strategy

Cloakd's architecture is intentionally split into two distinct repositories to guarantee zero compromise on latency, resource footprint, and security isolation:
* **`cloakd-gateway/core`** (This Repository) — The ultra-fast, zero-persistence Rust Data Plane Gateway.
* **`cloakd-gateway/platform`** (Separate Repository) — The SaaS Control Plane web application, organization management, and billing system.

### 🛡️ Repository: `cloakd-gateway/core` (Data Plane Gateway)

* **v1.0.0 — Production Core (Current Version) :**
  - [x] High-performance stateless Axum HTTP Gateway (`/v1/chat/completions`).
  - [x] In-memory reversible pseudonymization (`SessionVault`) with zero persistence.
  - [x] Sliding-window SSE stream transformer resolving fragmented token boundaries.
  - [x] International DLP rule catalogue (IBAN Modulo 97, CRA Secrets, US SSN, UK NINO, ES DNI, FR NIR).
  - [x] Granular rule selection with zero runtime overhead (`CLOAKD_ENABLED_RULES`).
  - [x] FinOps in-memory prompt cache with cross-user privacy isolation.
  - [x] Multi-provider fallback cascade on HTTP 429/5xx errors.
  - [x] Real-time audit headers (`x-cloakd-cache`, `x-cloakd-fallback`, `x-cloakd-masked-count`).

* **v1.1.0 — High-Performance Multi-Tenant Data Plane (Upcoming) :**
  - [ ] `TenantContext` resolution via API keys (`Bearer sk-cloakd-tenant...`).
  - [ ] Dynamic organization DLP rules & custom fallback routing per tenant.
  - [ ] Ultra-fast local LRU config cache (sub-50 µs) with asynchronous sync from the Control Plane.
  - [ ] Prometheus & OpenTelemetry native metrics exporter (`/metrics`).
  - [ ] Standalone air-gapped / Local YAML configuration loader for enterprise on-premise deployments.

---

### 🌐 Repository: `cloakd-gateway/platform` (SaaS Control Plane)

* **v1.0.0 — SaaS MVP (Control Plane Foundation) :**
  - [ ] Modern Web Management Console (Next.js, Tailwind, Shadcn UI).
  - [ ] Multi-Tenant & Multi-Organization architecture with RBAC (Owner, Admin, Member).
  - [ ] Bring-Your-Own-Key (BYOK) provider credential storage with AES-256-GCM envelope encryption.
  - [ ] API Key issuance & token lifecycle management (`sk-cloakd-tenant...`).
  - [ ] Visual DLP rule configuration & fallback chain reordering per organization.
  - [ ] Stripe integration for usage-based billing, subscriptions, and quota enforcement.

* **v2.0.0 — Enterprise SaaS :**
  - [ ] Enterprise SSO / SAML 2.0 / OIDC authentication (Okta, Microsoft Entra ID, Google Workspace).
  - [ ] FinOps & Privacy Analytics Dashboard (tokens saved, cache hit ratios, PII masked count).
  - [ ] Team audit logs and compliance reporting (RGPD / CRA readiness export).
  - [ ] Webhook alerts for quota limits or abnormal DLP spikes.

---

## 📖 In-Depth Documentation

Detailed architectural and operational documentation is available in the [`documentation/`](documentation/) directory:

* 🏛️ [**Architecture & Internals**](documentation/architecture.md) — Detailed pipeline lifecycle, SessionVault mechanics, and SSE sliding-window state machine.
* ⚙️ [**Configuration Reference**](documentation/configuration.md) — Comprehensive guide to all environment variables and options.
* 🛡️ [**DLP Rules & Checksums**](documentation/dlp-rules.md) — Mathematical algorithms, country packs, and CRA compliance guidelines.
* 🚀 [**Deployment Guide**](documentation/deployment.md) — Docker, Kubernetes sidecar, and Bare-Metal systemd setups.
* 🔌 [**Integration Guide**](documentation/integration-guide.md) — Examples for Python, TypeScript, LangChain, and LlamaIndex.

---

## 📄 License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.
