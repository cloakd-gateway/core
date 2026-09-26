# Cloakd — Architecture & Internals

This document provides an in-depth technical explanation of Cloakd's architecture, data flow, in-memory vault mechanics, streaming finite state machine (FSM), and caching pipeline.

---

## 1. Core Architectural Tenets

Cloakd was designed from the ground up to operate in high-security, low-resource environments (such as Kubernetes sidecar containers or embedded edge proxies) with four strict architectural principles:

1. **Sub-Millisecond Overhead ($O(1)$ / $O(N)$ Zero-Copy Processing) :**  
   Every pipeline component avoids unnecessary memory allocations and uses zero-copy string slices (`&str`, `bytes::Bytes`) wherever possible.
2. **Zero-Persistence (*Privacy by Design*) :**  
   No plaintext PII or vaulted tokens are ever written to disk, databases, or long-term storage. The encryption and tokenization context is scoped strictly to the lifecycle of the HTTP request.
3. **Strict Zero-Leak Tracing :**  
   The `Debug` trait on sensitive structures (`SessionVault`) is custom-implemented to emit only anonymized counters and metadata. Plaintext PII is mathematically barred from appearing in log sinks.
4. **Standard Compatibility :**  
   The API surface adheres 100% to the OpenAI Chat Completions specification (`/v1/chat/completions`), allowing transparent drop-in usage across all enterprise tooling.

---

## 2. Global Request Lifecycle

```
[Client Request]
       │
       ▼
1. Route & Parameter Resolution (ProviderRegistry)
       │
       ▼
2. Ingress DLP Inspection & Vault Tokenization (DlpEngine & SessionVault)
       │
       ▼ (Pseudonymized Payload)
3. FinOps Prompt Cache Lookup (SHA-256 Hash)
       ├── [CACHE HIT] ──────────────────────────────────────────┐
       │                                                         │
       ▼ [CACHE MISS]                                            │
4. Upstream Dispatcher & Resilience Failover                     │
       ├── Primary Target (e.g. Gemini)                          │
       └── [If 429/5xx] ──> Fallback Target (e.g. OpenAI)        │
       │                                                         │
       ▼ (Raw LLM Response)                                      │
5. Cache Storage (Fresh Anonymized Response)                     │
       │                                                         │
       ▼                                                         ▼
6. Egress Processing & De-Tokenization
       ├── Mode A (JSON Non-Streaming) : Direct Map Replacement
       └── Mode B (SSE Streaming)      : Sliding-Window FSM
       │
       ▼
7. Immediate Destruction (Drop of SessionVault)
       │
       ▼
[Client Response with Audit Headers]
```

---

## 3. The Reversible Pseudonymization Engine

### A. The Ephemeral `SessionVault` (`src/vault/session.rs`)
The vault maintains two in-memory hash maps:
* `plain_to_token: HashMap<String, String>`
* `token_to_plain: HashMap<String, String>`

When a sensitive string $S$ of type $T$ (e.g. `EMAIL`) is intercepted:
1. If $S$ already exists in `plain_to_token`, its existing token is returned (preserving intra-request coreference resolution).
2. If $S$ is new, a sequential token identifier is minted: `{{__VAR_<TYPE>_<ID>__}}` (e.g. `{{__VAR_EMAIL_1__}}`).
3. Both mappings are registered.

**Memory Safety & Destruction:**
The `SessionVault` is instantiated in the Axum handler and owned by the request's execution scope. When the HTTP request terminates (or the client disconnects), Rust's deterministic ownership model immediately invokes `Drop`, freeing all associated memory buffers.

### B. Ingress Traversal (`src/dlp/engine.rs`)
The DLP engine traverses the JSON request payload:
* Scans `messages[*].content` whether formatted as a plain string or as an array of multimodal content parts (`[{"type": "text", "text": "..."}]`).
* Runs the configured active `DlpRule` instances.
* Merges candidate spans and eliminates overlaps by selecting the longest non-overlapping match.
* Substitutes sensitive values with their typed vault tokens.

---

## 4. The FinOps Prompt Cache (`src/cache/`)

### A. Deterministic Hachage SHA-256 (`src/cache/key.rs`)
The cache key is calculated on the **pseudonymized prompt**:
$$\text{Key} = \text{SHA-256}(\text{Model} \mathbin{\Vert} \text{NormalizedMessages} \mathbin{\Vert} \text{Hyperparameters})$$

Parameters influencing generation determinism (`temperature`, `top_p`, `response_format`, `tools`) are canonicalized into the hash.

