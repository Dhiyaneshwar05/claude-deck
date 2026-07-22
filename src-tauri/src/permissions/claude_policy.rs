//! Evaluate the user's OWN Claude Code permission policy.
//!
//! Ported from preloop's `cli/internal/cmd/claude_permission_policy.go`
//! (github.com/preloop/preloop). Instead of re-inventing an allowlist (the job
//! `safe_bash.rs` does for a hardcoded set of read-only commands), this reads
//! the user's `~/.claude/settings.json` (+ `settings.local.json`) permission
//! rules and computes what Claude Code itself would decide: allow / deny / ask.
//!
//! Only an `ask` outcome escalates to the human queue — a matching allow/deny
//! rule short-circuits without ever surfacing in the Permission Center, so the
//! hub honors the config the user already maintains rather than second-guessing
//! it. We parse only the `permissions` block; everything else is ignored.

use std::path::{Path, PathBuf};

use serde::Deserialize;

/// The outcome of evaluating the user's policy against one tool call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyDecision {
    /// A matching allow rule (or a permissive mode) — auto-approve, no prompt.
    Allow,
    /// A matching deny rule — auto-reject, no prompt.
    Deny,
    /// No rule decided it (or a matching ask rule) — escalate to the human.
    Ask,
}

/// The merged allow/deny/ask rule sets + default mode from the user's config.
#[derive(Debug, Clone, Default)]
pub struct ClaudePermissionPolicy {
    pub allow: Vec<String>,
    pub deny: Vec<String>,
    pub ask: Vec<String>,
    pub default_mode: String,
}

/// The subset of a Claude `settings.json` we care about.
#[derive(Debug, Default, Deserialize)]
struct ClaudeSettingsDocument {
    #[serde(default)]
    permissions: SettingsPermissions,
}

#[derive(Debug, Default, Deserialize)]
struct SettingsPermissions {
    #[serde(default)]
    allow: Vec<String>,
    #[serde(default)]
    deny: Vec<String>,
    #[serde(default)]
    ask: Vec<String>,
    #[serde(rename = "defaultMode", default)]
    default_mode: String,
}

/// Load + merge `~/.claude/settings.json` and `~/.claude/settings.local.json`.
///
/// Local wins for `defaultMode`; rule lists are unioned (appended). A missing
/// settings file is not an error — it just contributes nothing, so with no
/// config at all every call falls through to `ask`.
pub fn load_claude_permission_policy() -> Result<ClaudePermissionPolicy, String> {
    let home = home_dir().ok_or_else(|| "failed to resolve home directory".to_string())?;
    load_claude_permission_policy_from(&home.join(".claude"))
}

/// Testable core of [`load_claude_permission_policy`] — reads the two settings
/// files from an explicit `.claude` directory.
fn load_claude_permission_policy_from(claude_dir: &Path) -> Result<ClaudePermissionPolicy, String> {
    let mut policy = ClaudePermissionPolicy::default();
    for name in ["settings.json", "settings.local.json"] {
        let path = claude_dir.join(name);
        match read_claude_settings_document(&path)? {
            Some(doc) => {
                policy.allow.extend(doc.permissions.allow);
                policy.deny.extend(doc.permissions.deny);
                policy.ask.extend(doc.permissions.ask);
                let mode = doc.permissions.default_mode.trim();
                if !mode.is_empty() {
                    policy.default_mode = mode.to_string();
                }
            }
            None => continue,
        }
    }
    Ok(policy)
}

/// Read one settings file. `Ok(None)` means "absent or empty" (not an error).
fn read_claude_settings_document(path: &Path) -> Result<Option<ClaudeSettingsDocument>, String> {
    let data = match std::fs::read_to_string(path) {
        Ok(d) => d,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("failed to read {}: {}", path.display(), e)),
    };
    if data.trim().is_empty() {
        return Ok(None);
    }
    let doc: ClaudeSettingsDocument = serde_json::from_str(&data)
        .map_err(|e| format!("failed to parse {}: {}", path.display(), e))?;
    Ok(Some(doc))
}

