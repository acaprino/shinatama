//! Secret redaction applied before any export byte is written (R33, ADR-0021 INV-005).
//!
//! Both modes remove credentials: private-key blocks, provider and forge tokens, JWTs, bearer
//! and basic authorization values, credentials in URLs and `secret=value` style assignments.
//! `portable` also replaces the user's home directory with `~` and removes e-mail addresses.
//! The scanner is hand-written (no pattern-engine dependency) and runs in linear time.
use crate::model::Mode;
use std::collections::BTreeMap;

/// Redaction counts by class, written into the export header and report.
pub type Redactions = BTreeMap<String, u64>;

pub struct Redactor {
    mode: Mode,
    home: Vec<String>,
    counts: Redactions,
    /// `REDACTED` for exports, `redacted` for seeds (FRD-0005 R27).
    marker: &'static str,
    /// Long high-entropy tokens; on for seeds, off for exports.
    high_entropy: bool,
}

/// Token prefixes with the shortest body that still looks like a credential.
const PREFIXES: &[(&str, &str, usize)] = &[
    ("sk-ant-", "anthropic_key", 20),
    ("sk-proj-", "openai_key", 20),
    ("sk-or-", "openrouter_key", 20),
    ("sk-", "api_key", 20),
    ("github_pat_", "github_token", 20),
    ("ghp_", "github_token", 20),
    ("gho_", "github_token", 20),
    ("ghu_", "github_token", 20),
    ("ghs_", "github_token", 20),
    ("ghr_", "github_token", 20),
    ("glpat-", "gitlab_token", 20),
    ("xoxb-", "slack_token", 10),
    ("xoxp-", "slack_token", 10),
    ("xoxa-", "slack_token", 10),
    ("xoxs-", "slack_token", 10),
    ("xapp-", "slack_token", 10),
    ("AKIA", "aws_access_key", 16),
    ("ASIA", "aws_access_key", 16),
    ("AIza", "google_api_key", 30),
    ("hf_", "huggingface_token", 30),
    ("npm_", "npm_token", 30),
    ("pypi-", "pypi_token", 30),
    ("dop_v1_", "digitalocean_token", 40),
    ("shpat_", "shopify_token", 20),
    ("rk_live_", "stripe_key", 16),
    ("sk_live_", "stripe_key", 16),
    ("pk_live_", "stripe_key", 16),
];

/// Assignment keys whose value is treated as a secret. Matched on the key's lowercase suffix.
const SECRET_KEYS: &[&str] = &[
    "password",
    "passwd",
    "pwd",
    "secret",
    "api_key",
    "apikey",
    "api-key",
    "token",
    "access_key",
    "secret_key",
    "private_key",
    "client_secret",
    "auth",
    "authorization",
    "credentials",
    "session_key",
    "cookie",
];

fn token_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-')
}
fn word_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

impl Redactor {
    /// `home` is the user's profile directory; it is used only in `portable` mode.
    pub fn new(mode: Mode, home: Option<&str>) -> Self {
        let mut forms = Vec::new();
        if mode == Mode::Portable
            && let Some(home) = home.map(|h| h.trim_end_matches(['\\', '/']))
            && home.len() >= 3
        {
            forms.push(home.to_owned());
            forms.push(home.replace('\\', "/"));
            forms.push(home.replace('\\', "\\\\"));
            forms.sort();
            forms.dedup();
            // Longest first so the escaped form wins over its prefix.
            forms.sort_by_key(|form| std::cmp::Reverse(form.len()));
        }
        Self {
            mode,
            home: forms,
            counts: Redactions::new(),
            marker: "REDACTED",
            high_entropy: false,
        }
    }
    /// The seed rule set (FRD-0005 R27): `[redacted:<kind>]` markers and the high-entropy rule.
    pub fn for_seed() -> Self {
        let mut redactor = Self::new(Mode::Full, None);
        redactor.marker = "redacted";
        redactor.high_entropy = true;
        redactor
    }
    fn mark(&self, class: &str) -> String {
        format!("[{}:{class}]", self.marker)
    }
    pub fn mode(&self) -> Mode {
        self.mode
    }
    pub fn counts(&self) -> &Redactions {
        &self.counts
    }
    pub fn into_counts(self) -> Redactions {
        self.counts
    }
    fn count(&mut self, class: &str) {
        *self.counts.entry(class.to_owned()).or_insert(0) += 1;
    }

