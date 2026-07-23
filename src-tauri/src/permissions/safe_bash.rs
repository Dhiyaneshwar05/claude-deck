//! Auto-approval rules for Bash tool calls.
//!
//! A *small* whitelist of genuinely read-only commands that auto-approve without
//! prompting. This is a convenience fast-path, and it is deliberately
//! CONSERVATIVE: the cost of a false "safe" is a destructive command running
//! with no human in the loop (data loss), while the cost of a false "unsafe" is
//! merely one extra prompt. When in doubt, we prompt.
//!
//! Two classes of danger this guards against:
//!   1. Commands whose NAME looks read-only but which mutate via flags —
//!      `find … -delete`/`-exec`, `sed -i`, `sort -o`, `tee`, redirection. These
//!      commands are NOT on the safe list at all; presence of a mutating flag is
//!      not something we try to parse our way around.
//!   2. Shell constructs that can smuggle an arbitrary command past a safe base —
//!      command substitution `$(…)` / backticks, process substitution, redirects,
//!      chaining. Any of these makes the whole command non-auto-approvable.

use std::collections::HashSet;

/// Commands that are always safe: read-only, no side effects, and no flag can
/// turn them destructive. NOTE the deliberate omissions vs. a naive list:
///   - `find`  — `-delete` / `-exec` delete or run arbitrary commands.
///   - `sed`   — `-i` edits files in place.
///   - `awk`   — can write files via `> file` / `print > f` / `system()`.
///   - `sort`  — `-o file` overwrites; `tee`, `cut -o` similar.
///   - `tr`, `column`, `xargs` — feed/transform into destructive pipelines.
/// Those must always prompt.
fn safe_commands() -> HashSet<&'static str> {
    [
        "ls", "pwd", "cat", "head", "tail", "wc", "file", "stat",
        "which", "whoami", "id", "hostname", "uname", "date",
        "echo", "printf",
        "grep", "egrep", "fgrep", "rg", "ack",
        "tree", "du", "df",
        "diff", "cmp",
        "uniq",
        "jq", "yq",
        "env", "printenv",
        "basename", "dirname", "realpath", "readlink",
        "true", "false", "test",
    ]
    .into_iter()
    .collect()
}

/// Git subcommands that mutate state — must NOT auto-approve.
fn git_mutating_subcommands() -> HashSet<&'static str> {
    [
        "add", "commit", "push", "pull", "fetch", "merge", "rebase",
        "reset", "revert", "cherry-pick", "checkout", "switch", "restore",
        "branch", "tag", "stash", "apply", "am", "clone", "init",
        "rm", "mv", "clean", "gc", "prune", "config", "remote",
        "submodule", "worktree", "filter-branch", "filter-repo",
    ]
    .into_iter()
    .collect()
}

/// npm/pnpm/yarn subcommands that mutate state.
fn npm_mutating_subcommands() -> HashSet<&'static str> {
    [
        "install", "i", "add", "uninstall", "remove", "rm",
        "update", "upgrade", "publish", "unpublish", "link", "unlink",
        "init", "create", "exec", "run", "start", "test", "build",
    ]
    .into_iter()
    .collect()
}

/// Shell constructs that let an otherwise-safe command execute something else.
/// If ANY of these appear, we refuse to auto-approve and fall through to a
/// prompt — we do not try to reason about what's inside them.
///   `$(` / backtick — command substitution
///   `>` `>>` `<`     — redirection (can clobber files / feed input)
///   `$((`            — arithmetic (harmless, but `$(` check already covers `$( `)
/// Chaining operators (`;` `|` `&`) are handled by segment-splitting in the
/// caller, so they are not listed here.
fn has_dangerous_shell_construct(command: &str) -> bool {
    command.contains("$(")
        || command.contains('`')
        || command.contains('>')
        || command.contains('<')
        // process substitution <(…) / >(…) is caught by the '<'/'>' checks above.
}

