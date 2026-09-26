# Cloakd — Configuration Reference

This guide details all environment variables supported by Cloakd, their default values, and production configuration profiles.

---

## 1. Environment Variables Reference

### Network & Core Gateway

| Variable | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `CLOAKD_HOST` | String | `0.0.0.0` | Network interface IP to bind. |
| `CLOAKD_PORT` | Integer | `8080` | Port on which Cloakd listens for incoming HTTP requests. |
| `CLOAKD_LOG_LEVEL` | String | `info` | Logging verbosity (`trace`, `debug`, `info`, `warn`, `error`). Outputs structured JSON. |

### Model Routing & Provider Credentials

Cloakd automatically determines which upstream provider to route to based on the requested model name. Upstream API keys are managed locally by Cloakd so client applications do not need access to them.

| Variable | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `CLOAKD_DEFAULT_MODEL` | String | `gemini-3.8-flash` | Fallback model used when the client omits the `model` field in `/v1/chat/completions`. |
| `GEMINI_API_KEY` | String | *None* | Google Gemini API key. Routes models starting with `gemini-`. |
| `OPENAI_API_KEY` | String | *None* | OpenAI API key. Routes models starting with `gpt-`, `o1`, `o3`, `chatgpt-`. |
| `ANTHROPIC_API_KEY` | String | *None* | Anthropic API key. Routes models starting with `claude-`. |
| `UPSTREAM_BASE_URL` | String | `https://api.openai.com` | Base URL for custom or self-hosted LLM backends (Ollama, vLLM, LocalAI). Routes models like `llama-`, `mistral-`, `qwen-`, `custom-`. |
| `UPSTREAM_API_KEY` | String | *None* | Optional secret bearer key for the custom upstream endpoint. |

### Granular DLP Rule Packs

| Variable | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `CLOAKD_ENABLED_RULES` | String | `default` | Comma-separated list of active rules or regional packs. Inactive rules have zero CPU/memory overhead. |

**Available Built-In Packs:**
* `default` : Universal rules (`email, card, iban, phone, secrets, ip`).
* `all` : All universal and national rules.
* `banking` : `card, iban`.
* `secrets` : `secrets, ip` (Focus Cyber Resilience Act / CRA & NIS 2).
* `pii` : `email, phone`.
* `eu` : `iban, fr_nir, uk_nino, es_dni`.
* Country packs : `fr` (`fr_nir`), `us` (`us_ssn`), `uk` (`uk_nino`), `es` (`es_dni`).

### FinOps In-Memory Cache (V2)

| Variable | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `CLOAKD_CACHE_ENABLED` | Boolean | `true` | Enables or disables the local in-memory prompt cache. |
| `CLOAKD_CACHE_TTL_SECS` | Integer | `3600` | Time-To-Live for cached completions in seconds (default is 1 hour). |
| `CLOAKD_CACHE_MAX_CAPACITY` | Integer | `10000` | Maximum number of cached prompt entries before TinyLFU eviction. |

### Resilience & Multi-Provider Failover (V2)

| Variable | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `CLOAKD_FAILOVER_ENABLED` | Boolean | `true` | Enables automatic failover to fallback providers upon HTTP 429 or 5xx errors. |
| `CLOAKD_FALLBACK_PROVIDERS` | String | `gemini,openai,anthropic` | Comma-separated preferred order of fallback providers. |

---

## 2. Production Configuration Profiles

### Profile A: Standard Production (Gemini Default + OpenAI Fallback)
```env
CLOAKD_HOST=0.0.0.0
CLOAKD_PORT=8080
CLOAKD_LOG_LEVEL=info

CLOAKD_DEFAULT_MODEL=gemini-3.5-flash-lite
GEMINI_API_KEY=AIzaSy...
OPENAI_API_KEY=sk-proj-...

CLOAKD_ENABLED_RULES=default,us,uk
CLOAKD_CACHE_ENABLED=true
CLOAKD_FAILOVER_ENABLED=true
CLOAKD_FALLBACK_PROVIDERS=gemini,openai
```

### Profile B: Strict European Banking & FinOps Profile
Focus on zero-persistence, ISO 13616 Modulo 97-10 IBANs, and strict EU national numbers:
```env
CLOAKD_HOST=127.0.0.1
CLOAKD_PORT=8080
CLOAKD_LOG_LEVEL=warn

CLOAKD_DEFAULT_MODEL=gemini-3.5-flash-lite
GEMINI_API_KEY=AIzaSy...

CLOAKD_ENABLED_RULES=banking,eu,secrets
CLOAKD_CACHE_ENABLED=true
CLOAKD_CACHE_TTL_SECS=7200
CLOAKD_CACHE_MAX_CAPACITY=50000
CLOAKD_FAILOVER_ENABLED=false
```

### Profile C: Sovereign / Air-Gapped Local Cluster (vLLM / Ollama)
No data leaves the local network:
```env
CLOAKD_HOST=0.0.0.0
CLOAKD_PORT=8080

CLOAKD_DEFAULT_MODEL=llama-3-8b
UPSTREAM_BASE_URL=http://vllm-service.internal:8000/v1
UPSTREAM_API_KEY=

CLOAKD_ENABLED_RULES=all
CLOAKD_CACHE_ENABLED=true
CLOAKD_FAILOVER_ENABLED=false
```
