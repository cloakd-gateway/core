# Cloakd — Client Integration Guide

Cloakd implements 100% of the OpenAI `/v1/chat/completions` API specification. Switching your application to Cloakd requires changing only the `base_url` parameter.

---

## 1. Python Integration

### Official OpenAI Python SDK
```python
from openai import OpenAI

# Point client directly to Cloakd gateway
client = OpenAI(
    base_url="http://localhost:8080/v1",
    api_key="not-needed" # Cloakd manages upstream credentials
)

# Standard non-streaming call
response = client.chat.completions.create(
    model="gemini-3.5-flash-lite", # Or gpt-4o, claude-3-5-sonnet, llama-3-8b
    messages=[
        {"role": "system", "content": "You are a customer support agent."},
        {"role": "user", "content": "Update record for client john.doe@acme.com and card 4532-0150-1808-1114."}
    ]
)

print(response.choices[0].message.content)
```

### Streaming Responses (SSE)
```python
stream = client.chat.completions.create(
    model="gemini-3.5-flash-lite",
    messages=[{"role": "user", "content": "Summarize user history for contact@partner.fr"}],
    stream=True
)

for chunk in stream:
    delta = chunk.choices[0].delta.content
    if delta:
        print(delta, end="", flush=True)
```

### LangChain Integration
```python
from langchain_openai import ChatOpenAI

llm = ChatOpenAI(
    base_url="http://localhost:8080/v1",
    api_key="not-needed",
    model="gemini-3.5-flash-lite",
)

response = llm.invoke("Review file for patient with SSN 219-45-7819.")
print(response.content)
```

---

## 2. TypeScript / Node.js Integration

### Official OpenAI SDK
```typescript
import OpenAI from "openai";

const openai = new OpenAI({
  baseURL: "http://localhost:8080/v1",
  apiKey: "not-needed",
});

async function main() {
  const completion = await openai.chat.completions.create({
    model: "gemini-3.5-flash-lite",
    messages: [
      { role: "user", content: "Transfer 500 EUR to IBAN FR14 2004 1010 0505 0001 3M02 606." }
    ],
  });

  console.log(completion.choices[0].message.content);
}

main();
```

---

## 3. Go Integration

Using `github.com/sashabaranov/go-openai`:

```go
package main

import (
    "context"
    "fmt"
    openai "github.com/sashabaranov/go-openai"
)

func main() {
    config := openai.DefaultConfig("not-needed")
    config.BaseURL = "http://localhost:8080/v1"
    client := openai.NewClientWithConfig(config)

    resp, err := client.CreateChatCompletion(
        context.Background(),
        openai.ChatCompletionRequest{
            Model: "gemini-3.5-flash-lite",
            Messages: []openai.ChatCompletionMessage{
                {
                    Role:    openai.ChatMessageRoleUser,
                    Content: "Please contact support@client.de.",
                },
            },
        },
    )
    if err != nil {
        panic(err)
    }

    fmt.Println(resp.Choices[0].Message.Content)
}
```

---

## 4. Inspecting Audit Response Headers

Every response processed by Cloakd includes auditing headers for SecOps transparency:

```bash
curl -i -X POST http://127.0.0.1:8080/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gemini-3.5-flash-lite",
    "messages": [{"role": "user", "content": "Secret key AKIAIOSFODNN7EXAMPLE."}]
  }'
```

**Returned Headers:**
* `x-cloakd-masked-count: 1` : Number of sensitive entities intercepted and pseudonymized.
* `x-cloakd-provider: gemini` : The upstream LLM provider that processed the prompt.
* `x-cloakd-model: gemini-3.5-flash-lite` : The resolved model identifier.
* `x-cloakd-cache: HIT / MISS` : Shows if the response was served from in-memory cache.
* `x-cloakd-fallback: true / false` : Shows whether upstream failover occurred.
