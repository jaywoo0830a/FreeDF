//! 사전 데이터 모델 — 여러 사전 API 응답을 **하나의 공통 형식**으로 정규화하고
//! 오버레이에 표시할 평문으로 포맷합니다.
//!
//! 네트워크 조회는 앱(dictionary.rs)의 프로바이더가 담당하고, 이 모듈은
//! **순수 변환**(JSON → 항목 → 텍스트)만 담아 GUI/네트워크 없이 단위 테스트로
//! 검증합니다. 새 API를 추가하려면 앱에서 `parse_*` 함수 하나만 더 만들면 됩니다.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 사전 정의 하나.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Definition {
    /// 품사 (예: "noun", "verb"). 없으면 빈 문자열.
    pub part_of_speech: String,
    pub text: String,
}

impl Definition {
    pub fn new(part_of_speech: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            part_of_speech: part_of_speech.into(),
            text: text.into(),
        }
    }
}

/// 한 단어의 사전 조회 결과 (API 공통 형식).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DictionaryEntry {
    pub word: String,
    /// 발음 기호 (없으면 빈 문자열).
    pub phonetic: String,
    pub definitions: Vec<Definition>,
}

impl DictionaryEntry {
    pub fn new(word: impl Into<String>) -> Self {
        Self {
            word: word.into(),
            phonetic: String::new(),
            definitions: Vec::new(),
        }
    }

    /// DB 캐시(JSONB)에 저장된 값에서 복원합니다. 형식이 다르면 `None`.
    pub fn from_value(v: &Value) -> Option<Self> {
        serde_json::from_value(v.clone()).ok()
    }

    /// DB 캐시에 저장할 JSON 값.
    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }

    /// 오버레이에 표시할 평문 (최대 5개 정의).
    pub fn format(&self, fallback_word: &str) -> String {
        let w = if self.word.is_empty() {
            fallback_word
        } else {
            &self.word
        };
        if self.definitions.is_empty() {
            return "No definition found.".to_string();
        }
        let mut out = String::new();
        let ph = self.phonetic.trim_matches('/');
        if ph.is_empty() {
            out.push_str(&format!("{w}\n\n"));
        } else {
            out.push_str(&format!("{w}  /{ph}/\n\n"));
        }
        for (i, d) in self.definitions.iter().take(5).enumerate() {
            if i > 0 {
                out.push('\n');
            }
            if d.part_of_speech.is_empty() {
                out.push_str(&format!("• {}", d.text));
            } else {
                out.push_str(&format!("• [{}] {}", d.part_of_speech, d.text));
            }
        }
        out
    }
}

/// dictionaryapi.dev (`https://api.dictionaryapi.dev/api/v2/entries/en/{word}`)
/// 응답을 정규화합니다.
pub fn parse_dictionaryapi_dev(v: &Value) -> DictionaryEntry {
    let mut entry = DictionaryEntry::new("");
    let Some(items) = v.as_array() else {
        return entry;
    };
    for e in items {
        let w = e
            .get("word")
            .and_then(|w| w.as_str())
            .filter(|s| !s.is_empty());
        if let Some(w) = w {
            if entry.word.is_empty() {
                entry.word = w.to_string();
            }
        }
        if entry.phonetic.is_empty() {
            if let Some(p) = e.get("phonetic").and_then(|p| p.as_str()) {
                entry.phonetic = p.to_string();
            } else if let Some(phonetics) = e.get("phonetics").and_then(|p| p.as_array()) {
                for ph in phonetics {
                    if let Some(t) = ph.get("text").and_then(|t| t.as_str()) {
                        if !t.is_empty() {
                            entry.phonetic = t.to_string();
                            break;
                        }
                    }
                }
            }
        }
        let meanings = e.get("meanings").and_then(|m| m.as_array());
        if let Some(meanings) = meanings {
            for m in meanings {
                let pos = m
                    .get("partOfSpeech")
                    .and_then(|p| p.as_str())
                    .unwrap_or("");
                if let Some(defs) = m.get("definitions").and_then(|d| d.as_array()) {
                    for d in defs {
                        if let Some(def) = d.get("definition").and_then(|d| d.as_str()) {
                            entry.definitions.push(Definition::new(pos, def));
                        }
                    }
                }
            }
        }
    }
    entry
}

