//! Auto-approval rules for Bash tool calls.
//!
//! Mirrors the `SAFE_BASH_COMMANDS` whitelist + sub-command guards from
//! clui-cc's `permission-server.ts`. Read-only commands auto-approve;
//! anything mutating (git push, npm install, rm, writing to files via `>`)
//! falls through and prompts the user.

use std::collections::HashSet;

/// Commands that are always safe (read-only, no side effects).
fn safe_commands() -> HashSet<&'static str> {
    [
        "ls", "pwd", "cat", "head", "tail", "wc", "file", "stat",
        "which", "whoami", "id", "hostname", "uname", "date",
        "echo", "printf",
        "grep", "egrep", "fgrep", "rg", "ack",
        "find", "tree", "du", "df",
        "diff", "cmp",
        "sort", "uniq", "cut", "awk", "sed", "tr", "column",
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

/// Check if a full bash command string is safe to auto-approve.
pub fn is_safe_bash_command(command: &str) -> bool {
    // Split on any command chaining operator
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
    // Reject stdout redirection to anywhere except /dev/null
    // (checking '>' is fine — we split on '|' already, so this only catches redirects)
    if let Some(idx) = segment.find('>') {
        let after = segment[idx..].trim_start_matches('>').trim();
        if !after.starts_with("/dev/null") {
            return false;
        }
    }

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

    #[test]
    fn dev_null_redirect_ok() {
        assert!(is_safe_bash_command("ls nonexistent 2>/dev/null"));
    }
}
