use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntityType {
    Email,
    Nir,        // Numéro d'inscription au répertoire (Sécurité Sociale FR)
    CreditCard, // Carte bancaire (Luhn)
    Iban,       // International Bank Account Number (ISO 7064 Mod 97-10)
    Phone,      // Téléphone international E.164 / national
    Secret,     // Clés API, JWT, PEM, Credentials (CRA / NIS 2)
    IpAddress,  // Adresse IP privée RFC 1918
    UsSsn,      // US Social Security Number
    UkNino,     // UK National Insurance Number
    EsDni,      // Documento Nacional de Identidad / NIE (Espagne)
}

impl EntityType {
    pub fn code(&self) -> &'static str {
        match self {
            EntityType::Email => "EMAIL",
            EntityType::Nir => "NIR",
            EntityType::CreditCard => "CARD",
            EntityType::Iban => "IBAN",
            EntityType::Phone => "PHONE",
            EntityType::Secret => "SECRET",
            EntityType::IpAddress => "IP",
            EntityType::UsSsn => "US_SSN",
            EntityType::UkNino => "UK_NINO",
            EntityType::EsDni => "ES_DNI",
        }
    }

    pub fn rule_name(&self) -> &'static str {
        match self {
            EntityType::Email => "email",
            EntityType::Nir => "fr_nir",
            EntityType::CreditCard => "card",
            EntityType::Iban => "iban",
            EntityType::Phone => "phone",
            EntityType::Secret => "secrets",
            EntityType::IpAddress => "ip",
            EntityType::UsSsn => "us_ssn",
            EntityType::UkNino => "uk_nino",
            EntityType::EsDni => "es_dni",
        }
    }

    #[allow(dead_code)]
    pub fn display_name(&self) -> &'static str {
        match self {
            EntityType::Email => "Email Address",
            EntityType::Nir => "French Social Security Number (NIR)",
            EntityType::CreditCard => "Credit Card Number",
            EntityType::Iban => "International Bank Account Number (IBAN)",
            EntityType::Phone => "Phone Number",
            EntityType::Secret => "API Key or Secret Token",
            EntityType::IpAddress => "Private IP Address",
            EntityType::UsSsn => "US Social Security Number",
            EntityType::UkNino => "UK National Insurance Number",
            EntityType::EsDni => "Spanish Identity Number (DNI/NIE)",
        }
    }
}

impl fmt::Display for EntityType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.code())
    }
}

#[derive(Debug, Clone)]
pub struct MatchSpan {
    pub start: usize,
    pub end: usize,
    pub entity_type: EntityType,
    pub text: String,
}

