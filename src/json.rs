use std::borrow::Cow;

pub fn escape_json(input: &str) -> Cow<'_, str> {
    let needs_escape = input.chars().any(|ch| {
        matches!(
            ch,
            '"' | '\\' | '\x08' | '\x09' | '\x0A' | '\x0C' | '\x0D'
                | '\u{0000}'..='\u{001F}' | '\u{007F}' | '\u{2028}' | '\u{2029}'
                | '\u{10000}'..

        )
    });

    if !needs_escape {
        return Cow::Borrowed(input);
    }

    let mut output = String::with_capacity(input.len() + input.len().div_ceil(5));

    for ch in input.chars() {
        match ch {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\x08' => output.push_str("\\b"),
            '\x09' => output.push_str("\\t"),
            '\x0A' => output.push_str("\\n"),
            '\x0C' => output.push_str("\\f"),
            '\x0D' => output.push_str("\\r"),
            '\u{0000}'..='\u{001F}' | '\u{007F}' | '\u{2028}' | '\u{2029}' => {
                output.push_str(&format!("\\u{:04X}", ch as u32))
            }
            ch if ch > '\u{7F}' => {
                for c in ch.encode_utf16(&mut [0; 2]) {
                    output.push_str(&format!("\\u{:04X}", c));
                }
            }
            _ => output.push(ch),
        }
    }

    Cow::Owned(output)
}