/// Decide, using the user's own configuration, whether a tool call would be
/// auto-allowed, auto-denied, or prompted for. The runtime `mode` from the hook
/// event overrides the configured `defaultMode` when non-empty.
///
/// Precedence mirrors Claude Code's own evaluation order:
///  1. `bypassPermissions` mode      → allow everything
///  2. a matching **deny** rule       → deny
///  3. a matching **ask** rule        → ask (beats allow)
///  4. `acceptEdits` mode + edit tool → allow
///  5. a matching **allow** rule      → allow
///  6. otherwise                      → ask (the default "would prompt" case)
pub fn evaluate_claude_permission_policy(
    policy: &ClaudePermissionPolicy,
    mode: &str,
    tool_name: &str,
    tool_input: &serde_json::Value,
) -> PolicyDecision {
    let effective_mode = {
        let m = mode.trim();
        if m.is_empty() {
            policy.default_mode.trim()
        } else {
            m
        }
    };

    if effective_mode.eq_ignore_ascii_case("bypassPermissions") {
        return PolicyDecision::Allow;
    }

    // Bash is evaluated segment-aware (compound commands like `a && b` require
    // EVERY segment to be allowed, and ANY denied segment denies the whole) —
    // see `evaluate_bash`. Everything else uses the generic single-target path.
    if tool_name.trim().eq_ignore_ascii_case("bash") {
        return evaluate_bash(policy, effective_mode, tool_input);
    }

    if match_any_claude_rule(&policy.deny, tool_name, tool_input, RulePosition::Deny) {
        return PolicyDecision::Deny;
    }
    if match_any_claude_rule(&policy.ask, tool_name, tool_input, RulePosition::Ask) {
        return PolicyDecision::Ask;
    }
    if effective_mode.eq_ignore_ascii_case("acceptEdits") && is_claude_edit_tool(tool_name) {
        return PolicyDecision::Allow;
    }
    if match_any_claude_rule(&policy.allow, tool_name, tool_input, RulePosition::Allow) {
        return PolicyDecision::Allow;
    }
    PolicyDecision::Ask
}

/// Where a rule came from. Matching for `Allow` is deliberately *conservative*
/// for dangerous tools (a false allow silently skips the human); `Deny`/`Ask`
/// matching is *liberal* (a false match just adds safety). This asymmetry is
/// intentional — see `match_claude_permission_rule`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RulePosition {
    Allow,
    Deny,
    Ask,
}

/// Segment-aware Bash evaluation. Claude treats a compound command
/// (`a && b`, `a | b`, `a; b`) as allowed only if every segment is allowed, and
/// denied if any segment is denied. We replicate that so a permitted prefix
/// (e.g. `ls:*`) can never smuggle an un-permitted second command
/// (`ls && curl evil.com`) past the hub.
fn evaluate_bash(
    policy: &ClaudePermissionPolicy,
    effective_mode: &str,
    tool_input: &serde_json::Value,
) -> PolicyDecision {
    let command = string_field(tool_input, "command");
    let segments = split_bash_segments(&command);

    // Deny/Ask: liberal — any segment matching triggers it (deny beats ask).
    for seg in &segments {
        if policy.deny.iter().any(|r| bash_rule_matches(r, seg)) {
            return PolicyDecision::Deny;
        }
    }
    for seg in &segments {
        if policy.ask.iter().any(|r| bash_rule_matches(r, seg)) {
            return PolicyDecision::Ask;
        }
    }

    let _ = effective_mode; // acceptEdits never auto-allows Bash (matches Claude).

    // Allow: conservative. A command with shell constructs we can't safely
    // reason about (command substitution, backticks, redirection) must never be
    // auto-allowed — fall through to the human. Otherwise, every segment must be
    // covered by some allow rule.
    if !bash_allow_analyzable(&command) {
        return PolicyDecision::Ask;
    }
    let all_allowed = !segments.is_empty()
        && segments
            .iter()
            .all(|seg| policy.allow.iter().any(|r| bash_rule_matches(r, seg)));
    if all_allowed {
        PolicyDecision::Allow
    } else {
        PolicyDecision::Ask
    }
}

