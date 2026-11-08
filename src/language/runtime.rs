//! Pure language decisions. Source text is never rewritten or sent to a
//! separate detection service. The resulting policy is copied into a bundle.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ResponsePolicy {
    pub mode: String,
    #[serde(default = "legacy_locale")]
    pub locale: String,
    #[serde(default = "legacy_source")]
    pub source: String,
    #[serde(default = "legacy_locale")]
    pub fallback_locale: String,
    #[serde(default)]
    pub requested_locale: Option<String>,
    #[serde(default)]
    pub locale_resolution: Option<String>,
}

fn legacy_locale() -> String {
    "zh-Hans".into()
}
fn legacy_source() -> String {
    "legacy".into()
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct BundleLanguage {
    pub ui_locale: String,
    pub instruction_locale: String,
    pub background_instruction_locale: String,
    pub response: ResponsePolicy,
    pub knowledge_locale: String,
    pub knowledge_policy_version: i64,
    pub knowledge_policy_generation: i64,
}

/// Only unsupported language tags fall back; script distinctions stay explicit.
pub(crate) fn supported_locale(raw: &str) -> &'static str {
    match raw {
        "en" => "en",
        _ if raw.starts_with("en-") => "en",
        _ => "zh-Hans",
    }
}

// Quoted examples and code are evidence, never language-selection instructions.
pub(crate) fn prose(text: &str) -> String {
    let mut fenced = false;
    let mut result = String::new();
    for line in text.lines() {
        if line.starts_with("    ") || line.starts_with('\t') {
            continue;
        }
        let line = line.trim();
        if line.starts_with("```") || line.starts_with("~~~") {
            fenced = !fenced;
            continue;
        }
        if fenced || line.starts_with('>') {
            continue;
        }
        let mut quote = None;
        let mut inline_code = false;
        let mut previous = ' ';
        for ch in line.chars() {
            if ch == '`' {
                inline_code = !inline_code;
                result.push(' ');
                continue;
            }
            if inline_code {
                continue;
            }
            if let Some(end) = quote {
                if ch == end {
                    quote = None;
                    result.push(' ');
                }
                continue;
            }
            quote = match ch {
                '\'' if !previous.is_alphanumeric() => Some('\''),
                '"' => Some('"'),
                '“' => Some('”'),
                '「' => Some('」'),
                '『' => Some('』'),
                _ => None,
            };
            if quote.is_none() {
                result.push(ch);
            }
            previous = ch;
        }
        result.push('\n');
    }
    result
        .split_whitespace()
        .filter(|word| !word.contains("://"))
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn explicit_locale(text: &str) -> Option<&'static str> {
    let text = prose(text);
    let mut selected = None;
    for clause in text.split(['.', '!', '?', '。', '！', '？', ';', '；', '\n']) {
        // An ambiguous/negated instruction must not be recorded as a detected
        // request. The actual message is still present for the model to read.
        if [
            "don't",
            "do not",
            "not in",
            "不要",
            "别用",
            "不用",
            "不需要",
            "是否",
            "whether",
            "what does",
            "the phrase",
            "the sentence",
            "how to",
            "how would",
            "how do",
        ]
        .iter()
        .any(|n| clause.contains(n))
        {
            continue;
        }
        for (locale, patterns) in [
            (
                "en",
                &[
                    "answer in english",
                    "reply in english",
                    "respond in english",
                    "用英文回答",
                    "用英语回答",
                    "以英文回答",
                    "用英文回复",
                    "用英语回复",
                    "请用英文",
                    "请用英语",
                ] as &[&str],
            ),
            (
                "zh-Hans",
                &[
                    "answer in chinese",
                    "reply in chinese",
                    "respond in chinese",
                    "用中文回答",
                    "用汉语回答",
                    "以中文回答",
                    "用中文回复",
                    "请用中文",
                ] as &[&str],
            ),
        ] {
            if patterns.iter().any(|p| clause.contains(p)) {
                selected = Some(locale);
            }
        }
    }
    selected
}

fn reliable_locale(text: &str) -> Option<&'static str> {
    let text = prose(text);
    let han = text
        .chars()
        .filter(|c| ('\u{3400}'..='\u{9fff}').contains(c))
        .count();
    let words = text
        .split(|c: char| !c.is_ascii_alphabetic())
        .filter(|w| w.len() >= 2)
        .collect::<Vec<_>>();
    let latin: usize = words.iter().map(|w| w.len()).sum();
    if han >= 4 && (han * 2 >= latin || words.len() < 4) {
        Some("zh-Hans")
    } else if words.len() >= 4 && latin >= 12 && han < 4 {
        Some("en")
    } else {
        None
    }
}