/// Wiktionary REST (`https://en.wiktionary.org/api/rest_v1/page/definition/{word}`)
/// 응답을 정규화합니다.
///
/// 응답 형태: `{ "en": [ { "partOfSpeech": "Noun", "language": "English",
/// "definitions": [ { "definition": "...", "examples": [...] } ] } ] }`
pub fn parse_wiktionary(v: &Value) -> DictionaryEntry {
    let mut entry = DictionaryEntry::new("");
    let Some(obj) = v.as_object() else {
        return entry;
    };
    // 첫 번째 언어 키(보통 "en")의 항목들을 사용.
    let Some(lang_items) = obj.values().find_map(|v| v.as_array()) else {
        return entry;
    };
    for item in lang_items {
        if entry.word.is_empty() {
            if let Some(w) = item.get("word").and_then(|w| w.as_str()) {
                entry.word = w.to_string();
            }
        }
        let pos = item
            .get("partOfSpeech")
            .and_then(|p| p.as_str())
            .unwrap_or("");
        if let Some(defs) = item.get("definitions").and_then(|d| d.as_array()) {
            for d in defs {
                if let Some(def) = d.get("definition").and_then(|d| d.as_str()) {
                    // 위키 마크업을 가볍게 정리 (평문에 가깝게).
                    let text = strip_wiki_markup(def);
                    entry.definitions.push(Definition::new(pos, text));
                }
            }
        }
    }
    entry
}

/// Datamuse (`https://api.datamuse.com/words?sp={word}&md=d&max=1`) 응답을
/// 정규화합니다.
///
/// 응답 형태: `[ { "word": "...", "defs": ["n\tdefinition", ...] } ]`
/// (`defs`는 "품사\t정의" 형식).
pub fn parse_datamuse(v: &Value) -> DictionaryEntry {
    let mut entry = DictionaryEntry::new("");
    let Some(items) = v.as_array() else {
        return entry;
    };
    for item in items {
        if entry.word.is_empty() {
            if let Some(w) = item.get("word").and_then(|w| w.as_str()) {
                entry.word = w.to_string();
            }
        }
        if let Some(defs) = item.get("defs").and_then(|d| d.as_array()) {
            for d in defs {
                if let Some(s) = d.as_str() {
                    let (pos, text) = match s.split_once('\t') {
                        Some((p, t)) => (p.to_string(), t.to_string()),
                        None => (String::new(), s.to_string()),
                    };
                    entry.definitions.push(Definition::new(pos, text));
                }
            }
        }
    }
    entry
}

/// 위키 정의에서 마크업/HTML/참조를 제거한 짧은 평문을 만듭니다.
fn strip_wiki_markup(s: &str) -> String {
    // 정의 뒤 참조/용례 템플릿({{...}})부터는 버립니다.
    let s = match s.find("{{") {
        Some(i) => &s[..i],
        None => s,
    };
    // HTML 태그 제거 (Wiktionary는 <a>/<span> 등 HTML을 섞어 반환).
    let mut no_tags = String::new();
    {
        let mut in_tag = false;
        for c in s.chars() {
            match c {
                '<' => in_tag = true,
                '>' => in_tag = false,
                c if !in_tag => no_tags.push(c),
                _ => {}
            }
        }
    }
    // [[target|label]] / [url label] 처리.
    let chars: Vec<char> = no_tags.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '\'' => { /* 굵기/기울임 마크 */ }
            '[' => {
                if i + 1 < chars.len() && chars[i + 1] == '[' {
                    // [[target|label]] → label (파이프 없으면 target).
                    let mut j = i + 2;
                    let mut content = String::new();
                    while j < chars.len() && !(chars[j] == ']' && chars.get(j + 1) == Some(&']')) {
                        content.push(chars[j]);
                        j += 1;
                    }
                    if let Some(label) = content.rsplit('|').next() {
                        out.push_str(label);
                    }
                    i = (j + 2).min(chars.len());
                    continue;
                } else {
                    // [url label] → label (마지막 토큰).
                    let mut j = i + 1;
                    let mut content = String::new();
                    while j < chars.len() && chars[j] != ']' {
                        content.push(chars[j]);
                        j += 1;
                    }
                    if let Some(label) = content.rsplit(' ').next() {
                        out.push_str(label);
                    }
                    i = (j + 1).min(chars.len());
                    continue;
                }
            }
            c => out.push(c),
        }
        i += 1;
    }
    // 엔티티 디코드 + 공백 정리.
    let decoded = decode_entities(&out);
    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// HTML 엔티티를 평문 문자로 복원합니다.
