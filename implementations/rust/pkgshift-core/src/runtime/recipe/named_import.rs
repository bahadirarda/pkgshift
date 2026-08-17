pub(super) struct NamedImport {
    pub(super) indentation: String,
    pub(super) names: Vec<String>,
    pub(super) semicolon: String,
}

pub(super) fn parse(line: &str, specifier: &str) -> Option<NamedImport> {
    let trimmed = line.trim_start();
    let indentation = &line[..line.len() - trimmed.len()];
    let body = trimmed.strip_prefix("import ")?;
    let open = body.find('{')?;
    let close = body.find('}')?;
    if !body[..open].trim().is_empty() {
        return None;
    }
    let tail = body[close + 1..].trim();
    let expected_double = format!("from \"{specifier}\";");
    let expected_single = format!("from '{specifier}';");
    if tail != expected_double
        && tail != expected_single
        && tail != expected_double.trim_end_matches(';')
        && tail != expected_single.trim_end_matches(';')
    {
        return None;
    }
    let names = body[open + 1..close]
        .split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    Some(NamedImport {
        indentation: indentation.to_owned(),
        names,
        semicolon: if tail.ends_with(';') { ";" } else { "" }.to_owned(),
    })
}

pub(super) fn imported_name(name: &str) -> Option<&str> {
    let imported = name.split_whitespace().next()?;
    (!imported.is_empty()).then_some(imported)
}

pub(super) fn local_name(name: &str) -> Option<&str> {
    let mut tokens = name.split_whitespace();
    let imported = tokens.next()?;
    match (tokens.next(), tokens.next(), tokens.next()) {
        (None, None, None) => Some(imported),
        (Some("as"), Some(local), None) => Some(local),
        _ => None,
    }
}