pub(crate) fn response_policy(
    mode: &str,
    fixed: Option<&str>,
    current: &str,
    previous_user_messages: &[String],
    fallback: &str,
) -> ResponsePolicy {
    let fallback = supported_locale(fallback);
    let (locale, source) = if let Some(locale) = explicit_locale(current) {
        (locale, "explicit_request")
    } else if mode == "fixed" {
        (
            supported_locale(fixed.unwrap_or(fallback)),
            "fixed_preference",
        )
    } else if let Some(locale) = reliable_locale(current) {
        (locale, "current_message")
    } else if let Some(locale) = previous_user_messages
        .iter()
        .rev()
        .take(12)
        .find_map(|s| explicit_locale(s).or_else(|| reliable_locale(s)))
    {
        (locale, "conversation")
    } else {
        (fallback, "fallback")
    };
    let requested = if source == "fixed_preference" {
        fixed.unwrap_or(fallback)
    } else {
        locale
    };
    ResponsePolicy {
        mode: mode.into(),
        locale: locale.into(),
        source: source.into(),
        fallback_locale: fallback.into(),
        requested_locale: Some(requested.into()),
        locale_resolution: (requested != locale).then(|| {
            format!(
                "{}:{requested}:{locale}",
                if requested.starts_with("en-") {
                    "parent_locale"
                } else {
                    "missing_translation"
                }
            )
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_response_uses_user_language_but_short_followups_inherit() {
        let english = vec!["Help me decide how to approach this project.".into()];
        for short in [
            "?",
            "continue",
            "ok",
            "继续",
            "```rust\nfn main() {}\n```",
            "> 请用中文回答",
        ] {
            let p = response_policy("auto", None, short, &english, "zh-Hans");
            assert_eq!(p.locale, "en", "{short}");
            assert_eq!(p.source, "conversation");
        }
        assert_eq!(
            response_policy("auto", None, "请帮我分析这个计划", &english, "en").locale,
            "zh-Hans"
        );
        assert_eq!(
            response_policy(
                "auto",
                None,
                "Explain the constraints of this plan.",
                &[],
                "zh-Hans"
            )
            .locale,
            "en"
        );
        let p = response_policy("auto", None, "?", &[], "en-GB");
        assert_eq!((p.locale.as_str(), p.source.as_str()), ("en", "fallback"));
    }

    #[test]
    fn explicit_request_beats_fixed_preference_without_treating_quotes_as_requests() {
        let p = response_policy(
            "fixed",
            Some("zh-Hans"),
            "Please reply in English.",
            &[],
            "zh-Hans",
        );
        assert_eq!(
            (p.locale.as_str(), p.source.as_str()),
            ("en", "explicit_request")
        );
        for quoted in [
            "Translate the phrase “reply in English”.",
            "`reply in English`",
            "> reply in English",
            "Do not reply in English.",
            "不要用英文回答",
            "Translate the phrase 'reply in English'.",
            "    reply in English",
            "What does reply in English mean?",
        ] {
            let p = response_policy("fixed", Some("zh-Hans"), quoted, &[], "en");
            assert_eq!(p.locale, "zh-Hans", "{quoted}");
            assert_eq!(p.source, "fixed_preference");
        }
        assert_eq!(
            response_policy("fixed", Some("en"), "请用中文回答", &[], "en").locale,
            "zh-Hans"
        );
    }

    #[test]
    fn unsupported_and_regional_fixed_preferences_record_requested_and_actual_locales() {
        let p = response_policy("fixed", Some("fr"), "?", &[], "en");
        assert_eq!(p.locale, "zh-Hans");
        assert_eq!(p.requested_locale.as_deref(), Some("fr"));
        assert_eq!(
            p.locale_resolution.as_deref(),
            Some("missing_translation:fr:zh-Hans")
        );
        let p = response_policy("fixed", Some("en-GB"), "?", &[], "zh-Hans");
        assert_eq!(p.locale, "en");
        assert_eq!(
            p.locale_resolution.as_deref(),
            Some("parent_locale:en-GB:en")
        );
    }
}
