//! Client-side secret scanner for team memory.
//!
//! Scans content for credentials before upload so secrets never leave the
//! user's machine. Uses a curated subset of high-confidence rules from
//! gitleaks — only rules with distinctive prefixes that have near-zero
//! false-positive rates are included.

use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashSet;

/// A match result from the scanner (rule ID + human-readable label).
#[derive(Debug, Clone)]
pub struct SecretMatch {
    /// Scanner rule ID that matched (e.g., "github-pat", "aws-access-token").
    pub rule_id: String,
    /// Human-readable label derived from the rule ID.
    pub label: String,
}

struct SecretRule {
    id: &'static str,
    source: &'static str,
    case_insensitive: bool,
}

static SECRET_RULES: &[SecretRule] = &[
    // Cloud providers
    SecretRule {
        id: "aws-access-token",
        source: r"\b((?:A3T[A-Z0-9]|AKIA|ASIA|ABIA|ACCA)[A-Z2-7]{16})\b",
        case_insensitive: false,
    },
    SecretRule {
        id: "gcp-api-key",
        source: r#"\b(AIza[\w\-]{35})(?:[`'"\s;]|\\[nr]|$)"#,
        case_insensitive: false,
    },
    SecretRule {
        id: "azure-ad-client-secret",
        source: r#"(?:^|[\\'"`\s>=:(,)])([a-zA-Z0-9_~.]{3}\dQ~[a-zA-Z0-9_~.\-]{31,34})(?:$|[\\'"`\s<),])"#,
        case_insensitive: false,
    },
    SecretRule {
        id: "digitalocean-pat",
        source: r#"\b(dop_v1_[a-f0-9]{64})(?:[`'"\s;]|\\[nr]|$)"#,
        case_insensitive: false,
    },
    SecretRule {
        id: "digitalocean-access-token",
        source: r#"\b(doo_v1_[a-f0-9]{64})(?:[`'"\s;]|\\[nr]|$)"#,
        case_insensitive: false,
    },
    // AI APIs
    SecretRule {
        id: "third-party-ai-api-key",
        source: r#"\b(sk-mossen-api03-[a-zA-Z0-9_\-]{93}AA)(?:[`'"\s;]|\\[nr]|$)"#,
        case_insensitive: false,
    },
    SecretRule {
        id: "third-party-ai-admin-api-key",
        source: r#"\b(sk-mossen-admin01-[a-zA-Z0-9_\-]{93}AA)(?:[`'"\s;]|\\[nr]|$)"#,
        case_insensitive: false,
    },
    SecretRule {
        id: "openai-api-key",
        source: r#"\b(sk-(?:proj|svcacct|admin)-(?:[A-Za-z0-9_\-]{74}|[A-Za-z0-9_\-]{58})T3BlbkFJ(?:[A-Za-z0-9_\-]{74}|[A-Za-z0-9_\-]{58})\b|sk-[a-zA-Z0-9]{20}T3BlbkFJ[a-zA-Z0-9]{20})(?:[`'"\s;]|\\[nr]|$)"#,
        case_insensitive: false,
    },
    SecretRule {
        id: "huggingface-access-token",
        source: r#"\b(hf_[a-zA-Z]{34})(?:[`'"\s;]|\\[nr]|$)"#,
        case_insensitive: false,
    },
    // Version control
    SecretRule {
        id: "github-pat",
        source: r"ghp_[0-9a-zA-Z]{36}",
        case_insensitive: false,
    },
    SecretRule {
        id: "github-fine-grained-pat",
        source: r"github_pat_\w{82}",
        case_insensitive: false,
    },
    SecretRule {
        id: "github-app-token",
        source: r"(?:ghu|ghs)_[0-9a-zA-Z]{36}",
        case_insensitive: false,
    },
    SecretRule {
        id: "github-oauth",
        source: r"gho_[0-9a-zA-Z]{36}",
        case_insensitive: false,
    },
    SecretRule {
        id: "github-refresh-token",
        source: r"ghr_[0-9a-zA-Z]{36}",
        case_insensitive: false,
    },
    SecretRule {
        id: "gitlab-pat",
        source: r"glpat-[\w\-]{20}",
        case_insensitive: false,
    },
    SecretRule {
        id: "gitlab-deploy-token",
        source: r"gldt-[0-9a-zA-Z_\-]{20}",
        case_insensitive: false,
    },
    // Communication
    SecretRule {
        id: "slack-bot-token",
        source: r"xoxb-[0-9]{10,13}-[0-9]{10,13}[a-zA-Z0-9\-]*",
        case_insensitive: false,
    },
    SecretRule {
        id: "slack-user-token",
        source: r"xox[pe](?:-[0-9]{10,13}){3}-[a-zA-Z0-9\-]{28,34}",
        case_insensitive: false,
    },
    SecretRule {
        id: "slack-app-token",
        source: r"xapp-\d-[A-Z0-9]+-\d+-[a-z0-9]+",
        case_insensitive: true,
    },
    SecretRule {
        id: "twilio-api-key",
        source: r"SK[0-9a-fA-F]{32}",
        case_insensitive: false,
    },
    SecretRule {
        id: "sendgrid-api-token",
        source: r#"\b(SG\.[a-zA-Z0-9=_\-.]{66})(?:[`'"\s;]|\\[nr]|$)"#,
        case_insensitive: false,
    },
    // Dev tooling
    SecretRule {
        id: "npm-access-token",
        source: r#"\b(npm_[a-zA-Z0-9]{36})(?:[`'"\s;]|\\[nr]|$)"#,
        case_insensitive: false,
    },
    SecretRule {
        id: "pypi-upload-token",
        source: r"pypi-AgEIcHlwaS5vcmc[\w\-]{50,1000}",
        case_insensitive: false,
    },
    SecretRule {
        id: "databricks-api-token",
        source: r#"\b(dapi[a-f0-9]{32}(?:-\d)?)(?:[`'"\s;]|\\[nr]|$)"#,
        case_insensitive: false,
    },
    SecretRule {
        id: "hashicorp-tf-api-token",
        source: r"[a-zA-Z0-9]{14}\.atlasv1\.[a-zA-Z0-9\-_=]{60,70}",
        case_insensitive: false,
    },
    SecretRule {
        id: "pulumi-api-token",
        source: r#"\b(pul-[a-f0-9]{40})(?:[`'"\s;]|\\[nr]|$)"#,
        case_insensitive: false,
    },
    SecretRule {
        id: "postman-api-token",
        source: r#"\b(PMAK-[a-fA-F0-9]{24}-[a-fA-F0-9]{34})(?:[`'"\s;]|\\[nr]|$)"#,
        case_insensitive: false,
    },
    // Observability
    SecretRule {
        id: "grafana-api-key",
        source: r#"\b(eyJrIjoi[A-Za-z0-9+/]{70,400}={0,3})(?:[`'"\s;]|\\[nr]|$)"#,
        case_insensitive: false,
    },
    SecretRule {
        id: "grafana-cloud-api-token",
        source: r#"\b(glc_[A-Za-z0-9+/]{32,400}={0,3})(?:[`'"\s;]|\\[nr]|$)"#,
        case_insensitive: false,
    },
    SecretRule {
        id: "grafana-service-account-token",
        source: r#"\b(glsa_[A-Za-z0-9]{32}_[A-Fa-f0-9]{8})(?:[`'"\s;]|\\[nr]|$)"#,
        case_insensitive: false,
    },
    SecretRule {
        id: "sentry-user-token",
        source: r#"\b(sntryu_[a-f0-9]{64})(?:[`'"\s;]|\\[nr]|$)"#,
        case_insensitive: false,
    },
    SecretRule {
        id: "sentry-org-token",
        source: r"\bsntrys_eyJpYXQiO[a-zA-Z0-9+/]{10,200}(?:LCJyZWdpb25fdXJs|InJlZ2lvbl91cmwi|cmVnaW9uX3VybCI6)[a-zA-Z0-9+/]{10,200}={0,2}_[a-zA-Z0-9+/]{43}",
        case_insensitive: false,
    },
    // Payment / commerce
    SecretRule {
        id: "stripe-access-token",
        source: r#"\b((?:sk|rk)_(?:test|live|prod)_[a-zA-Z0-9]{10,99})(?:[`'"\s;]|\\[nr]|$)"#,
        case_insensitive: false,
    },
    SecretRule {
        id: "shopify-access-token",
        source: r"shpat_[a-fA-F0-9]{32}",
        case_insensitive: false,
    },
    SecretRule {
        id: "shopify-shared-secret",
        source: r"shpss_[a-fA-F0-9]{32}",
        case_insensitive: false,
    },
    // Crypto
    SecretRule {
        id: "private-key",
        source: r"-----BEGIN[ A-Z0-9_\-]{0,100}PRIVATE KEY(?: BLOCK)?-----[\s\S\-]{64,}?-----END[ A-Z0-9_\-]{0,100}PRIVATE KEY(?: BLOCK)?-----",
        case_insensitive: true,
    },
];