    /// Redact one text. Idempotent: an already redacted marker is left alone.
    pub fn redact(&mut self, text: &str) -> String {
        let mut value = self.private_keys(text);
        value = self.scan(&value);
        if self.mode == Mode::Portable {
            value = self.home_paths(&value);
            value = self.emails(&value);
        }
        value
    }

    /// Redact every string inside a JSON value (keys are kept).
    pub fn redact_json(&mut self, value: &mut serde_json::Value) {
        match value {
            serde_json::Value::String(text) => {
                let redacted = self.redact(text);
                if redacted != *text {
                    *text = redacted;
                }
            }
            serde_json::Value::Array(items) => items.iter_mut().for_each(|v| self.redact_json(v)),
            serde_json::Value::Object(map) => {
                for (key, item) in map.iter_mut() {
                    let lower = key.to_ascii_lowercase();
                    if item.is_string()
                        && SECRET_KEYS.iter().any(|k| lower.ends_with(k))
                        && item.as_str().is_some_and(plausible_secret_value)
                    {
                        self.count("assignment");
                        *item = serde_json::Value::String(self.mark("assignment"));
                    } else {
                        self.redact_json(item);
                    }
                }
            }
            _ => {}
        }
    }

    fn private_keys(&mut self, text: &str) -> String {
        const BEGIN: &str = "-----BEGIN ";
        if !text.contains(BEGIN) {
            return text.to_owned();
        }
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(start) = rest.find(BEGIN) {
            let header_end = rest[start..].find("-----\n").or_else(|| {
                rest[start + BEGIN.len()..]
                    .find("-----")
                    .map(|i| i + BEGIN.len())
            });
            let Some(header_end) = header_end else { break };
            let header = &rest[start..start + header_end];
            if !header.contains("PRIVATE KEY") {
                out.push_str(&rest[..start + BEGIN.len()]);
                rest = &rest[start + BEGIN.len()..];
                continue;
            }
            let Some(end) = rest[start..].find("-----END ") else {
                // An unterminated block: redact to the end rather than leak half a key.
                out.push_str(&rest[..start]);
                out.push_str(&self.mark("private_key"));
                self.count("private_key");
                return out;
            };
            let end = start + end;
            let close = rest[end + 9..]
                .find("-----")
                .map_or(rest.len(), |i| end + 9 + i + 5);
            out.push_str(&rest[..start]);
            out.push_str(&self.mark("private_key"));
            self.count("private_key");
            rest = &rest[close..];
        }
        out.push_str(rest);
        out
    }

    fn scan(&mut self, text: &str) -> String {
        let bytes = text.as_bytes();
        let mut out = String::with_capacity(text.len());
        let mut copied = 0;
        let mut i = 0;
        while i < bytes.len() {
            let boundary = i == 0 || !token_char(bytes[i - 1]);
            if boundary && let Some((len, class)) = self.token_at(bytes, i) {
                out.push_str(&text[copied..i]);
                out.push_str(&self.mark(class));
                self.count(class);
                i += len;
                copied = i;
                continue;
            }
            if boundary
                && self.high_entropy
                && let Some(len) = high_entropy_at(bytes, i)
            {
                out.push_str(&text[copied..i]);
                out.push_str(&self.mark("high_entropy"));
                self.count("high_entropy");
                i += len;
                copied = i;
                continue;
            }
            if let Some((value_start, value_end, class)) = self.authorization_at(bytes, i) {
                out.push_str(&text[copied..value_start]);
                out.push_str(&self.mark(class));
                self.count(class);
                i = value_end;
                copied = i;
                continue;
            }
            if bytes[i] == b':'
                && i + 2 < bytes.len()
                && bytes[i + 1] == b'/'
                && bytes[i + 2] == b'/'
                && let Some(at) = url_credentials(bytes, i + 3)
            {
                out.push_str(&text[copied..i + 3]);
                out.push_str(&self.mark("url_credentials"));
                self.count("url_credentials");
                i = at;
                copied = i;
                continue;
            }
            if matches!(bytes[i], b'=' | b':')
                && let Some((value_start, value_end)) = assignment_value(bytes, i)
            {
                out.push_str(&text[copied..value_start]);
                out.push_str(&self.mark("assignment"));
                self.count("assignment");
                i = value_end;
                copied = i;
                continue;
            }
            i += 1;
        }
        out.push_str(&text[copied..]);
        out
    }

