# Cloakd 🛡️

[![Version](https://img.shields.io/badge/version-1.2.0-blue.svg)](Cargo.toml)
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

## 🏢 Declarative Multi-Tenancy & Model Governance (v1.2.0)

Cloakd Core features an enterprise-grade declarative manifest system inspired by Kubernetes (`kind: Tenant`, `kind: Role`, `kind: User`) with **cascading configuration inheritance**:

$$\mathbf{User} \longrightarrow \mathbf{Role} \longrightarrow \mathbf{Tenant} \longrightarrow \mathbf{Global\ (.env)}$$

* 📁 **Multi-Document & Directory Scanning** : Accepts single multi-document YAML (`---`) or recursively scans entire directories (`CLOAKD_TENANTS_FILE=config/tenants/`).
* 🎯 **Model Governance (`default_model` & `allowed_models`)** : Restrict expensive frontier models (e.g. `o1`, `gpt-4o`) to specific roles or users using wildcards (`gemini-*`, `gpt-4o-mini`). Unauthorized requests return `HTTP 403 Forbidden`.
* 🔄 **Precise Failover with `fallback_models`** : Cascades through explicit alternative models on HTTP 429/5xx (e.g. `gemini-3.5-flash-lite` $\rightarrow$ `gpt-4o-mini`) rather than guessing providers.
* 💼 **Per-Tenant BYOK** : Ingress tenant keys (`sk-cloakd-...`) are stripped while injecting each organization's private LLM keys.
* 🛡️ **Defense-in-Depth DLP** : Effective rules are the additive union: `Tenant.rules ∪ Role.rules ∪ User.rules`.
* 🏷️ **End-to-End Audit Trail** : Every response includes:
  ```http
  x-cloakd-tenant-id: bank-corp
  x-cloakd-user-id: usr_alice
  x-cloakd-role: developer
  ```

```yaml
# Example: Declarative Manifests (config/tenants.yaml)
kind: Tenant
id: "bank-corp"
name: "Bank Corp Global"
default_model: "gemini-3.5-flash-lite"
allowed_models: ["*"]
fallback_models: ["gpt-4o-mini", "claude-3-5-haiku"]
enabled_rules: ["banking", "fr", "secrets"]
provider_keys:
  openai: "sk-proj-bankcorp-private-key"
  gemini: "AIzaSyBankCorpKey"
rate_limit_rpm: 2000
---
kind: Role
id: "developer"
tenant_id: "bank-corp"
name: "Software Engineer"
allowed_models: ["gemini-3.5-flash-lite", "gpt-4o-mini"]
fallback_models: ["gpt-4o-mini"]
rate_limit_rpm: 60
---
kind: User
id: "usr_alice"
tenant_id: "bank-corp"
role: "developer"
name: "Alice Martin"
key: "sk-cloakd-bank-alice-7788"
```

---

## 📊 Prometheus Native Metrics (`/metrics`)

Cloakd Core exposes zero-dependency, atomic thread-safe Prometheus metrics at `GET /metrics`:

| Metric Name | Type | Description & Labels |
| :--- | :--- | :--- |
| `cloakd_http_requests_total` | Counter | Total HTTP requests handled (`tenant`, `role`, `status`, `cache="HIT\|MISS\|BYPASS"`) |
| `cloakd_dlp_masked_entities_total` | Counter | Sensitive PII entities detected and pseudonymized (`tenant`, `entity`) |
| `cloakd_cache_hits_total` | Counter | FinOps cache hits saving upstream LLM token costs (`tenant`) |
| `cloakd_cache_misses_total` | Counter | Cache misses forwarded to upstream LLMs (`tenant`) |
| `cloakd_upstream_failover_total` | Counter | Failover cascade triggers on 429/5xx errors (`from`, `to`) |

---

## 🗺️ Product Roadmap

> *This repository strictly covers the **Data Plane Gateway**. For the SaaS Control Plane web application, organization management, and billing, please refer to the [**Cloakd Platform Roadmap**](https://github.com/cloakd-gateway/platform#-saas-product-roadmap).*

* **v1.0.0 — Production Core :**
  - [x] High-performance stateless Axum HTTP Gateway (`/v1/chat/completions`).
  - [x] In-memory reversible pseudonymization (`SessionVault`) with zero persistence.
  - [x] Sliding-window SSE stream transformer resolving fragmented token boundaries.
  - [x] International DLP rule catalogue (IBAN Modulo 97, CRA Secrets, US SSN, UK NINO, ES DNI, FR NIR).
  - [x] Granular rule selection with zero runtime overhead (`CLOAKD_ENABLED_RULES`).
  - [x] FinOps in-memory prompt cache with cross-user privacy isolation.
  - [x] Multi-provider fallback cascade on HTTP 429/5xx errors.
  - [x] Real-time audit headers (`x-cloakd-cache`, `x-cloakd-fallback`, `x-cloakd-masked-count`).

* **v1.1.0 — High-Performance Multi-Tenant Data Plane :**
  - [x] Multi-tenant isolation and per-tenant BYOK credentials.
  - [x] In-memory sub-50 µs LRU `TenantCache` (powered by `moka`).
  - [x] Native Prometheus metrics exporter (`GET /metrics`).
  - [x] Multi-tenant audit trail with `x-cloakd-tenant-id`.

* **v1.2.0 — Declarative Manifests & Model Governance (Current Version) :**
  - [x] Kubernetes-style declarative manifests (`kind: Tenant`, `kind: Role`, `kind: User`).
  - [x] Cascading configuration inheritance (`User > Role > Tenant > Global`).
  - [x] Fine-grained Model Governance with wildcard allowlists (`allowed_models`) and `default_model`.
  - [x] Model-based failover cascades (`fallback_models`) replacing provider guessing.
  - [x] Recursive directory scanning (`conf.d/`) and multi-document YAML parsing.
  - [x] Enriched caller audit telemetry (`x-cloakd-user-id`, `x-cloakd-role`) and Prometheus `role` metric label.

* **v1.3.0 — Dynamic Control Plane Synchronization & Distributed Caching (Upcoming) :**
  - [ ] Webhook-driven tenant cache invalidation and hot-reloading.
  - [ ] Real-time Control Plane streaming sync (gRPC / WebSocket).
  - [ ] Redis / Dragonfly optional distributed cache backend for clustered horizontal scaling.
  - [ ] Semantic prompt caching using local SIMD embedding models.

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