### B. Cross-User Deduplication with Complete Privacy Isolation
Because hashing occurs **after** Ingress DLP masking:
1. User A asks: `"Please generate a summary for client alice@alpha.com"`.
   * Masked prompt: `"Please generate a summary for client {{__VAR_EMAIL_1__}}"`.
   * Stored in cache: Anonymized response containing token `{{__VAR_EMAIL_1__}}`.
2. User B asks: `"Please generate a summary for client bob@beta.org"`.
   * Masked prompt: `"Please generate a summary for client {{__VAR_EMAIL_1__}}"`.
   * **Cache HIT :** The stored response is retrieved in < 100 microseconds.
   * **De-tokenization :** User B's vault replaces `{{__VAR_EMAIL_1__}}` with `bob@beta.org`.
   * **Zero Leakage :** Alice's email is never stored in User B's vault, guaranteeing strict cross-tenant isolation.

---

## 5. Streaming SSE Transformation FSM (`src/stream/sse.rs`)

In streaming mode (`stream: true`), LLMs emit Server-Sent Events (SSE) in arbitrary TCP chunks. A single vaulted token like `{{__VAR_EMAIL_1__}}` may arrive fragmented across chunk boundaries:
* Chunk 1: `data: {"choices":[{"delta":{"content":"Hello {{__VAR_"}}]}\n\n`
* Chunk 2: `data: {"choices":[{"delta":{"content":"EMAIL_1__}} how are you?"}}]}\n\n`

A naive token replacement would fail to detect the token in Chunk 1 and leak the half-token or corrupt the stream.

### The Sliding-Window FSM Solution
Cloakd implements `TokenStreamFsm`:
1. Maintains an internal sliding buffer (`window`).
2. When the buffer ends with a prefix of the token delimiter (e.g. `{{` or `{{__VAR_`), the FSM holds back only those candidate bytes and flushes preceding confirmed plaintext immediately.
3. Upon receiving the subsequent chunk, the buffer completes the token, performs vault lookup, and emits the substituted plaintext.
4. On stream termination, any remaining pending bytes are flushed cleanly.

This ensures **zero noticeable latency** on regular tokens while guaranteeing 100% token reconstitution across arbitrary network splits.

---

## 6. Upstream Dispatcher & Failover (`src/upstream/`)

The upstream client encapsulates connection pooling, credential injection, and automated resilience:

1. **Credential Decoupling :** The client application does not need to possess or transmit upstream API keys. Cloakd injects the appropriate provider key from its local configuration.
2. **Failover Decision Matrix :**
   * **Retryable :** `HTTP 429 Too Many Requests`, `HTTP 500`, `502`, `503`, `504`, or TCP connection drops.
   * **Non-Retryable :** `HTTP 400 Bad Request`, `HTTP 401 Unauthorized`.
3. **Cascade Execution :** When a retryable error occurs on the primary target, Cloakd inspects `CLOAKD_FALLBACK_PROVIDERS` and transparently invokes the next candidate with a valid key, updating the payload model name accordingly.

---

## 7. Two-Repository Architecture: Data Plane vs Control Plane

To maintain ultra-low latency, strict security boundaries, and enterprise deployment versatility, Cloakd's architecture is strictly partitioned into two decoupled components hosted in separate repositories:

```
┌─────────────────────────────────────────────────────────────────┐
│                 CONTROL PLANE (SaaS Web Application)            │
│                 Repository: cloakd-gateway/platform             │
├─────────────────────────────────────────────────────────────────┤
│  • Next.js Web Console, User Management & Authentication        │
│  • Multi-Tenant Organization Hierarchy & RBAC                   │
│  • BYOK Credential Vault (AES-256-GCM envelope encryption)      │
│  • Stripe Subscriptions, Metering & Quota Limits                │
│  • Management API for Tenant API Key & Policy Provisioning      │
└────────────────────────────────┬────────────────────────────────┘
                                 │
                                 │ Asynchronous Config Sync (Pull/Push)
                                 │ (Never in critical request path!)
                                 ▼
┌─────────────────────────────────────────────────────────────────┐
│                 DATA PLANE (Privacy & FinOps Gateway)           │
│                 Repository: cloakd-gateway/core (This Repo)     │
├─────────────────────────────────────────────────────────────────┤
│  • High-performance stateless Rust engine (< 10 MB RAM)         │
│  • Sub-millisecond Ingress DLP & Ephemeral SessionVault         │
│  • Real-time SSE Sliding-Window Transformation FSM              │
│  • In-Memory FinOps Cache (TinyLFU) & Multi-Provider Failover   │
│  • High-Speed Local Tenant Config LRU Cache (sub-50 µs)         │
│  • Deployable as Standalone, Docker, or Kubernetes Sidecar      │
└─────────────────────────────────────────────────────────────────┘
```