    fn token_at(&self, bytes: &[u8], i: usize) -> Option<(usize, &'static str)> {
        // JSON Web Tokens: three base64url segments, the first two starting with `eyJ`.
        if bytes[i..].starts_with(b"eyJ") {
            let mut j = i;
            let mut segments = 0;
            let mut seg_len = 0;
            let mut short = false;
            while j < bytes.len() && (token_char(bytes[j]) || bytes[j] == b'.') {
                if bytes[j] == b'.' {
                    segments += 1;
                    short |= seg_len < 8;
                    seg_len = 0;
                } else {
                    seg_len += 1;
                }
                j += 1;
            }
            short |= seg_len < 8;
            if segments == 2 && !short {
                return Some((j - i, "jwt"));
            }
        }
        for (prefix, class, min_body) in PREFIXES {
            let p = prefix.as_bytes();
            if !bytes[i..].starts_with(p) {
                continue;
            }
            let mut j = i + p.len();
            let aws = prefix.starts_with("AKIA") || prefix.starts_with("ASIA");
            while j < bytes.len()
                && if aws {
                    bytes[j].is_ascii_uppercase() || bytes[j].is_ascii_digit()
                } else {
                    token_char(bytes[j])
                }
            {
                j += 1;
            }
            let body = &bytes[i + p.len()..j];
            let mixed =
                body.iter().any(u8::is_ascii_digit) && body.iter().any(u8::is_ascii_alphabetic);
            if body.len() >= *min_body && (mixed || body.len() >= 32) {
                return Some((j - i, class));
            }
            // A longer prefix failing never hides a shorter one (sk-ant- vs sk-).
        }
        None
    }

    fn authorization_at(&self, bytes: &[u8], i: usize) -> Option<(usize, usize, &'static str)> {
        for (scheme, class) in [
            (&b"bearer "[..], "bearer_token"),
            (&b"basic "[..], "basic_auth"),
        ] {
            if bytes.len() < i + scheme.len()
                || !bytes[i..i + scheme.len()].eq_ignore_ascii_case(scheme)
                || (i > 0 && word_char(bytes[i - 1]))
            {
                continue;
            }
            let start = i + scheme.len();
            let mut end = start;
            while end < bytes.len()
                && (token_char(bytes[end])
                    || matches!(bytes[end], b'.' | b'+' | b'/' | b'=' | b'~'))
            {
                end += 1;
            }
            if end - start >= 16 {
                return Some((start, end, class));
            }
        }
        None
    }

    fn home_paths(&mut self, text: &str) -> String {
        let mut value = text.to_owned();
        for form in self.home.clone() {
            let mut out = String::with_capacity(value.len());
            let lower = value.to_ascii_lowercase();
            let needle = form.to_ascii_lowercase();
            let mut copied = 0;
            let mut from = 0;
            while let Some(found) = lower[from..].find(&needle) {
                let at = from + found;
                let after = at + needle.len();
                let next = value.as_bytes().get(after).copied();
                // Only a whole path component: `C:\Users\al` must not match `C:\Users\alex`.
                if next.is_none_or(|c| !word_char(c) && c != b'-') {
                    out.push_str(&value[copied..at]);
                    out.push('~');
                    self.count("home_path");
                    copied = after;
                }
                from = after;
            }
            out.push_str(&value[copied..]);
            value = out;
        }
        value
    }