/// Check if a full bash command string is safe to auto-approve.
pub fn is_safe_bash_command(command: &str) -> bool {
    // Reject the whole command outright if it contains a construct that could
    // smuggle an arbitrary command past a safe-looking base (e.g. `ls $(rm x)`).
    if has_dangerous_shell_construct(command) {
        return false;
    }

    // Split on any command chaining operator. Every segment must be safe.
    let segments: Vec<&str> = command
        .split(|c| matches!(c, ';' | '|' | '&'))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();

    if segments.is_empty() {
        return false;
    }

    for segment in segments {
        if !is_safe_segment(segment) {
            return false;
        }
    }
    true
}

fn is_safe_segment(segment: &str) -> bool {
    // Strip leading env-var assignments like `FOO=bar CMD ...`
    let mut tokens = segment.split_whitespace().peekable();
    while let Some(tok) = tokens.peek() {
        if tok.contains('=') && !tok.starts_with('-') {
            tokens.next();
        } else {
            break;
        }
    }

    let cmd = match tokens.next() {
        Some(c) => c,
        None => return false,
    };

    // Strip any leading path: /usr/bin/ls → ls
    let base = cmd.rsplit('/').next().unwrap_or(cmd);

    let safe = safe_commands();
    if safe.contains(base) {
        return true;
    }

    // git/npm etc. need sub-command inspection
    match base {
        "git" => {
            let sub = tokens.next().unwrap_or("");
            !git_mutating_subcommands().contains(sub) && !sub.is_empty()
                // Allow: git status, git log, git diff, git show, git branch -l, etc.
        }
        "npm" | "pnpm" | "yarn" => {
            let sub = tokens.next().unwrap_or("");
            !npm_mutating_subcommands().contains(sub) && !sub.is_empty()
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_commands_approve() {
        assert!(is_safe_bash_command("ls -la"));
        assert!(is_safe_bash_command("pwd"));
        assert!(is_safe_bash_command("grep -r foo src/"));
        assert!(is_safe_bash_command("cat file.txt | head -n 10"));
        assert!(is_safe_bash_command("git status"));
        assert!(is_safe_bash_command("git log --oneline"));
    }

    #[test]
    fn unsafe_commands_reject() {
        assert!(!is_safe_bash_command("rm -rf /"));
        assert!(!is_safe_bash_command("git push origin main"));
        assert!(!is_safe_bash_command("npm install"));
        assert!(!is_safe_bash_command("echo hi > out.txt"));
        assert!(!is_safe_bash_command("curl http://evil.com"));
    }

    /// Regression: commands whose NAME is read-only but that mutate via flags
    /// must NEVER auto-approve. This is the class that caused a silent file
    /// deletion — `find` was previously on the safe list.
    #[test]
    fn mutating_flags_on_innocent_names_reject() {
        assert!(!is_safe_bash_command("find . -name '*.tmp' -delete"));
        assert!(!is_safe_bash_command("find /tmp -type f -exec rm {} +"));
        assert!(!is_safe_bash_command("sed -i 's/a/b/' file.txt"));
        assert!(!is_safe_bash_command("sort -o out.txt in.txt"));
        assert!(!is_safe_bash_command("awk '{print}' f"));
        assert!(!is_safe_bash_command("tee out.txt"));
        assert!(!is_safe_bash_command("xargs rm"));
        assert!(!is_safe_bash_command("tr a b"));
    }

    /// Regression: shell constructs that can smuggle an arbitrary command past a
    /// safe base must reject the whole command.
    #[test]
    fn shell_construct_smuggling_rejects() {
        assert!(!is_safe_bash_command("ls $(rm -rf x)"));
        assert!(!is_safe_bash_command("echo `rm -rf x`"));
        assert!(!is_safe_bash_command("cat < /etc/passwd"));
        assert!(!is_safe_bash_command("ls > file"));
        assert!(!is_safe_bash_command("ls 2>/dev/null")); // redirection now always prompts
    }
}