fn decode_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&nbsp;", " ")
        .replace("&ndash;", "–")
        .replace("&mdash;", "—")
        .replace("&hellip;", "…")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_dictionaryapi_dev_extracts_entries() {
        let v = json!([
            {
                "word": "hello",
                "phonetic": "/həˈləʊ/",
                "meanings": [
                    {
                        "partOfSpeech": "noun",
                        "definitions": [
                            { "definition": "A greeting." },
                            { "definition": "An exclamation." }
                        ]
                    },
                    {
                        "partOfSpeech": "interjection",
                        "definitions": [ { "definition": "Said in greeting." } ]
                    }
                ]
            }
        ]);
        let e = parse_dictionaryapi_dev(&v);
        assert_eq!(e.word, "hello");
        assert_eq!(e.phonetic, "/həˈləʊ/");
        assert_eq!(e.definitions.len(), 3);
        assert_eq!(e.definitions[0], Definition::new("noun", "A greeting."));
        assert_eq!(e.definitions[2], Definition::new("interjection", "Said in greeting."));
    }

    #[test]
    fn parse_dictionaryapi_dev_phonetics_array_fallback() {
        let v = json!([
            {
                "word": "test",
                "phonetics": [ { "text": "/tɛst/" } ],
                "meanings": []
            }
        ]);
        let e = parse_dictionaryapi_dev(&v);
        assert_eq!(e.phonetic, "/tɛst/");
    }

    #[test]
    fn parse_wiktionary_extracts_definitions() {
        let v = json!({
            "en": [
                {
                    "partOfSpeech": "Noun",
                    "language": "English",
                    "definitions": [
                        {
                            "definition": "A [[word]] spoken to greet someone.",
                            "examples": []
                        },
                        {
                            "definition": "An expression of greeting. {{gloss|rare}}",
                            "examples": []
                        }
                    ]
                }
            ]
        });
        let e = parse_wiktionary(&v);
        assert_eq!(e.definitions.len(), 2);
        assert_eq!(e.definitions[0].part_of_speech, "Noun");
        assert_eq!(e.definitions[0].text, "A word spoken to greet someone.");
        // 템플릿부터 잘림.
        assert_eq!(e.definitions[1].text, "An expression of greeting.");
    }

    #[test]
    fn parse_wiktionary_strips_html_and_entities() {
        // 실제 en.wiktionary.org REST 응답 형태 (HTML 링크/스팬 포함).
        let v = json!({
            "en": [
                {
                    "partOfSpeech": "Interjection",
                    "language": "English",
                    "definitions": [
                        {
                            "definition": "<span class=\"use-with-mention\">A <a rel=\"mw:WikiLink\" href=\"/wiki/greeting#English\" title=\"greeting\">greeting</a> said when <a rel=\"mw:WikiLink\" href=\"/wiki/meet#English\" title=\"meet\">meeting</a> someone &amp; acknowledging their arrival.</span>",
                            "examples": []
                        }
                    ]
                }
            ]
        });
        let e = parse_wiktionary(&v);
        assert_eq!(e.definitions.len(), 1);
        assert_eq!(
            e.definitions[0].text,
            "A greeting said when meeting someone & acknowledging their arrival."
        );
    }

    #[test]
    fn parse_datamuse_extracts_defs() {
        let v = json!([ { "word": "hello", "defs": ["n\ta greeting"] } ]);
        let e = parse_datamuse(&v);
        assert_eq!(e.word, "hello");
        assert_eq!(e.definitions, vec![Definition::new("n", "a greeting")]);
    }

    #[test]
    fn empty_responses_yield_no_definitions() {
        assert!(parse_dictionaryapi_dev(&json!(null)).definitions.is_empty());
        assert!(parse_wiktionary(&json!({})).definitions.is_empty());
        assert!(parse_datamuse(&json!([])).definitions.is_empty());
    }

    #[test]
    fn entry_format_and_value_round_trip() {
        let mut e = DictionaryEntry::new("hello");
        e.phonetic = "/həˈləʊ/".to_string();
        e.definitions = vec![
            Definition::new("noun", "A greeting."),
            Definition::new("interjection", "Said in greeting."),
        ];
        let text = e.format("hello");
        assert!(text.starts_with("hello  /həˈləʊ/"));
        assert!(text.contains("• [noun] A greeting."));
        // DB 캐시 왕복.
        let v = e.to_value();
        assert_eq!(DictionaryEntry::from_value(&v), Some(e.clone()));
        // 캐시 형식이 다르면(이전 버전 원시 응답) None.
        assert!(DictionaryEntry::from_value(&json!([{ "word": "x", "meanings": [] }])).is_none());
    }
}