/// Whether the user set an explicit **ask** rule for this call. A convenience
/// auto-allow (e.g. `safe_bash`) must NOT override this — the user deliberately
/// asked to review this tool call. (Deny rules already short-circuit in
/// [`evaluate_claude_permission_policy`], so we only need to guard against ask.)
pub fn has_explicit_ask_rule(
    policy: &ClaudePermissionPolicy,
    tool_name: &str,
    tool_input: &serde_json::Value,
) -> bool {
    if tool_name.trim().eq_ignore_ascii_case("bash") {
        let command = string_field(tool_input, "command");
        return split_bash_segments(&command)
            .iter()
            .any(|seg| policy.ask.iter().any(|r| bash_rule_matches(r, seg)));
    }
    match_any_claude_rule(&policy.ask, tool_name, tool_input, RulePosition::Ask)
}

fn match_any_claude_rule(
    rules: &[String],
    tool_name: &str,
    tool_input: &serde_json::Value,
    position: RulePosition,
) -> bool {
    rules
        .iter()
        .any(|rule| match_claude_permission_rule(rule, tool_name, tool_input, position))
}

/// Report whether a single rule (e.g. `"Bash"`, `"Read(~/.zshrc)"`,
/// `"WebFetch(domain:*.internal)"`) matches the given non-Bash tool call.
/// (Bash goes through `bash_rule_matches` / `evaluate_bash`.)
///
/// Network tools (WebFetch/WebSearch) are special: a bare or wildcard **allow**
/// rule is NOT honored for auto-approval, because doing so safely would require
/// perfectly reproducing Claude's SSRF/domain-deny semantics — a miss there
/// would auto-approve a request to an internal/metadata host that today
/// correctly prompts. So for network tools we only auto-allow a concrete
/// `domain:<specific-host>` rule, and we still apply every deny/ask rule.
fn match_claude_permission_rule(
    rule: &str,
    tool_name: &str,
    tool_input: &serde_json::Value,
    position: RulePosition,
) -> bool {
    let (rule_tool, specifier, has_specifier) = split_claude_permission_rule(rule);
    if rule_tool.is_empty() {
        return false;
    }
    if !rule_tool.eq_ignore_ascii_case(tool_name.trim()) {
        return false;
    }

    let is_network = matches!(
        tool_name.trim().to_lowercase().as_str(),
        "webfetch" | "websearch"
    );

    if !has_specifier {
        // Bare tool-wide rule. For a network tool in ALLOW position we refuse to
        // honor a blanket "allow all fetches" (SSRF risk); deny/ask still apply.
        return !(is_network && position == RulePosition::Allow);
    }

    // A `domain:<host>` specifier matches against the request's host.
    if let Some(domain_pat) = specifier.strip_prefix("domain:") {
        let host = extract_host(&claude_rule_target(tool_name, tool_input));
        let matches = domain_matches(domain_pat.trim(), &host);
        if position == RulePosition::Allow && domain_pat.contains('*') {
            // Don't auto-allow on a wildcard-domain allow rule (too broad to
            // trust for auto-approval); it still counts for deny/ask.
            return false;
        }
        return matches;
    }

    let target = claude_rule_target(tool_name, tool_input);
    glob_match(&specifier, &target)
}

/// Match a single Bash permission rule against ONE already-split command
/// segment. A bare `Bash` rule matches any segment; `Bash(<spec>)` uses
/// Claude's prefix-wildcard semantics (see `bash_specifier_matches`).
fn bash_rule_matches(rule: &str, segment: &str) -> bool {
    let (rule_tool, specifier, has_specifier) = split_claude_permission_rule(rule);
    if !rule_tool.eq_ignore_ascii_case("bash") {
        return false;
    }
    if !has_specifier {
        return true;
    }
    bash_specifier_matches(&specifier, segment.trim())
}

/// Claude's Bash specifier matching:
///  - `prefix:*` or `prefix *`  → token-boundary prefix match (the common form:
///    `git status:*` matches `git status` and `git status -s`, but `ls:*` does
///    NOT match `lsof`).
///  - any other `*`             → simple glob.
///  - no `*`                    → exact match.
fn bash_specifier_matches(spec: &str, segment: &str) -> bool {
    let spec = spec.trim();
    let prefix = spec
        .strip_suffix(":*")
        .or_else(|| spec.strip_suffix(" *"))
        .map(str::trim);
    if let Some(p) = prefix {
        if p.is_empty() {
            return true;
        }
        if !segment.starts_with(p) {
            return false;
        }
        // Require a token boundary right after the prefix so `ls` doesn't match
        // `lsof`, but `git status` does match `git status -s` / `git status:x`.
        return match segment[p.len()..].chars().next() {
            None => true,
            Some(c) => !(c.is_alphanumeric() || c == '_' || c == '-'),
        };
    }
    if spec.contains('*') {
        return glob_match(spec, segment);
    }
    spec == segment
}

