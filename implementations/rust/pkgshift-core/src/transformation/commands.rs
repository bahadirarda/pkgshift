pub(super) fn replace_command_token(content: &str, source: &str, target: &str) -> String {
    let mut output = String::with_capacity(content.len());
    let mut remainder = content;
    while let Some(index) = remainder.find(source) {
        output.push_str(&remainder[..index]);
        let before = output.chars().next_back();
        let after = remainder[index + source.len()..].chars().next();
        let boundary = |character: Option<char>| {
            character.is_none_or(|value| !value.is_ascii_alphanumeric() && value != '_')
        };
        if boundary(before) && boundary(after) {
            output.push_str(target);
            let suffix = &remainder[index + source.len()..];
            let whitespace = suffix
                .bytes()
                .take_while(|value| matches!(value, b' ' | b'\t'))
                .count();
            let command_source = &suffix[whitespace..];
            if whitespace > 0
                && let Some(command) = ["install", "ci", "run", "task", "add", "remove"]
                    .into_iter()
                    .find(|command| {
                        command_source.starts_with(command)
                            && boundary(command_source[command.len()..].chars().next())
                    })
            {
                output.push_str(&suffix[..whitespace]);
                let mapped = if target == "deno" && matches!(command, "run" | "task") {
                    "task"
                } else if source == "deno" && command == "task" {
                    "run"
                } else if command == "ci" {
                    "install"
                } else {
                    command
                };
                output.push_str(mapped);
                remainder = &command_source[command.len()..];
                continue;
            }
        } else {
            output.push_str(source);
        }
        remainder = &remainder[index + source.len()..];
    }
    output.push_str(remainder);
    output
}