    fn emails(&mut self, text: &str) -> String {
        if !text.contains('@') {
            return text.to_owned();
        }
        let bytes = text.as_bytes();
        let local_char =
            |c: u8| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'%' | b'+' | b'-');
        let domain_char = |c: u8| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'-');
        let mut out = String::with_capacity(text.len());
        let mut copied = 0;
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] != b'@' {
                i += 1;
                continue;
            }
            let mut start = i;
            while start > copied && local_char(bytes[start - 1]) {
                start -= 1;
            }
            let mut end = i + 1;
            while end < bytes.len() && domain_char(bytes[end]) {
                end += 1;
            }
            while end > i + 1 && matches!(bytes[end - 1], b'.' | b'-') {
                end -= 1;
            }
            let domain = &text[i + 1..end];
            let tld_ok = domain.rsplit_once('.').is_some_and(|(host, tld)| {
                !host.is_empty() && tld.len() >= 2 && tld.bytes().all(|c| c.is_ascii_alphabetic())
            });
            if start < i && tld_ok {
                out.push_str(&text[copied..start]);
                out.push_str(&self.mark("email"));
                self.count("email");
                copied = end;
                i = end;
            } else {
                i += 1;
            }
        }
        out.push_str(&text[copied..]);
        out
    }
}

/// A run of at least 32 base64/url-safe characters with upper, lower and digits and a Shannon
/// entropy of at least 4.3 bits per character. Hex digests (at most 4 bits) and UUIDs never match.
fn high_entropy_at(bytes: &[u8], start: usize) -> Option<usize> {
    let run = |c: u8| c.is_ascii_alphanumeric() || matches!(c, b'+' | b'=' | b'_' | b'-');
    let mut end = start;
    while end < bytes.len() && run(bytes[end]) && end - start < 4096 {
        end += 1;
    }
    let token = &bytes[start..end];
    if token.len() < 32 || (end < bytes.len() && run(bytes[end])) {
        return None;
    }
    if !token.iter().any(u8::is_ascii_uppercase)
        || !token.iter().any(u8::is_ascii_lowercase)
        || token.iter().filter(|c| c.is_ascii_digit()).count() < 2
    {
        return None;
    }
    let mut counts = [0u32; 256];
    for &c in token {
        counts[c as usize] += 1;
    }
    let n = token.len() as f64;
    let entropy: f64 = counts
        .iter()
        .filter(|&&k| k > 0)
        .map(|&k| {
            let p = f64::from(k) / n;
            -p * p.log2()
        })
        .sum();
    (entropy >= 4.3).then_some(token.len())
}

/// `user:password@` right after `://`. Returns the index just past `@`.
fn url_credentials(bytes: &[u8], start: usize) -> Option<usize> {
    let mut j = start;
    let mut colon = None;
    while j < bytes.len() && j - start < 512 {
        match bytes[j] {
            b'@' => {
                let colon = colon?;
                return (colon > start && j > colon + 1).then_some(j);
            }
            b':' if colon.is_none() => colon = Some(j),
            b'/' | b'?' | b'#' | b' ' | b'\n' | b'"' | b'\'' | b'<' | b'>' => return None,
            _ => {}
        }
        j += 1;
    }
    None
}

fn plausible_secret_value(value: &str) -> bool {
    let v = value.trim();
    v.len() >= 8
        && !v
            .get(..9)
            .is_some_and(|p| p.eq_ignore_ascii_case("[redacted"))
        && !v.starts_with('$')
        && !v.starts_with('<')
        && !v.starts_with('{')
        && !v.starts_with('%')
        && !matches!(
            v.to_ascii_lowercase().as_str(),
            "null" | "none" | "true" | "false" | "undefined" | "required" | "optional" | "string"
        )
        && !v
            .chars()
            .all(|c| c == '*' || c == 'x' || c == 'X' || c == '.')
        && v.chars().any(|c| c.is_ascii_alphanumeric())
}