/// Split a Bash command into the segments Claude evaluates independently: on
/// `&&`, `||`, `;`, `|`, and newlines. Conservative (doesn't fully honor quotes)
/// — over-splitting only ever makes allow-matching *stricter*, which is safe.
fn split_bash_segments(command: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = String::new();
    let bytes = command.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let two = &command[i..(i + 2).min(command.len())];
        if two == "&&" || two == "||" {
            segments.push(std::mem::take(&mut current));
            i += 2;
            continue;
        }
        let c = bytes[i] as char;
        if c == ';' || c == '|' || c == '\n' {
            segments.push(std::mem::take(&mut current));
            i += 1;
            continue;
        }
        current.push(c);
        i += 1;
    }
    segments.push(current);
    segments
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Whether a Bash command is safe to reason about for AUTO-ALLOW. Commands with
/// substitution, backticks, or redirection can turn a benign-looking prefix into
/// something dangerous (`cat x > /etc/passwd`), so they never auto-allow — they
/// fall through to the human queue.
fn bash_allow_analyzable(command: &str) -> bool {
    !command.contains('`')
        && !command.contains("$(")
        && !command.contains('>')
        && !command.contains('<')
}

/// Extract the lowercased host from a URL (best-effort, no external crate).
/// Handles scheme, userinfo, port, path/query/fragment, and `[ipv6]` literals.
fn extract_host(url: &str) -> String {
    let s = url.trim();
    let after_scheme = match s.find("://") {
        Some(i) => &s[i + 3..],
        None => s,
    };
    let after_user = match after_scheme.find('@') {
        Some(i) => &after_scheme[i + 1..],
        None => after_scheme,
    };
    // IPv6 literal: host is inside the brackets.
    if let Some(rest) = after_user.strip_prefix('[') {
        if let Some(close) = rest.find(']') {
            return rest[..close].to_lowercase();
        }
    }
    let end = after_user
        .find(|c| c == '/' || c == '?' || c == '#' || c == ':')
        .unwrap_or(after_user.len());
    after_user[..end].trim_end_matches('.').to_lowercase()
}

/// Match a `domain:` pattern against a host. Wildcards glob; a bare host matches
/// itself and any subdomain (`example.com` matches `api.example.com`).
fn domain_matches(pattern: &str, host: &str) -> bool {
    let pattern = pattern.trim().to_lowercase();
    if pattern.is_empty() || host.is_empty() {
        return false;
    }
    if pattern.contains('*') {
        return glob_match(&pattern, host);
    }
    host == pattern || host.ends_with(&format!(".{}", pattern))
}

/// Parse `"Tool(specifier)"` into `(tool, specifier, has_specifier)`. An empty
/// specifier (`"Tool()"`) is treated as tool-wide, same as `"Tool"`.
fn split_claude_permission_rule(rule: &str) -> (String, String, bool) {
    let rule = rule.trim();
    if rule.is_empty() {
        return (String::new(), String::new(), false);
    }
    let open = match rule.find('(') {
        Some(i) => i,
        None => return (rule.to_string(), String::new(), false),
    };
    let tool = rule[..open].trim().to_string();
    let mut inner = &rule[open + 1..];
    if let Some(close) = inner.rfind(')') {
        inner = &inner[..close];
    }
    let inner = inner.trim();
    if inner.is_empty() {
        return (tool, String::new(), false);
    }
    (tool, inner.to_string(), true)
}

/// The string a specifier is matched against for a given tool: Bash matches the
/// command, file tools match the path, web tools match the URL. Anything else
/// falls back to the most common identifying field, then the empty string
/// (which only an unrestricted rule would match).
fn claude_rule_target(tool_name: &str, tool_input: &serde_json::Value) -> String {
    match tool_name.trim().to_lowercase().as_str() {
        "bash" => string_field(tool_input, "command"),
        "read" | "edit" | "write" | "multiedit" | "notebookedit" => first_non_empty(&[
            string_field(tool_input, "file_path"),
            string_field(tool_input, "path"),
            string_field(tool_input, "notebook_path"),
        ]),
        "webfetch" | "websearch" => first_non_empty(&[
            string_field(tool_input, "url"),
            string_field(tool_input, "domain"),
            string_field(tool_input, "query"),
        ]),
        _ => first_non_empty(&[
            string_field(tool_input, "command"),
            string_field(tool_input, "file_path"),
            string_field(tool_input, "path"),
            string_field(tool_input, "url"),
        ]),
    }
}

