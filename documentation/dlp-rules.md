# Cloakd — DLP Rules & Checksums Catalogue

This document details the mathematical checksums, regular expressions, and regulatory frameworks underpinning Cloakd's inspection engine.

---

## 1. Universal Rules

### A. `iban` — International Bank Account Numbers
* **Standard :** ISO 13616 (Format) & **ISO 7064 Modulo 97-10** (Verification).
* **Coverage :** 75+ countries (France, Germany, UK, Spain, Italy, Switzerland, Belgium, Netherlands, Poland, UAE, etc.).
* **Algorithm :**
  1. Validates exact country character length (e.g. FR=27, DE=22, GB=22, ES=24, IT=27, CH=21, BE=16, NL=18).
  2. Rearranges the string by moving the first 4 characters (country code + check digits) to the end.
  3. Converts letters to numbers ($A=10, B=11 \dots Z=35$).
  4. Computes $\text{remainder} = \text{number} \pmod{97}$ using a streaming $O(N)$ integer accumulator to avoid heap allocation.
  5. The IBAN is valid if and only if $\text{remainder} == 1$.
* **Guarantee :** Zero false-positive detection.

---

### B. `secrets` — Cloud Credentials & Tokens (CRA & NIS 2)
Aligned with the **EU Cyber Resilience Act (CRA)** and **NIS 2 Directive** to prevent enterprise credential leaks into public AI models:
* **AWS Access Key IDs :** `\bAKIA[0-9A-Z]{16}\b`
* **GitHub Tokens :** Personal Access Tokens (`ghp_`, `gho_`, `ghu_`, `ghs_`, `ghr_`) and fine-grained PATs (`github_pat_...`).
* **Google Cloud API Keys :** `\bAIzaSy[A-Za-z0-9_-]{33}\b`
* **OpenAI & Anthropic Keys :** `\bsk-(?:proj-|ant-)?[A-Za-z0-9_-]{32,}\b`
* **JSON Web Tokens (JWT) :** Three Base64URL-encoded segments separated by periods (`eyJ...`).
* **PEM Private Key Blocks :** `-----BEGIN [A-Z ]*PRIVATE KEY-----` headers and footers.
* **Database Connection URIs :** Credentials embedded in `postgres://`, `mysql://`, `mongodb://`, `redis://` connection strings.

---

### C. `card` — Credit & Debit Cards
* **Standard :** ANSI X4.13 & ISO/IEC 7812.
* **Schemes :** Visa, MasterCard, American Express, Discover, Diners Club, JCB.
* **Algorithm :** **Luhn Algorithm (Modulo 10)** :
  * Doubling every second digit from right to left, summing digits, ensuring $\text{sum} \pmod{10} == 0$.
  * Eliminates false alarms on random 16-digit serial numbers.

---

### D. `phone` — Global Telephony
* **International :** ITU-T recommendation **E.164** (`\+(?:[1-9]\d{0,2})[\s.-]?...`).
* **National formats :**
  * North American Numbering Plan (NANP) : `(xxx) xxx-xxxx` or `xxx-xxx-xxxx`.
  * France : `0[1-9] xx xx xx xx`.

---

### E. `ip` — Private Infrastructure IP Addresses
* **Standard :** RFC 1918 Private IPv4 address ranges:
  * `10.0.0.0/8`
  * `172.16.0.0/12`
  * `192.168.0.0/16`
  * Loopback `127.0.0.0/8`
* Public IPs (e.g. `8.8.8.8`, `1.1.1.1`) are intentionally **not** masked to preserve technical context in programming and infrastructure prompts.

---

### F. `email` — Email Addresses
* **Standard :** RFC 5322 compliant regex with boundary enforcement.

---

## 2. National Identity Numbers (`src/dlp/rules/national/`)

### A. France — `fr_nir` (Numéro de Sécurité Sociale)
* **Format :** 13 digits (core NIR) or 15 digits (with control key).
* **Validation :**
  * Gender digit: `1` (Male) or `2` (Female).
  * Month of birth: `01`–`12`, special birth codes (`20`–`42`), or `99`.
  * 15-digit validation: **Modulo 97 control key** ($\text{Key} = 97 - (\text{NIR}_{13} \pmod{97})$).

---

### B. United States — `us_ssn` (Social Security Number)
* **Format :** `XXX-XX-XXXX` (9 digits).
* **Validation :** Social Security Administration (SSA) restrictions:
  * **Area Number :** Cannot be `000`, `666`, or in the range `900`–`999`.
  * **Group Number :** Cannot be `00`.
  * **Serial Number :** Cannot be `0000`.

---

### C. United Kingdom — `uk_nino` (National Insurance Number)
* **Format :** 2 prefix letters, 6 digits, 1 suffix letter (`A`–`D`).
* **Validation :** HM Revenue & Customs (HMRC) restrictions:
  * Letters `D, F, I, Q, U, V` are strictly prohibited in 1st and 2nd positions.
  * Letter `O` is prohibited as 2nd letter.
  * Prefix combinations `BG, GB, KN, NK, NT, TN, ZZ` are reserved and invalid.

---

### D. Spain — `es_dni` (DNI & NIE)
* **Format :**
  * DNI: 8 digits + 1 control letter.
  * NIE: Prefix `X` (0), `Y` (1), or `Z` (2) + 7 digits + 1 control letter.
* **Validation :** Official **Modulo 23** lookup algorithm using string `"TRWAGMYFPDXBNJZSQVHLCKE"`.

---

## 3. Adding a Custom DLP Rule

Thanks to the modular trait architecture (`src/dlp/rules/`), adding a new enterprise-specific rule (e.g. employee badge number, internal order ID) requires creating a single file:

```rust
use crate::dlp::rules::DlpRule;
use crate::dlp::types::{EntityType, MatchSpan};
use regex::Regex;
use std::sync::LazyLock;

static BADGE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\bEMP-[0-9]{6}\b").expect("Badge regex compiles")
});

pub struct EmployeeBadgeRule;

impl DlpRule for EmployeeBadgeRule {
    fn entity_type(&self) -> EntityType {
        EntityType::Secret // Or your custom variant
    }

    fn find_matches(&self, text: &str) -> Vec<MatchSpan> {
        BADGE_REGEX
            .find_iter(text)
            .map(|m| MatchSpan {
                start: m.start(),
                end: m.end(),
                entity_type: self.entity_type(),
                text: m.as_str().to_string(),
            })
            .collect()
    }
}
```
Register it in `src/dlp/rules/mod.rs` and it becomes instantly selectable via `CLOAKD_ENABLED_RULES`.
