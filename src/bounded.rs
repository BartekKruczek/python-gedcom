//! Line-splitting helpers for `set_multi_line_value`.

fn is_line_boundary(character: char) -> bool {
    matches!(
        character,
        '\n' // LINE FEED
        | '\r' // CARRIAGE RETURN
        | '\u{0b}' // LINE TABULATION
        | '\u{0c}' // FORM FEED
        | '\u{1c}' // FILE SEPARATOR
        | '\u{1d}' // GROUP SEPARATOR
        | '\u{1e}' // RECORD SEPARATOR
        | '\u{85}' // NEXT LINE
        | '\u{2028}' // LINE SEPARATOR
        | '\u{2029}' // PARAGRAPH SEPARATOR
    )
}

pub fn splitlines(value: &[char]) -> Vec<Vec<char>> {
    let mut lines = Vec::new();
    let mut start = 0usize;
    let mut index = 0usize;

    while index < value.len() {
        let character = value[index];
        if is_line_boundary(character) {
            lines.push(value[start..index].to_vec());
            // `\r\n` counts as a single boundary.
            if character == '\r' && index + 1 < value.len() && value[index + 1] == '\n' {
                index += 1;
            }
            index += 1;
            start = index;
        } else {
            index += 1;
        }
    }

    if start < value.len() {
        lines.push(value[start..].to_vec());
    }

    lines
}

pub fn line_length(line: &[char], available: usize) -> usize {
    let total = line.len();
    if total <= available {
        return total;
    }

    let mut spaces = 0usize;
    while spaces < available && line[available - spaces - 1] == ' ' {
        spaces += 1;
    }

    if spaces == available {
        return available;
    }

    available - spaces
}

pub fn available_characters(rendered_length: usize) -> usize {
    if rendered_length > 255 {
        0
    } else {
        255 - rendered_length
    }
}