fn is_claude_edit_tool(tool_name: &str) -> bool {
    matches!(
        tool_name.trim().to_lowercase().as_str(),
        "edit" | "write" | "multiedit" | "notebookedit"
    )
}

fn string_field(input: &serde_json::Value, key: &str) -> String {
    input
        .get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

fn first_non_empty(candidates: &[String]) -> String {
    candidates
        .iter()
        .find(|s| !s.is_empty())
        .cloned()
        .unwrap_or_default()
}

/// Simple, case-sensitive glob match supporting `*` (any run of characters,
/// including empty). Covers Claude's common rule shapes such as `npm run test:*`
/// and `https://api.*`. A pattern with no `*` must match exactly.
fn glob_match(pattern: &str, s: &str) -> bool {
    if !pattern.contains('*') {
        return pattern == s;
    }
    let parts: Vec<&str> = pattern.split('*').collect();
    let mut rest = s;

    // Anchor the first segment as a prefix.
    let first = parts[0];
    if !first.is_empty() {
        if !rest.starts_with(first) {
            return false;
        }
        rest = &rest[first.len()..];
    }

    // Middle segments must appear in order.
    let last = parts[parts.len() - 1];
    for part in &parts[1..parts.len() - 1] {
        if part.is_empty() {
            continue;
        }
        match rest.find(part) {
            Some(idx) => rest = &rest[idx + part.len()..],
            None => return false,
        }
    }

    // Anchor the last segment as a suffix.
    if last.is_empty() {
        return true;
    }
    rest.ends_with(last)
}

/// Resolve the user's home directory without pulling in an extra crate.
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn glob_match_cases() {
        let cases = [
            ("npm run build", "npm run build", true),
            ("npm run build", "npm run build:prod", false),
            ("npm run test:*", "npm run test:unit", true),
            ("npm run test:*", "npm run test:", true),
            ("npm run test:*", "npm run lint", false),
            ("*", "anything", true),
            ("https://api.*", "https://api.example.com", true),
            ("https://api.*", "https://web.example.com", false),
            ("a*c", "abc", true),
            ("a*c", "ac", true),
            ("a*c", "abd", false),
        ];
        for (pat, input, want) in cases {
            assert_eq!(glob_match(pat, input), want, "glob_match({pat:?}, {input:?})");
        }
    }

    #[test]
    fn split_rule_cases() {
        let cases = [
            ("Bash", "Bash", "", false),
            ("Bash(npm run test:*)", "Bash", "npm run test:*", true),
            ("Read(~/.zshrc)", "Read", "~/.zshrc", true),
            ("Bash()", "Bash", "", false),
            ("  Edit ", "Edit", "", false),
        ];
        for (rule, tool, spec, has) in cases {
            let (t, s, h) = split_claude_permission_rule(rule);
            assert_eq!((t.as_str(), s.as_str(), h), (tool, spec, has), "rule {rule:?}");
        }
    }

    #[test]
    fn evaluate_precedence() {
        let bash = json!({"command": "npm run test:unit"});
        let edit = json!({"file_path": "/repo/src/app.rs"});
        let p = |allow: &[&str], deny: &[&str], ask: &[&str], mode: &str| ClaudePermissionPolicy {
            allow: allow.iter().map(|s| s.to_string()).collect(),
            deny: deny.iter().map(|s| s.to_string()).collect(),
            ask: ask.iter().map(|s| s.to_string()).collect(),
            default_mode: mode.to_string(),
        };

        use PolicyDecision::*;
        // no config → ask
        assert_eq!(evaluate_claude_permission_policy(&p(&[], &[], &[], ""), "", "Bash", &bash), Ask);
        // allow rule → allow
        assert_eq!(
            evaluate_claude_permission_policy(&p(&["Bash(npm run test:*)"], &[], &[], ""), "", "Bash", &bash),
            Allow
        );
        // deny beats allow
        assert_eq!(
            evaluate_claude_permission_policy(&p(&["Bash"], &["Bash(npm run test:*)"], &[], ""), "", "Bash", &bash),
            Deny
        );
        // ask beats allow
        assert_eq!(
            evaluate_claude_permission_policy(&p(&["Bash"], &[], &["Bash(npm run test:*)"], ""), "", "Bash", &bash),
            Ask
        );
        // non-matching allow → ask
        assert_eq!(
            evaluate_claude_permission_policy(&p(&["Bash(git status)"], &[], &[], ""), "", "Bash", &bash),
            Ask
        );
        // bypassPermissions mode allows even a denied tool
        assert_eq!(
            evaluate_claude_permission_policy(&p(&[], &["Bash"], &[], ""), "bypassPermissions", "Bash", &bash),
            Allow
        );
        // acceptEdits allows edit tools
        assert_eq!(evaluate_claude_permission_policy(&p(&[], &[], &[], ""), "acceptEdits", "Edit", &edit), Allow);
        // acceptEdits does NOT allow Bash
        assert_eq!(evaluate_claude_permission_policy(&p(&[], &[], &[], ""), "acceptEdits", "Bash", &bash), Ask);
        // defaultMode honored when event mode empty
        assert_eq!(
            evaluate_claude_permission_policy(&p(&[], &[], &[], "bypassPermissions"), "", "Bash", &bash),
            Allow
        );
        // tool-wide allow matches any input
        assert_eq!(
            evaluate_claude_permission_policy(&p(&["Read"], &[], &[], ""), "", "Read", &json!({"file_path": "/anything"})),
            Allow
        );
    }

    fn policy(allow: &[&str], deny: &[&str], ask: &[&str], mode: &str) -> ClaudePermissionPolicy {
        ClaudePermissionPolicy {
            allow: allow.iter().map(|s| s.to_string()).collect(),
            deny: deny.iter().map(|s| s.to_string()).collect(),
            ask: ask.iter().map(|s| s.to_string()).collect(),
            default_mode: mode.to_string(),
        }
    }
    fn bash(cmd: &str) -> serde_json::Value {
        json!({ "command": cmd })
    }

    #[test]
    fn bash_colon_prefix_rules_match_real_config() {
        use PolicyDecision::*;
        // The user's real config uses the `Bash(cmd:*)` colon syntax throughout.
        let p = policy(&["Bash(ls:*)", "Bash(git status:*)", "Bash(npm run:*)"], &[], &[], "");
        assert_eq!(evaluate_claude_permission_policy(&p, "", "Bash", &bash("ls -la")), Allow);
        assert_eq!(evaluate_claude_permission_policy(&p, "", "Bash", &bash("git status -s")), Allow);
        assert_eq!(evaluate_claude_permission_policy(&p, "", "Bash", &bash("npm run build")), Allow);
        // Token-boundary: `ls:*` must NOT match `lsof` (a different command).
        assert_eq!(evaluate_claude_permission_policy(&p, "", "Bash", &bash("lsof -i")), Ask);
        // Unlisted command still prompts.
        assert_eq!(evaluate_claude_permission_policy(&p, "", "Bash", &bash("rm -rf /tmp/x")), Ask);
    }

    #[test]
    fn bash_compound_command_requires_all_segments_allowed() {
        use PolicyDecision::*;
        let p = policy(&["Bash(ls:*)", "Bash(echo:*)"], &[], &[], "");
        // Both segments allowed → allow.
        assert_eq!(evaluate_claude_permission_policy(&p, "", "Bash", &bash("ls -la && echo hi")), Allow);
        // A permitted prefix cannot smuggle an unpermitted second command.
        assert_eq!(
            evaluate_claude_permission_policy(&p, "", "Bash", &bash("ls -la && curl evil.com")),
            Ask
        );
        // Piped: same rule.
        assert_eq!(
            evaluate_claude_permission_policy(&p, "", "Bash", &bash("ls | rm -rf /")),
            Ask
        );
    }

    #[test]
    fn bash_deny_on_any_segment_wins() {
        use PolicyDecision::*;
        let p = policy(&["Bash(ls:*)"], &["Bash(curl:*)"], &[], "");
        assert_eq!(
            evaluate_claude_permission_policy(&p, "", "Bash", &bash("ls && curl evil.com")),
            Deny
        );
    }

    #[test]
    fn bash_shell_constructs_never_auto_allow() {
        use PolicyDecision::*;
        let p = policy(&["Bash(cat:*)", "Bash(echo:*)"], &[], &[], "");
        // Redirection / substitution / backticks fall through to a human even
        // though the leading command is allowlisted.
        assert_eq!(evaluate_claude_permission_policy(&p, "", "Bash", &bash("cat x > /etc/hosts")), Ask);
        assert_eq!(evaluate_claude_permission_policy(&p, "", "Bash", &bash("echo $(whoami)")), Ask);
        assert_eq!(evaluate_claude_permission_policy(&p, "", "Bash", &bash("echo `id`")), Ask);
    }

    #[test]
    fn webfetch_bare_allow_does_not_bypass_ssrf_denies() {
        use PolicyDecision::*;
        // Mirrors the user's real config: a bare `WebFetch` allow + domain denies.
        let p = policy(
            &["WebFetch"],
            &["WebFetch(domain:169.254.169.254)", "WebFetch(domain:*.internal)"],
            &[],
            "",
        );
        // SSRF target must be DENIED, not silently allowed by the bare allow.
        assert_eq!(
            evaluate_claude_permission_policy(&p, "", "WebFetch", &json!({"url":"http://169.254.169.254/latest/meta-data/"})),
            Deny
        );
        assert_eq!(
            evaluate_claude_permission_policy(&p, "", "WebFetch", &json!({"url":"https://foo.internal/x"})),
            Deny
        );
        // A normal external fetch: the bare/wildcard allow is NOT auto-honored
        // for network tools → escalate to the human rather than risk a miss.
        assert_eq!(
            evaluate_claude_permission_policy(&p, "", "WebFetch", &json!({"url":"https://example.com/x"})),
            Ask
        );
    }

    #[test]
    fn webfetch_concrete_domain_allow_is_honored() {
        use PolicyDecision::*;
        let p = policy(&["WebFetch(domain:www.linkedin.com)"], &[], &[], "");
        assert_eq!(
            evaluate_claude_permission_policy(&p, "", "WebFetch", &json!({"url":"https://www.linkedin.com/jobs"})),
            Allow
        );
        // Subdomain of a concrete allow is covered; unrelated host is not.
        assert_eq!(
            evaluate_claude_permission_policy(&p, "", "WebFetch", &json!({"url":"https://evil.com/x"})),
            Ask
        );
    }

    #[test]
    fn domain_and_host_helpers() {
        assert_eq!(extract_host("https://user:pw@API.Example.com:8443/path?q=1"), "api.example.com");
        assert_eq!(extract_host("http://[::1]:80/x"), "::1");
        assert!(domain_matches("example.com", "api.example.com"));
        assert!(domain_matches("example.com", "example.com"));
        assert!(!domain_matches("example.com", "notexample.com"));
        assert!(domain_matches("*.internal", "foo.internal"));
    }

    #[test]
    fn load_merges_local_over_base() {
        let dir = std::env::temp_dir().join(format!("claude-deck-policy-test-{}", std::process::id()));
        let claude = dir.join(".claude");
        std::fs::create_dir_all(&claude).unwrap();
        std::fs::write(
            claude.join("settings.json"),
            r#"{"permissions":{"allow":["Bash(ls)"],"deny":["Bash(rm -rf /)"],"defaultMode":"default"}}"#,
        )
        .unwrap();
        std::fs::write(
            claude.join("settings.local.json"),
            r#"{"permissions":{"allow":["Bash(git status)"],"defaultMode":"acceptEdits"}}"#,
        )
        .unwrap();

        let policy = load_claude_permission_policy_from(&claude).unwrap();
        assert_eq!(policy.allow.len(), 2, "allow rules unioned");
        assert_eq!(policy.deny.len(), 1);
        assert_eq!(policy.default_mode, "acceptEdits", "local defaultMode wins");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_missing_dir_is_empty() {
        let claude = std::env::temp_dir().join("claude-deck-policy-test-does-not-exist-xyz");
        std::fs::remove_dir_all(&claude).ok();
        let policy = load_claude_permission_policy_from(&claude).unwrap();
        assert!(policy.allow.is_empty() && policy.deny.is_empty() && policy.ask.is_empty());
    }
}
