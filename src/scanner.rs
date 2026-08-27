pub struct ScannedLine<'a> {
    pub level: i64,
    pub pointer: &'a str,
    pub tag: &'a str,
    pub value: &'a str,
    pub crlf: &'a str,
}

pub fn scan_line(line: &str) -> Option<ScannedLine<'_>> {
    let (level, rest) = scan_level(line)?;

    // `(@[^@]+@ |)` prefers the pointer, then backs off to the empty branch.
    if let Some((pointer, after_pointer)) = scan_pointer(rest) {
        if let Some(scanned) = scan_tail(level, pointer, after_pointer, true) {
            return Some(scanned);
        }
    }
    scan_tail(level, "", rest, true)
}

pub fn scan_line_without_terminator(line: &str) -> Option<ScannedLine<'_>> {
    let (level, rest) = scan_level(line)?;

    if let Some((pointer, after_pointer)) = scan_pointer(rest) {
        if let Some(scanned) = scan_tail(level, pointer, after_pointer, false) {
            return Some(scanned);
        }
    }
    scan_tail(level, "", rest, false)
}

pub fn scan_continuation(line: &str) -> Option<(&str, &str)> {
    let bytes = line.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() && bytes[index] != b'\n' && bytes[index] != b'\r' {
        index += 1;
    }
    let terminator = scan_terminator(line, index)?;
    Some((&line[..index], terminator))
}

fn scan_level(line: &str) -> Option<(i64, &str)> {
    let bytes = line.as_bytes();
    if bytes.is_empty() {
        return None;
    }

    let digits = if bytes[0] == b'0' {
        1
    } else if bytes[0].is_ascii_digit() {
        let mut end = 1usize;
        while end < bytes.len() && bytes[end].is_ascii_digit() {
            end += 1;
        }
        end
    } else {
        return None;
    };

    if bytes.get(digits) != Some(&b' ') {
        return None;
    }

    let level = line[..digits].parse::<i64>().ok()?;
    Some((level, &line[digits + 1..]))
}

fn scan_pointer(rest: &str) -> Option<(&str, &str)> {
    let bytes = rest.as_bytes();
    if bytes.first() != Some(&b'@') {
        return None;
    }

    // `[^@]+` is greedy but cannot cross an `@`, so the closing `@` is the
    // next one along and there is nothing to backtrack over.
    let mut index = 1usize;
    while index < bytes.len() && bytes[index] != b'@' {
        index += 1;
    }
    if index == 1 || index >= bytes.len() {
        return None;
    }
    if bytes.get(index + 1) != Some(&b' ') {
        return None;
    }

    // The captured group keeps its trailing space, exactly as the regex group
    // does; the caller strips it with `rstrip(' ')` the way the original did.
    Some((&rest[..index + 2], &rest[index + 2..]))
}

fn scan_tail<'a>(
    level: i64,
    pointer: &'a str,
    rest: &'a str,
    require_terminator: bool,
) -> Option<ScannedLine<'a>> {
    let bytes = rest.as_bytes();

    let mut tag_end = 0usize;
    while tag_end < bytes.len() && is_tag_byte(bytes[tag_end]) {
        tag_end += 1;
    }
    if tag_end == 0 {
        return None;
    }
    let tag = &rest[..tag_end];

    let (value, after_value) = if bytes.get(tag_end) == Some(&b' ') {
        let mut end = tag_end + 1;
        while end < bytes.len() && bytes[end] != b'\n' && bytes[end] != b'\r' {
            end += 1;
        }
        (&rest[tag_end + 1..end], end)
    } else {
        ("", tag_end)
    };

    if !require_terminator {
        // The lenient variant forces `\n` regardless of what the line ends with.
        return Some(ScannedLine {
            level,
            pointer,
            tag,
            value,
            crlf: "\n",
        });
    }

    // Backtracking: if the value branch leaves no terminator, the empty branch
    // cannot help -- what follows the tag is a space, never `\r` or `\n`.
    let crlf = scan_terminator(rest, after_value)?;
    Some(ScannedLine {
        level,
        pointer,
        tag,
        value,
        crlf,
    })
}

fn is_tag_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn scan_terminator(line: &str, start: usize) -> Option<&str> {
    let bytes = line.as_bytes();
    let first = *bytes.get(start)?;
    if first != b'\n' && first != b'\r' {
        return None;
    }
    let mut end = start + 1;
    if let Some(second) = bytes.get(end) {
        if *second == b'\n' || *second == b'\r' {
            end += 1;
        }
    }
    Some(&line[start..end])
}

pub fn line_ranges(blob: &[u8]) -> Vec<(usize, usize)> {
    let mut ranges = Vec::with_capacity(blob.len() / 16 + 1);
    let mut start = 0usize;
    for index in memchr::memchr_iter(b'\n', blob) {
        ranges.push((start, index + 1 - start));
        start = index + 1;
    }
    if start < blob.len() {
        ranges.push((start, blob.len() - start));
    }
    ranges
}

pub fn strip_bom(text: &str) -> &str {
    text.strip_prefix('\u{feff}').unwrap_or(text)
}
