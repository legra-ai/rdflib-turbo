use super::unicode;
use std::net::Ipv6Addr;

pub(super) fn language(text: &str) -> bool {
    let mut parts = text.split('-');
    let Some(first) = parts.next() else {
        return false;
    };
    !first.is_empty()
        && first.len() <= 8
        && first.bytes().all(|c| c.is_ascii_alphabetic())
        && parts.all(|part| {
            !part.is_empty() && part.len() <= 8 && part.bytes().all(|c| c.is_ascii_alphanumeric())
        })
}

pub(super) fn name(text: &str, kind: &str) -> bool {
    let Some(first) = text.chars().next() else {
        return false;
    };
    if kind != "NMTOKEN" && (first.is_ascii_digit() || matches!(first, '.' | '-')) {
        return false;
    }
    text.chars()
        .all(|ch| unicode::word(ch) || matches!(ch, '.' | '-') || (ch == ':' && kind != "NCName"))
}

pub(super) fn normalized(text: &str) -> bool {
    !text.is_empty() && !text.contains(['\n', '\t', '\r'])
}

pub(super) fn token(text: &str) -> bool {
    if text.is_empty() {
        return true;
    }
    if !normalized(text) {
        return false;
    }
    let mut previous_space = true;
    for ch in text.chars() {
        let space = unicode::whitespace(ch);
        if space && previous_space {
            return false;
        }
        previous_space = space;
    }
    !previous_space
}

pub(super) fn plain(text: &str) -> bool {
    let prefix = text.split('\n').next().unwrap_or("");
    let Some(at) = prefix.rfind('@') else {
        return false;
    };
    let tag = text[at + 1..].split('@').next().unwrap_or("");
    tag.is_empty() || language(tag)
}

pub(super) fn base64(text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    let mut position = 0;
    let mut padding = 0;
    // Check the whole lexical form before the decoder's early padding return.
    if !text
        .bytes()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'+' | b'/' | b'='))
    {
        return false;
    }
    for byte in text.bytes() {
        if byte == b'=' {
            if position >= 2 {
                padding += 1;
                if position + padding >= 4 {
                    return true;
                }
            }
        } else {
            padding = 0;
            position = (position + 1) % 4;
        }
    }
    position == 0
}

fn host_valid(host: &str) -> bool {
    // These compatibility characters introduce URL delimiters under NFKC.
    // bounded: the fixed Unicode normalization preimages of /?#@:.
    if host.chars().any(|ch| {
        matches!(
            ch,
            '\u{2047}'
                ..='\u{2049}'
                    | '\u{2100}'
                    | '\u{2101}'
                    | '\u{2105}'
                    | '\u{2106}'
                    | '\u{2a74}'
                    | '\u{fe13}'
                    | '\u{fe16}'
                    | '\u{fe55}'
                    | '\u{fe56}'
                    | '\u{fe5f}'
                    | '\u{fe6b}'
                    | '\u{ff03}'
                    | '\u{ff0f}'
                    | '\u{ff1a}'
                    | '\u{ff1f}'
                    | '\u{ff20}'
        )
    }) {
        return false;
    }
    match (host.find('['), host.find(']')) {
        (None, None) => true,
        (Some(start), Some(end)) if start < end => {
            let address = &host[start + 1..end];
            if let Some(future) = address.strip_prefix('v') {
                let Some((version, body)) = future.split_once('.') else {
                    return false;
                };
                !version.is_empty()
                    && version.bytes().all(|c| c.is_ascii_hexdigit())
                    && !body.is_empty()
            } else {
                let (address, scope) = address
                    .split_once('%')
                    .map_or((address, None), |(address, scope)| (address, Some(scope)));
                scope.is_none_or(|scope| !scope.is_empty() && !scope.contains('%'))
                    && address.parse::<Ipv6Addr>().is_ok()
            }
        }
        _ => false,
    }
}

pub(super) fn any_uri(text: &str) -> bool {
    if text.is_empty() {
        return true;
    }
    // urlsplit strips leading C0 controls and removes these ASCII tabs/newlines.
    let stripped: String = text
        .trim_start_matches(|c: char| c <= ' ')
        .chars()
        .filter(|c| !matches!(c, '\r' | '\n' | '\t'))
        .collect(); // bounded: one scalar lexical value.
    let scheme = stripped.find(':').filter(|&index| {
        let value = &stripped[..index];
        value
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphabetic)
            && value
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'+' | b'-' | b'.'))
    });
    let rest = scheme.map_or(stripped.as_str(), |index| &stripped[index + 1..]);
    if let Some(authority) = rest.strip_prefix("//") {
        let host = authority.split(['/', '?', '#']).next().unwrap_or("");
        if !host_valid(host) {
            return false;
        }
    }
    if scheme.is_some() {
        return true;
    }
    if text.starts_with('%') {
        text.as_bytes()
            .get(1..3)
            .is_some_and(|bytes| bytes.iter().all(u8::is_ascii_hexdigit))
    } else {
        !text.starts_with(['?', ':'])
    }
}

pub(super) fn xml(text: &str) -> bool {
    // minidom/Expat does not retrieve external entities during parseString;
    // declared external references contribute no replacement text.
    let ignore_external = |_public: Option<&str>, _uri: &str| Ok(Some(""));
    let Ok(document) = roxmltree::Document::parse_with_options(
        text,
        roxmltree::ParsingOptions {
            allow_dtd: true,
            entity_resolver: Some(&ignore_external),
            ..Default::default()
        },
    ) else {
        return false;
    };
    // Expat enforces Namespaces 1.0: only the default prefix can be undeclared.
    document.descendants().all(|node| {
        node.namespaces()
            .all(|ns| ns.name().is_none() || !ns.uri().is_empty())
    })
}
