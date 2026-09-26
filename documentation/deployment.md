# Cloakd — Production Deployment Guide

Cloakd's single-binary, zero-dependency architecture makes it trivial to deploy in any container or bare-metal environment.

---

## 1. Docker Deployment

### Multi-Stage Dockerfile
Use this production Dockerfile for a minimal, security-hardened container (~15 MB):

```dockerfile
# Build stage
FROM rust:1.85-slim as builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked

# Runtime stage
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/cloakd /usr/local/bin/cloakd

USER 10001:10001
EXPOSE 8080

ENTRYPOINT ["/usr/local/bin/cloakd"]
```

### Docker Run Command
```bash
docker run -d \
  --name cloakd \
  --restart unless-stopped \
  -p 8080:8080 \
  -e CLOAKD_DEFAULT_MODEL="gemini-3.5-flash-lite" \
  -e GEMINI_API_KEY="AIzaSy..." \
  -e OPENAI_API_KEY="sk-proj-..." \
  -e CLOAKD_ENABLED_RULES="default,us,uk" \
  -e CLOAKD_CACHE_ENABLED="true" \
  cloakd:latest
```

---

## 2. Kubernetes Deployments

### Option A: Kubernetes Sidecar (Recommended for Zero-Trust VPCs)
Inject Cloakd into the same pod as your application. Communication takes place strictly over `127.0.0.1`:

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: llm-application
spec:
  replicas: 3
  template:
    spec:
      containers:
        # Your core application container
        - name: app
          image: my-company/app:latest
          env:
            - name: OPENAI_BASE_URL
              value: "http://127.0.0.1:8080/v1"

        # Cloakd Privacy Gateway Sidecar
        - name: cloakd-sidecar
          image: cloakd:1.0.0
          resources:
            requests:
              cpu: "50m"
              memory: "16Mi"
            limits:
              cpu: "500m"
              memory: "64Mi"
          env:
            - name: CLOAKD_PORT
              value: "8080"
            - name: GEMINI_API_KEY
              valueFrom:
                secretKeyRef:
                  name: llm-secrets
                  key: gemini-api-key
          ports:
            - containerPort: 8080
          livenessProbe:
            httpGet:
              path: /health
              port: 8080
            initialDelaySeconds: 2
            periodSeconds: 10
```

### Option B: Standalone Internal Gateway Service
Expose Cloakd across the cluster as an internal `ClusterIP` service:

```yaml
apiVersion: v1
kind: Service
metadata:
  name: cloakd-gateway
spec:
  type: ClusterIP
  selector:
    app: cloakd
  ports:
    - port: 8080
      targetPort: 8080
```
All cluster pods can then target `http://cloakd-gateway.default.svc.cluster.local:8080/v1`.

---

## 3. Bare-Metal / Systemd Service

Create `/etc/systemd/system/cloakd.service`:

```ini
[Unit]
Description=Cloakd LLM Privacy Gateway
After=network.target

[Service]
Type=simple
User=cloakd
Group=cloakd
WorkingDirectory=/opt/cloakd
ExecStart=/opt/cloakd/cloakd
Restart=always
RestartSec=3
EnvironmentFile=/opt/cloakd/.env

# Security sandbox
ProtectSystem=strict
ProtectHome=true
NoNewPrivileges=true

[Install]
WantedBy=multi-user.target
```

Enable and start:
```bash
sudo systemctl daemon-reload
sudo systemctl enable --now cloakd
```
