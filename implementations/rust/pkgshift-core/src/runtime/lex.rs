#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Code,
    SingleQuote,
    DoubleQuote,
    Template,
    LineComment,
    BlockComment,
}

pub(crate) fn code_mask(content: &str) -> Vec<bool> {
    let bytes = content.as_bytes();
    let mut mask = vec![false; bytes.len()];
    let mut mode = Mode::Code;
    let mut escaped = false;
    let mut template_depths = Vec::<usize>::new();
    let mut index = 0;
    while index < bytes.len() {
        match mode {
            Mode::Code => {
                mask[index] = true;
                match bytes[index] {
                    b'\'' => {
                        mask[index] = false;
                        mode = Mode::SingleQuote;
                    }
                    b'"' => {
                        mask[index] = false;
                        mode = Mode::DoubleQuote;
                    }
                    b'`' => {
                        mask[index] = false;
                        mode = Mode::Template;
                    }
                    b'/' if bytes.get(index + 1) == Some(&b'/') => {
                        mask[index] = false;
                        mode = Mode::LineComment;
                    }
                    b'/' if bytes.get(index + 1) == Some(&b'*') => {
                        mask[index] = false;
                        mode = Mode::BlockComment;
                    }
                    b'{' if !template_depths.is_empty() => {
                        *template_depths.last_mut().expect("checked non-empty") += 1;
                    }
                    b'}' if !template_depths.is_empty() => {
                        let depth = template_depths.last_mut().expect("checked non-empty");
                        *depth -= 1;
                        if *depth == 0 {
                            mask[index] = false;
                            template_depths.pop();
                            mode = Mode::Template;
                        }
                    }
                    _ => {}
                }
            }
            Mode::SingleQuote | Mode::DoubleQuote => {
                let quote = if mode == Mode::SingleQuote {
                    b'\''
                } else {
                    b'"'
                };
                if escaped {
                    escaped = false;
                } else if bytes[index] == b'\\' {
                    escaped = true;
                } else if bytes[index] == quote {
                    mode = Mode::Code;
                }
            }
            Mode::Template => {
                if escaped {
                    escaped = false;
                } else if bytes[index] == b'\\' {
                    escaped = true;
                } else if bytes[index] == b'`' {
                    mode = Mode::Code;
                } else if bytes[index] == b'$' && bytes.get(index + 1) == Some(&b'{') {
                    template_depths.push(1);
                    index += 1;
                    mode = Mode::Code;
                }
            }
            Mode::LineComment => {
                if bytes[index] == b'\n' {
                    mask[index] = true;
                    mode = Mode::Code;
                }
            }
            Mode::BlockComment => {
                if bytes[index] == b'*' && bytes.get(index + 1) == Some(&b'/') {
                    index += 1;
                    mode = Mode::Code;
                }
            }
        }
        index += 1;
    }
    mask
}

pub(crate) fn code_occurrences(content: &str, needle: &str) -> Vec<usize> {
    let mask = code_mask(content);
    content
        .match_indices(needle)
        .filter_map(|(index, _)| mask.get(index).copied().unwrap_or(false).then_some(index))
        .collect()
}

pub(crate) fn contains_code(content: &str, needle: &str) -> bool {
    !code_occurrences(content, needle).is_empty()
}

pub(crate) fn without_comments(content: &str) -> String {
    let bytes = content.as_bytes();
    let mut output = bytes.to_vec();
    let mut mode = Mode::Code;
    let mut escaped = false;
    let mut index = 0usize;
    while index < bytes.len() {
        match mode {
            Mode::Code => match bytes[index] {
                b'\'' => mode = Mode::SingleQuote,
                b'"' => mode = Mode::DoubleQuote,
                b'`' => mode = Mode::Template,
                b'/' if bytes.get(index + 1) == Some(&b'/') => {
                    output[index] = b' ';
                    mode = Mode::LineComment;
                }
                b'/' if bytes.get(index + 1) == Some(&b'*') => {
                    output[index] = b' ';
                    mode = Mode::BlockComment;
                }
                _ => {}
            },
            Mode::SingleQuote | Mode::DoubleQuote | Mode::Template => {
                let closing = match mode {
                    Mode::SingleQuote => b'\'',
                    Mode::DoubleQuote => b'"',
                    Mode::Template => b'`',
                    _ => unreachable!(),
                };
                if escaped {
                    escaped = false;
                } else if bytes[index] == b'\\' {
                    escaped = true;
                } else if bytes[index] == closing {
                    mode = Mode::Code;
                }
            }
            Mode::LineComment => {
                if bytes[index] == b'\n' {
                    mode = Mode::Code;
                } else {
                    output[index] = b' ';
                }
            }
            Mode::BlockComment => {
                if bytes[index] == b'*' && bytes.get(index + 1) == Some(&b'/') {
                    output[index] = b' ';
                    output[index + 1] = b' ';
                    index += 1;
                    mode = Mode::Code;
                } else if bytes[index] != b'\n' {
                    output[index] = b' ';
                }
            }
        }
        index += 1;
    }
    String::from_utf8(output).expect("comment stripping preserves UTF-8 bytes")
}

#[cfg(test)]
mod tests {
    use super::{code_occurrences, without_comments};

    #[test]
    fn ignores_comments_and_strings_but_reads_template_expressions() {
        let content = r#"
// Bun.serve({})
const literal = "Bun.file";
const template = `${Bun.file(path).text()}`;
Bun.serve({ fetch: handler });
"#;
        assert_eq!(code_occurrences(content, "Bun.serve").len(), 1);
        assert_eq!(code_occurrences(content, "Bun.file").len(), 1);
    }

    #[test]
    fn removes_comments_without_removing_module_strings() {
        let content = "// from \"bun:test\"\nimport {\n  test\n} from \"bun:test\";\n";
        let stripped = without_comments(content);
        assert_eq!(stripped.matches("bun:test").count(), 1);
    }
}