struct CompiledRule {
    id: &'static str,
    re: Regex,
}

static COMPILED_RULES: Lazy<Vec<CompiledRule>> = Lazy::new(|| {
    SECRET_RULES
        .iter()
        .filter_map(|r| {
            let pattern = if r.case_insensitive {
                format!("(?i){}", r.source)
            } else {
                r.source.to_string()
            };
            match Regex::new(&pattern) {
                Ok(re) => Some(CompiledRule { id: r.id, re }),
                Err(_) => None,
            }
        })
        .collect()
});

/// Convert a scanner rule ID (kebab-case) to a human-readable label.
pub fn get_secret_label(rule_id: &str) -> String {
    rule_id_to_label(rule_id)
}

fn rule_id_to_label(rule_id: &str) -> String {
    rule_id
        .split('-')
        .map(|part| match part {
            "aws" => "AWS".to_string(),
            "gcp" => "GCP".to_string(),
            "api" => "API".to_string(),
            "pat" => "PAT".to_string(),
            "ad" => "AD".to_string(),
            "tf" => "TF".to_string(),
            "ai" => "AI".to_string(),
            "oauth" => "OAuth".to_string(),
            "npm" => "NPM".to_string(),
            "pypi" => "PyPI".to_string(),
            "jwt" => "JWT".to_string(),
            "github" => "GitHub".to_string(),
            "gitlab" => "GitLab".to_string(),
            "openai" => "OpenAI".to_string(),
            "digitalocean" => "DigitalOcean".to_string(),
            "huggingface" => "HuggingFace".to_string(),
            "hashicorp" => "HashiCorp".to_string(),
            "sendgrid" => "SendGrid".to_string(),
            _ => capitalize(part),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
}

/// Scan a string for potential secrets.
///
/// Returns one match per rule that fired (deduplicated by rule ID). The
/// actual matched text is intentionally NOT returned.
pub fn scan_for_secrets(content: &str) -> Vec<SecretMatch> {
    let mut matches = Vec::new();
    let mut seen = HashSet::new();

    for rule in COMPILED_RULES.iter() {
        if seen.contains(rule.id) {
            continue;
        }
        if rule.re.is_match(content) {
            seen.insert(rule.id);
            matches.push(SecretMatch {
                rule_id: rule.id.to_string(),
                label: rule_id_to_label(rule.id),
            });
        }
    }

    matches
}

/// Redact any matched secrets in-place with [REDACTED].
pub fn redact_secrets(content: &str) -> String {
    let mut result = content.to_string();
    for rule in COMPILED_RULES.iter() {
        result = rule.re.replace_all(&result, "[REDACTED]").to_string();
    }
    result
}
