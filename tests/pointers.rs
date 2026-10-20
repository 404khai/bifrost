use bifrost::{Pointer, PointerError, Trie};

fn pointer(text: &str) -> Pointer {
    Pointer::parse(text).unwrap()
}

#[test]
fn root_empty_tokens_and_nested_paths_remain_distinct() {
    for (text, expected) in [
        ("", vec![]),
        ("/", vec![""]),
        ("//", vec!["", ""]),
        ("/session/terminal", vec!["session", "terminal"]),
        ("/a//b/", vec!["a", "", "b", ""]),
    ] {
        let parsed = pointer(text);
        assert_eq!(parsed.tokens(), expected);
        assert_eq!(parsed.to_string(), text);
    }
}

#[test]
fn escapes_decode_once_and_preserve_unicode_and_literal_characters() {
    for (text, expected) in [
        ("/a~1b/m~0n", vec!["a/b", "m~n"]),
        ("/~01/~10/~00", vec!["~1", "/0", "~0"]),
        ("/🦀/é/é", vec!["🦀", "é", "é"]),
        ("/a%20b/#/ /\u{0}", vec!["a%20b", "#", " ", "\u{0}"]),
        (r#"/quote"/back\slash"#, vec!["quote\"", r"back\slash"]),
    ] {
        let parsed = pointer(text);
        assert_eq!(parsed.tokens(), expected);
        assert_eq!(parsed.to_string(), text);
    }
}

#[test]
fn malformed_syntax_and_fragments_are_rejected() {
    for text in ["session", "#/session", "#", " /a"] {
        assert_eq!(Pointer::parse(text), Err(PointerError::MissingLeadingSlash));
    }
    for (text, index) in [
        ("/~", 0),
        ("/~2", 0),
        ("/~x", 0),
        ("/a/b~", 1),
        ("/~00~2", 0),
        ("/é~🦀", 0),
    ] {
        assert_eq!(
            Pointer::parse(text),
            Err(PointerError::InvalidEscape { token_index: index })
        );
    }
    assert!(
        PointerError::InvalidEscape { token_index: 1 }
            .to_string()
            .contains("token 1")
    );
}

#[test]
fn numeric_tokens_are_not_evaluated_as_array_indices() {
    let path = pointer("/items/0/01/-/999999999999999999999999999999");
    assert_eq!(
        path.tokens(),
        ["items", "0", "01", "-", "999999999999999999999999999999"]
    );
    let mut trie = Trie::new();
    trie.insert(&path, 7);
    assert_eq!(trie.get(&path), Some(&7));
    assert!(trie.has_descendant(&pointer("/items/0")));
}

#[test]
fn trie_exact_lookup_and_descendants_have_different_meanings() {
    let mut trie = Trie::new();
    assert!(!trie.has_rules_at_or_below(&pointer("")));
    assert!(!trie.has_descendant(&pointer("")));
    trie.insert(&pointer("/session/terminal"), 1);
    trie.insert(&pointer("/session/windows"), 2);
    trie.insert(&pointer("/sessions"), 3);
    assert_eq!(trie.get(&pointer("/session")), None);
    assert!(trie.has_descendant(&pointer("/session")));
    assert!(trie.has_rules_at_or_below(&pointer("/session")));
    assert_eq!(trie.get(&pointer("/session/terminal")), Some(&1));
    assert!(!trie.has_descendant(&pointer("/session/terminal")));
    assert!(trie.has_rules_at_or_below(&pointer("/session/terminal")));
    for text in [
        "/sess",
        "/missing",
        "/session/missing",
        "/session/terminal/deeper",
    ] {
        assert_eq!(trie.get(&pointer(text)), None);
        assert!(!trie.has_descendant(&pointer(text)));
        assert!(!trie.has_rules_at_or_below(&pointer(text)));
    }
}

#[test]
fn trie_supports_root_empty_and_escaped_tokens_and_explicit_replacement() {
    let mut trie = Trie::new();
    assert_eq!(trie.insert(&pointer(""), "root"), None);
    assert!(!trie.has_descendant(&pointer("")));
    assert!(trie.has_rules_at_or_below(&pointer("")));
    trie.insert(&pointer("/"), "empty");
    trie.insert(&pointer("/a~1b/~0"), "escaped");
    assert!(trie.has_descendant(&pointer("")));
    assert_eq!(trie.get(&pointer("")), Some(&"root"));
    assert_eq!(trie.get(&pointer("/")), Some(&"empty"));
    assert_eq!(trie.get(&pointer("/a~1b/~0")), Some(&"escaped"));
    assert_eq!(trie.get(&pointer("/a/b/~0")), None);
    assert_eq!(trie.insert(&pointer("/"), "replacement"), Some("empty"));
    assert_eq!(trie.get(&pointer("/")), Some(&"replacement"));
}
