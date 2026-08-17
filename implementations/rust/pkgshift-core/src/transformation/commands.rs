use crate::model::PackageManagerId;

fn command_name(manager: PackageManagerId) -> &'static str {
    match manager {
        PackageManagerId::YarnClassic | PackageManagerId::YarnModern => "yarn",
        PackageManagerId::Npm => "npm",
        PackageManagerId::Pnpm => "pnpm",
        PackageManagerId::Bun => "bun",
        PackageManagerId::Vlt => "vlt",
        PackageManagerId::Deno => "deno",
    }
}

fn boundary(character: Option<char>) -> bool {
    character.is_none_or(|value| !value.is_ascii_alphanumeric() && value != '_')
}

fn command_position(prefix: &str) -> bool {
    let trimmed = prefix.trim_end();
    trimmed.is_empty()
        || trimmed == "$"
        || ["&&", "||", ";", "|", "(", "{"]
            .iter()
            .any(|value| trimmed.ends_with(value))
        || trimmed.ends_with('\n')
}

fn next_word(value: &str) -> (&str, &str, &str) {
    let whitespace = value
        .bytes()
        .take_while(|byte| matches!(byte, b' ' | b'\t'))
        .count();
    let remainder = &value[whitespace..];
    let length = remainder
        .bytes()
        .take_while(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b':'))
        .count();
    (
        &value[..whitespace],
        &remainder[..length],
        &remainder[length..],
    )
}

fn source_command_is_package_management(source: PackageManagerId, command: &str) -> bool {
    match source {
        PackageManagerId::Bun => matches!(
            command,
            "install" | "ci" | "add" | "remove" | "rm" | "run" | "x" | "pm" | "update"
        ),
        PackageManagerId::Deno => {
            matches!(command, "install" | "add" | "remove" | "task" | "outdated")
        }
        _ => !command.is_empty() && !command.starts_with('-'),
    }
}

fn mapped_invocation(
    source: PackageManagerId,
    target: PackageManagerId,
    command: &str,
) -> Option<String> {
    let target_name = command_name(target);
    if matches!(command, "run" | "task")
        || (!matches!(
            command,
            "install"
                | "ci"
                | "add"
                | "remove"
                | "rm"
                | "exec"
                | "dlx"
                | "x"
                | "pm"
                | "update"
                | "up"
                | "import"
                | "rebuild"
                | "dedupe"
                | "why"
                | "list"
                | "outdated"
        ) && !matches!(source, PackageManagerId::Bun | PackageManagerId::Deno))
    {
        let runner = if target == PackageManagerId::Deno {
            "task"
        } else {
            "run"
        };
        if matches!(command, "run" | "task") {
            return Some(format!("{target_name} {runner}"));
        }
        return Some(format!("{target_name} {runner} {command}"));
    }
    let mapped = match command {
        "ci" if target == PackageManagerId::Npm => "ci",
        "ci" | "install" => "install",
        "remove" | "rm" if target == PackageManagerId::Npm => "uninstall",
        "remove" | "rm" => "remove",
        "exec" | "dlx" | "x" if target == PackageManagerId::Bun => "x",
        "exec" | "dlx" | "x" if target == PackageManagerId::Deno => return None,
        "exec" | "dlx" | "x"
            if matches!(
                target,
                PackageManagerId::YarnClassic | PackageManagerId::YarnModern
            ) =>
        {
            "dlx"
        }
        "exec" | "dlx" | "x" => "exec",
        "pm" if target != PackageManagerId::Bun => return None,
        value => value,
    };
    Some(format!("{target_name} {mapped}"))
}

pub(super) fn rewrite_package_manager_commands(
    content: &str,
    source: PackageManagerId,
    target: PackageManagerId,
) -> String {
    let source_name = command_name(source);
    let target_name = command_name(target);
    if source_name == target_name {
        return content.to_owned();
    }

    let mut output = String::with_capacity(content.len());
    let mut remainder = content;
    while let Some(index) = remainder.find(source_name) {
        output.push_str(&remainder[..index]);
        let suffix = &remainder[index + source_name.len()..];
        if boundary(output.chars().next_back())
            && boundary(suffix.chars().next())
            && command_position(&output)
        {
            let (_whitespace, command, after_command) = next_word(suffix);
            if source_command_is_package_management(source, command)
                && let Some(invocation) = mapped_invocation(source, target, command)
            {
                output.push_str(&invocation);
                remainder = after_command;
                continue;
            }
        }
        output.push_str(source_name);
        remainder = suffix;
    }
    output.push_str(remainder);
    output
}

pub(super) fn contains_package_manager_command(content: &str, source: PackageManagerId) -> bool {
    let probe = if source == PackageManagerId::Npm {
        PackageManagerId::Pnpm
    } else {
        PackageManagerId::Npm
    };
    rewrite_package_manager_commands(content, source, probe) != content
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrites_only_shell_command_positions() {
        let content = "pnpm install && pnpm test\necho pnpm install\n";
        let rewritten = rewrite_package_manager_commands(
            content,
            PackageManagerId::Pnpm,
            PackageManagerId::Bun,
        );
        assert_eq!(
            rewritten,
            "bun install && bun run test\necho pnpm install\n"
        );
    }

    #[test]
    fn preserves_bun_runtime_commands() {
        let content = "bun test && bun run lint";
        let rewritten = rewrite_package_manager_commands(
            content,
            PackageManagerId::Bun,
            PackageManagerId::Deno,
        );
        assert_eq!(rewritten, "bun test && deno task lint");
    }
}