/// `key = value`, `key: value`, `"key": "value"` where key ends with a secret word.
/// `sep` is the index of `=` or `:`. Returns the value's byte range.
fn assignment_value(bytes: &[u8], sep: usize) -> Option<(usize, usize)> {
    // Key: walk back over optional closing quote and spaces, then key characters.
    let mut k = sep;
    while k > 0 && bytes[k - 1] == b' ' {
        k -= 1;
    }
    if k > 0 && matches!(bytes[k - 1], b'"' | b'\'') {
        k -= 1;
    }
    let key_end = k;
    while k > 0 && (word_char(bytes[k - 1]) || bytes[k - 1] == b'-') {
        k -= 1;
    }
    if key_end == k || key_end - k > 64 {
        return None;
    }
    let key = std::str::from_utf8(&bytes[k..key_end])
        .ok()?
        .to_ascii_lowercase();
    if !SECRET_KEYS.iter().any(|s| key.ends_with(s)) {
        return None;
    }
    // `://` belongs to URLs, `::` to paths and code.
    if bytes.get(sep + 1) == Some(&b'/') || bytes.get(sep + 1) == Some(&b':') {
        return None;
    }
    let mut v = sep + 1;
    while v < bytes.len() && bytes[v] == b' ' {
        v += 1;
    }
    let quote = bytes.get(v).copied().filter(|c| matches!(c, b'"' | b'\''));
    if quote.is_some() {
        v += 1;
    }
    let start = v;
    let mut end = v;
    while end < bytes.len() {
        let c = bytes[end];
        let stop = match quote {
            Some(q) => c == q || c == b'\n',
            None => {
                c.is_ascii_whitespace()
                    || matches!(c, b',' | b';' | b'&' | b'"' | b'\'' | b')' | b'}' | b']')
            }
        };
        if stop || (quote.is_some() && c == b'\\') {
            break;
        }
        end += 1;
    }
    let value = std::str::from_utf8(&bytes[start..end]).ok()?;
    plausible_secret_value(value).then_some((start, end))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full(text: &str) -> String {
        Redactor::new(Mode::Full, None).redact(text)
    }

    #[test]
    fn provider_and_forge_tokens_are_redacted_in_both_modes() {
        let key = "sk-ant-api03-AbCdEf0123456789AbCdEf0123456789xyz";
        let text = format!("use {key} now");
        assert_eq!(full(&text), "use [REDACTED:anthropic_key] now");
        let mut portable = Redactor::new(Mode::Portable, Some(r"C:\Users\someone"));
        assert_eq!(portable.redact(&text), "use [REDACTED:anthropic_key] now");
        assert_eq!(
            full("ghp_0123456789abcdefABCDEF0123456789abcd"),
            "[REDACTED:github_token]"
        );
        assert_eq!(full("AKIAIOSFODNN7EXAMPLE"), "[REDACTED:aws_access_key]");
        assert_eq!(
            full("token xoxb-1234567890-abcdefghij"),
            "token [REDACTED:slack_token]"
        );
    }

    #[test]
    fn ordinary_words_and_short_values_survive() {
        for text in [
            "ask-me later",
            "the sk-ip flag",
            "max_tokens: 4096",
            "tokens_before = 12000",
            "password: required",
            "set api_key = $OPENAI_API_KEY",
            "see https://example.com/a:b",
            "std::path::Path",
            "AKIA is a prefix",
        ] {
            assert_eq!(full(text), text, "{text}");
        }
    }

    #[test]
    fn jwt_bearer_basic_and_url_credentials() {
        let jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U";
        assert_eq!(full(&format!("t={jwt}")), "t=[REDACTED:jwt]");
        assert_eq!(
            full("Authorization: Bearer abcdefghijklmnop0123456789"),
            "Authorization: Bearer [REDACTED:bearer_token]"
        );
        assert_eq!(
            full("git clone https://user:hunter2secret@github.com/x/y"),
            "git clone https://[REDACTED:url_credentials]@github.com/x/y"
        );
    }

    #[test]
    fn assignments_in_env_json_and_yaml() {
        assert_eq!(
            full("DB_PASSWORD=correct-horse-battery"),
            "DB_PASSWORD=[REDACTED:assignment]"
        );
        assert_eq!(
            full(r#"{"client_secret": "abc123def456ghi"}"#),
            r#"{"client_secret": "[REDACTED:assignment]"}"#
        );
        assert_eq!(
            full("api-key: 9f8e7d6c5b4a3210"),
            "api-key: [REDACTED:assignment]"
        );
    }

    #[test]
    fn private_key_blocks_are_removed_whole() {
        let pem = "before\n-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXktdjEAAAA\n-----END OPENSSH PRIVATE KEY-----\nafter";
        assert_eq!(full(pem), "before\n[REDACTED:private_key]\nafter");
        let public = "-----BEGIN CERTIFICATE-----\nMIIB\n-----END CERTIFICATE-----";
        assert_eq!(full(public), public);
        let torn = "x -----BEGIN RSA PRIVATE KEY-----\nMIIEow";
        assert_eq!(full(torn), "x [REDACTED:private_key]");
    }

    #[test]
    fn portable_removes_home_and_email_but_full_keeps_them() {
        let text = r"open C:\Users\Someone\proj and c:/users/someone/x, mail me@example.org; C:\Users\Someone2 stays";
        assert_eq!(full(text), text);
        let mut redactor = Redactor::new(Mode::Portable, Some(r"C:\Users\Someone"));
        assert_eq!(
            redactor.redact(text),
            r"open ~\proj and ~/x, mail [REDACTED:email]; C:\Users\Someone2 stays"
        );
        assert_eq!(redactor.counts().get("home_path"), Some(&2));
        assert_eq!(redactor.counts().get("email"), Some(&1));
    }

    #[test]
    fn redaction_is_idempotent_and_json_aware() {
        let once = full("key sk-proj-abcdefghij0123456789ABCDEFGHIJ end");
        assert_eq!(full(&once), once);
        let mut value = serde_json::json!({"env": {"GITHUB_TOKEN": "plainvalue123", "PATH": "C:/bin"},
            "args": ["--token", "ghp_0123456789abcdefABCDEF0123456789abcd"]});
        let mut redactor = Redactor::new(Mode::Full, None);
        redactor.redact_json(&mut value);
        assert_eq!(value["env"]["GITHUB_TOKEN"], "[REDACTED:assignment]");
        assert_eq!(value["env"]["PATH"], "C:/bin");
        assert_eq!(value["args"][1], "[REDACTED:github_token]");
    }

    #[test]
    fn seed_rules_use_lowercase_markers_and_catch_high_entropy_tokens() {
        let mut seed = Redactor::for_seed();
        assert_eq!(
            seed.redact("k ghp_0123456789abcdefABCDEF0123456789abcd"),
            "k [redacted:github_token]"
        );
        let opaque = "Zx9QvR2mT8kLp4WnB7yHc3JdF6sGa1Ue5VtK0rYqXoNi";
        assert_eq!(
            seed.redact(&format!("secret is {opaque} ok")),
            "secret is [redacted:high_entropy] ok"
        );
        for benign in [
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            "550e8400-e29b-41d4-a716-446655440000",
            "ThisIsAVeryLongCamelCaseIdentifierNameWithoutDigits",
            "src/components/session/detail/renderer_component.ts",
            "very_long_snake_case_identifier_used_in_rust_code_42",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa1B2",
        ] {
            assert_eq!(seed.redact(benign), benign, "{benign}");
        }
        assert_eq!(seed.redact("[REDACTED:jwt] stays"), "[REDACTED:jwt] stays");
        let mut export = Redactor::new(Mode::Full, None);
        assert_eq!(
            export.redact(&format!("x {opaque}")),
            format!("x {opaque}"),
            "exports keep the rule off"
        );
    }

    #[test]
    fn multibyte_text_is_preserved() {
        let text = "🦀 città — password=ünïcødé-sëcret ok";
        assert_eq!(full(text), "🦀 città — password=[REDACTED:assignment] ok");
    }
}