### Strategic Benefits of the Split

1. **Zero Attack Surface & Air-Gapped Independence :**
   The Data Plane (`cloakd-gateway/core`) operates completely autonomously. In sovereign, on-premise, or air-gapped enterprise environments, it can run without any dependency on the SaaS platform, loading its rules and provider keys from local environment variables or configuration files.

2. **Zero Overhead in the Critical Path :**
   The Rust gateway does not link to heavy database drivers, web frameworks, or auth SDKs. Tenant configurations are micro-cached locally with sub-50 µs resolution, keeping proxy latency under 1 ms.

3. **Independent Scalability & Operational Simplicity :**
   The Data Plane can be horizontally autoscaled at the edge or deployed as local Kubernetes sidecars without introducing database connection pool saturation on the SaaS Control Plane database.

---

## 8. Multi-Tenant Data Plane & Observability (v1.1.0)

### A. The Pluggable `TenantResolver` (`src/tenant/resolver.rs`)
At request ingress, Cloakd inspects the authentication bearer token (`sk-cloakd-...`) or headers (`x-cloakd-tenant-key`). The `TenantResolver` acts as a pluggable abstraction layer:
* **`TenantCache` (`src/tenant/cache.rs`) :** Ultra-fast in-memory LRU cache powered by `moka`, resolving tenant metadata in under 50 microseconds.
* **`EnvTenantResolver` :** Single-tenant zero-configuration fallback providing 100% backward compatibility with v1.0.0.
* **`StaticTenantResolver` (`src/tenant/static_file.rs`) :** Loads air-gapped tenant configurations from `tenants.yaml` or `tenants.json`.

### B. BYOK Credential Injection & Internal Token Stripping
When a tenant defines custom `provider_keys`:
1. `UpstreamClient` overrides server-level environment credentials with the tenant's specific upstream key for the resolved provider.
2. Ingress tenant tokens (`sk-cloakd-...`) are mathematically stripped from forwarded headers, preventing Cloakd internal keys from leaking to external LLM providers.

### C. Zero-Dependency Prometheus Metrics (`src/metrics/exporter.rs`)
Cloakd avoids bulky metric runtime dependencies by employing atomic, thread-safe `AtomicU64` counters partitioned by read-heavy `RwLock` hash structures:
* Scraped via standard `GET /metrics`.
* Formatted strictly conforming to Prometheus Text Format 0.0.4.
* Exposes total requests, cache hits/misses, DLP masked entity counts by type, and provider failover cascade frequencies.

---

## 9. Declarative Manifests & Model Governance (v1.2.0)

### A. Tri-Manifest Architecture (`kind: Tenant`, `kind: Role`, `kind: User`)
Inspired by Kubernetes custom resources and GitOps workflows, configurations can be split across any number of documents and files:
* **`Tenant`** : Defines the root organization, BYOK provider keys, baseline DLP rules, and default fallback models.
* **`Role`** : Scoped strictly to a `tenant_id` to guarantee zero cross-tenant leakage. Restricts model access (`allowed_models`) and specifies role-level default models.
* **`User`** : Maps a secret client key (`key: "sk-cloakd-..."`) to a `tenant_id` and `role`, with optional user-specific quota overrides.

### B. Cascading Configuration Inheritance
When compiling user context, Cloakd evaluates settings in order of specificity:

$$\mathbf{User} \longrightarrow \mathbf{Role} \longrightarrow \mathbf{Tenant} \longrightarrow \mathbf{Global\ (.env)}$$

* **`default_model`** : User $\rightarrow$ Role $\rightarrow$ Tenant $\rightarrow$ Server `.env`.
* **`allowed_models`** : User $\rightarrow$ Role $\rightarrow$ Tenant $\rightarrow$ All (`*`). Checked at request ingress; unauthorized models return `HTTP 403 Forbidden`.
* **`fallback_models`** : Explicit model-based failover list (e.g. `["gpt-4o-mini", "claude-3-5-haiku"]`), eliminating provider guessing.
* **`enabled_rules`** : Additive security union: $\text{Tenant} \cup \text{Role} \cup \text{User}$.

### C. Recursive Directory & Multi-Document Scanning (`src/tenant/static_file.rs`)
When `CLOAKD_TENANTS_FILE` points to a directory (`conf.d/`), Cloakd traverses all `.yaml`, `.yml`, and `.json` files recursively, resolves references, and pre-compiles effective user contexts into the memory LRU cache.


