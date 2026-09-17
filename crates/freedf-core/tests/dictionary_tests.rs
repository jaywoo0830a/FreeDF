//! `dictionary` 모듈 단위 테스트 — `src/dictionary.rs`의 `#[cfg(test)]` 모듈에서 이동했습니다.

use freedf_core::dictionary::*;
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
