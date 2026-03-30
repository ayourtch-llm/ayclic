//! Data-driven command tree for Cisco IOS CLI simulation.
//!
//! Provides prefix (abbreviation) matching, ambiguous command detection,
//! correct error classification, and `?` help support.

use crate::{CliMode, MockIosDevice};

/// How a token in the command line is matched.
#[derive(Debug, Clone)]
pub enum TokenMatcher {
    /// A keyword — matched by unique prefix (e.g., "sh" matches "show").
    Keyword(String),
    /// A parameter placeholder — matches a value of the given type.
    Param { name: String, param_type: ParamType },
}

/// Types of parameter values.
#[derive(Debug, Clone, PartialEq)]
pub enum ParamType {
    /// Any single word (non-empty).
    Word,
    /// An integer.
    Number,
    /// Rest of line (greedy — consumes all remaining tokens).
    RestOfLine,
}

impl ParamType {
    pub fn matches(&self, token: &str) -> bool {
        match self {
            ParamType::Word => !token.is_empty(),
            ParamType::Number => token.parse::<i64>().is_ok(),
            ParamType::RestOfLine => !token.is_empty(),
        }
    }
}

/// Handler function signature.
/// Receives the device and the full original input line.
pub type CmdHandler = fn(&mut MockIosDevice, input: &str);

/// A node in the command tree.
#[derive(Clone)]
pub struct CommandNode {
    pub matcher: TokenMatcher,
    pub help: String,
    pub children: Vec<CommandNode>,
    pub handler: Option<CmdHandler>,
    /// Handler invoked only when the command is negated (prefixed with `no`).
    /// This allows commands where the affirmative form requires arguments but
    /// the negated form does not (e.g., `hostname <name>` vs `no hostname`).
    /// When set, `<cr>` is NOT shown in positive-form help output.
    pub no_handler: Option<CmdHandler>,
    pub mode_filter: ModeFilter,
}

/// Which CLI modes a command node is visible in.
#[derive(Debug, Clone)]
pub enum ModeFilter {
    /// Available in all modes where the parent tree applies.
    Any,
    /// Only in these specific mode classes.
    Only(Vec<CliModeClass>),
}

/// Simplified mode classification for filtering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliModeClass {
    UserExec,
    PrivExec,
    Config,
    ConfigSub,
}

impl CliModeClass {
    pub fn from_cli_mode(mode: &CliMode) -> Self {
        match mode {
            CliMode::UserExec => CliModeClass::UserExec,
            CliMode::PrivilegedExec => CliModeClass::PrivExec,
            CliMode::Config => CliModeClass::Config,
            CliMode::ConfigSub(_) => CliModeClass::ConfigSub,
            _ => panic!("not a command mode: {:?}", mode),
        }
    }
}

impl ModeFilter {
    pub fn matches(&self, mode: &CliMode) -> bool {
        match self {
            ModeFilter::Any => true,
            ModeFilter::Only(classes) => classes.contains(&CliModeClass::from_cli_mode(mode)),
        }
    }
}

/// Result of parsing a command line.
pub enum ParseResult {
    /// Command matched — execute handler with full original input.
    Execute { handler: CmdHandler, input: String },
    /// Valid prefix but command is incomplete (node has children, no handler).
    Incomplete,
    /// No match at a specific byte position in the input.
    InvalidInput { caret_pos: usize },
    /// Multiple keywords match the given prefix.
    Ambiguous { token: String, matches: Vec<String> },
    /// The input is empty (just whitespace).
    Empty,
}

/// Result of a `?` help query.
#[derive(Debug)]
pub enum HelpResult {
    /// List of (keyword/param-name, help_text) for "show ?" (space before ?).
    Subcommands(Vec<(String, String)>),
    /// List of matching keywords for "sh?" (no space before ?).
    PrefixMatches(Vec<String>),
    /// The path before the `?` is invalid.
    NotFound { caret_pos: usize },
}

// ─── Builder API ─────────────────────────────────────────────────────────────

/// Create a keyword node.
pub fn keyword(name: &str, help: &str) -> CommandNode {
    CommandNode {
        matcher: TokenMatcher::Keyword(name.to_string()),
        help: help.to_string(),
        children: Vec::new(),
        handler: None,
        no_handler: None,
        mode_filter: ModeFilter::Any,
    }
}

/// Create a parameter node.
pub fn param(name: &str, param_type: ParamType, help: &str) -> CommandNode {
    CommandNode {
        matcher: TokenMatcher::Param { name: name.to_string(), param_type },
        help: help.to_string(),
        children: Vec::new(),
        handler: None,
        no_handler: None,
        mode_filter: ModeFilter::Any,
    }
}

impl CommandNode {
    /// Set the handler for this node.
    pub fn handler(mut self, h: CmdHandler) -> Self {
        self.handler = Some(h);
        self
    }

    /// Set the no-form handler. Used when the negated form (e.g., `no hostname`)
    /// doesn't take arguments but the positive form does.
    pub fn no_handler(mut self, h: CmdHandler) -> Self {
        self.no_handler = Some(h);
        self
    }

    /// Set all children at once.
    pub fn children(mut self, c: Vec<CommandNode>) -> Self {
        self.children = c;
        self
    }

    /// Add a single child.
    pub fn child(mut self, c: CommandNode) -> Self {
        self.children.push(c);
        self
    }

    /// Set the mode filter.
    pub fn mode(mut self, m: ModeFilter) -> Self {
        self.mode_filter = m;
        self
    }
}

// ─── Tokenizer ────────────────────────────────────────────────────────────────

/// Tokenize input into (text, byte_offset) pairs.
pub fn tokenize_with_offsets(input: &str) -> Vec<(String, usize)> {
    let mut tokens = Vec::new();
    let bytes = input.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        // Skip whitespace
        while i < len && (bytes[i] == b' ' || bytes[i] == b'\t') {
            i += 1;
        }
        if i >= len {
            break;
        }
        let start = i;
        while i < len && bytes[i] != b' ' && bytes[i] != b'\t' {
            i += 1;
        }
        let token = input[start..i].to_string();
        tokens.push((token, start));
    }

    tokens
}

// ─── find_matches ─────────────────────────────────────────────────────────────

/// Find child nodes that match `token`, filtered by `mode`.
/// Keyword nodes: match if keyword.starts_with(token_lowercase).
/// Param nodes: match if param_type.matches(token).
///
/// Keywords always take priority over params: if any keyword matches,
/// param nodes are excluded from the result (real IOS behavior).
pub fn find_matches<'a>(
    token: &str,
    nodes: &'a [CommandNode],
    mode: &CliMode,
) -> Vec<&'a CommandNode> {
    let token_lower = token.to_lowercase();
    let all_matches: Vec<&'a CommandNode> = nodes
        .iter()
        .filter(|n| n.mode_filter.matches(mode))
        .filter(|n| match &n.matcher {
            TokenMatcher::Keyword(kw) => kw.to_lowercase().starts_with(&token_lower),
            TokenMatcher::Param { param_type, .. } => param_type.matches(token),
        })
        .collect();

    // If any keyword matched, return only keyword matches (keywords beat params).
    let keyword_matches: Vec<&'a CommandNode> = all_matches
        .iter()
        .copied()
        .filter(|n| matches!(&n.matcher, TokenMatcher::Keyword(_)))
        .collect();

    if !keyword_matches.is_empty() {
        // If exactly one keyword is an exact match, prefer it over prefix matches.
        // This is how real IOS works: "ip" is not ambiguous with "ipv6" because
        // "ip" is an exact match for the "ip" keyword.
        let exact_matches: Vec<&'a CommandNode> = keyword_matches
            .iter()
            .copied()
            .filter(|n| match &n.matcher {
                TokenMatcher::Keyword(kw) => kw.to_lowercase() == token_lower,
                _ => false,
            })
            .collect();
        if exact_matches.len() == 1 {
            exact_matches
        } else {
            keyword_matches
        }
    } else {
        all_matches
    }
}

// ─── Parser ───────────────────────────────────────────────────────────────────

/// Parse a command line against a tree of CommandNodes.
/// Returns a ParseResult describing what to do.
pub fn parse(input: &str, tree: &[CommandNode], mode: &CliMode) -> ParseResult {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return ParseResult::Empty;
    }

    let tokens = tokenize_with_offsets(trimmed);
    parse_tokens(&tokens, 0, tree, mode, trimmed)
}

fn parse_tokens(
    tokens: &[(String, usize)],
    idx: usize,
    nodes: &[CommandNode],
    mode: &CliMode,
    original: &str,
) -> ParseResult {
    if idx >= tokens.len() {
        return ParseResult::Empty;
    }

    let (token, offset) = &tokens[idx];
    let matches = find_matches(token, nodes, mode);

    match matches.len() {
        0 => ParseResult::InvalidInput { caret_pos: *offset },
        1 => {
            let node = matches[0];

            // RestOfLine param consumes all remaining tokens
            if let TokenMatcher::Param { param_type: ParamType::RestOfLine, .. } = &node.matcher {
                if let Some(handler) = node.handler {
                    return ParseResult::Execute {
                        handler,
                        input: original.to_string(),
                    };
                }
                // RestOfLine with children (unusual) — just report incomplete
                return ParseResult::Incomplete;
            }

            let is_last = idx + 1 >= tokens.len();

            if is_last {
                if let Some(handler) = node.handler {
                    ParseResult::Execute {
                        handler,
                        input: original.to_string(),
                    }
                } else if !node.children.is_empty() {
                    ParseResult::Incomplete
                } else {
                    // Leaf with no handler — treat as incomplete
                    ParseResult::Incomplete
                }
            } else {
                // More tokens — descend into children
                parse_tokens(tokens, idx + 1, &node.children, mode, original)
            }
        }
        _ => {
            // Multiple matches — ambiguous
            let names: Vec<String> = matches
                .iter()
                .map(|n| match &n.matcher {
                    TokenMatcher::Keyword(kw) => kw.clone(),
                    TokenMatcher::Param { name, .. } => name.clone(),
                })
                .collect();
            ParseResult::Ambiguous {
                token: token.clone(),
                matches: names,
            }
        }
    }
}

// ─── Help ─────────────────────────────────────────────────────────────────────

/// Process a `?` help query.
///
/// - `"show ?"` (ends with space before `?`): list show's visible children.
/// - `"sh?"` (no space before `?`): list visible top-level commands starting with "sh".
pub fn help(input_before_question: &str, tree: &[CommandNode], mode: &CliMode) -> HelpResult {
    let trimmed = input_before_question.trim_end();

    // If input ends with a space (or is empty), list children of the resolved path.
    let ends_with_space = input_before_question.ends_with(' ')
        || input_before_question.is_empty()
        || trimmed.is_empty();

    if ends_with_space {
        // Walk tree for the tokens we have, then list children of the arrived node.
        if trimmed.is_empty() {
            // "?" alone — list top-level nodes
            let subs = visible_children_help(tree, mode);
            return HelpResult::Subcommands(subs);
        }
        let tokens = tokenize_with_offsets(trimmed);
        match resolve_path_with_node(&tokens, 0, tree, mode) {
            Ok((children, parent_has_handler)) => {
                let mut subs = visible_children_help(children, mode);
                // If the resolved node has a handler, the command is already
                // complete — show <cr> like real IOS does.
                if parent_has_handler {
                    subs.push(("<cr>".to_string(), String::new()));
                }
                HelpResult::Subcommands(subs)
            }
            Err(caret_pos) => HelpResult::NotFound { caret_pos },
        }
    } else {
        // No trailing space: partial token. Split into path + partial.
        let tokens = tokenize_with_offsets(trimmed);
        if tokens.is_empty() {
            let subs = visible_children_help(tree, mode);
            return HelpResult::Subcommands(subs);
        }

        let (partial, _partial_offset) = tokens.last().unwrap();
        let path_tokens = &tokens[..tokens.len() - 1];

        let children = if path_tokens.is_empty() {
            tree
        } else {
            match resolve_path(path_tokens, 0, tree, mode) {
                Ok(c) => c,
                Err(caret_pos) => return HelpResult::NotFound { caret_pos },
            }
        };

        let partial_lower = partial.to_lowercase();
        let matches: Vec<String> = children
            .iter()
            .filter(|n| n.mode_filter.matches(mode))
            .filter_map(|n| match &n.matcher {
                TokenMatcher::Keyword(kw) if kw.to_lowercase().starts_with(&partial_lower) => Some(kw.clone()),
                _ => None,
            })
            .collect();

        HelpResult::PrefixMatches(matches)
    }
}

/// Walk the token path through the tree, returning the children of the final matched node.
fn resolve_path<'a>(
    tokens: &[(String, usize)],
    idx: usize,
    nodes: &'a [CommandNode],
    mode: &CliMode,
) -> Result<&'a [CommandNode], usize> {
    resolve_path_with_node(tokens, idx, nodes, mode).map(|(children, _)| children)
}

/// Walk the token path, returning (children, parent_has_handler).
fn resolve_path_with_node<'a>(
    tokens: &[(String, usize)],
    idx: usize,
    nodes: &'a [CommandNode],
    mode: &CliMode,
) -> Result<(&'a [CommandNode], bool), usize> {
    if idx >= tokens.len() {
        return Ok((nodes, false));
    }

    let (token, offset) = &tokens[idx];
    let matches = find_matches(token, nodes, mode);

    match matches.len() {
        0 => Err(*offset),
        1 => {
            if idx + 1 >= tokens.len() {
                Ok((&matches[0].children, matches[0].handler.is_some()))
            } else {
                resolve_path_with_node(tokens, idx + 1, &matches[0].children, mode)
            }
        }
        _ => Err(*offset), // Ambiguous — treat as not found for help
    }
}

fn visible_children_help(nodes: &[CommandNode], mode: &CliMode) -> Vec<(String, String)> {
    let mut result: Vec<(String, String)> = nodes
        .iter()
        .filter(|n| n.mode_filter.matches(mode))
        .map(|n| {
            let name = match &n.matcher {
                TokenMatcher::Keyword(kw) => kw.clone(),
                TokenMatcher::Param { name, .. } => name.clone(),
            };
            (name, n.help.clone())
        })
        .collect();
    result.sort_by(|a, b| a.0.cmp(&b.0));
    result
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Helpers ──────────────────────────────────────────────────────────────

    fn priv_exec_mode() -> CliMode {
        CliMode::PrivilegedExec
    }

    fn user_exec_mode() -> CliMode {
        CliMode::UserExec
    }

    fn config_mode() -> CliMode {
        CliMode::Config
    }

    fn dummy_handler(_device: &mut MockIosDevice, _input: &str) {}

    fn simple_tree() -> Vec<CommandNode> {
        vec![
            keyword("show", "Show info").children(vec![
                keyword("version", "Show version").handler(dummy_handler),
                keyword("running-config", "Show running config").handler(dummy_handler),
                keyword("clock", "Show clock").handler(dummy_handler),
            ]),
            keyword("configure", "Enter config mode")
                .mode(ModeFilter::Only(vec![CliModeClass::PrivExec]))
                .handler(dummy_handler)
                .children(vec![
                    keyword("terminal", "Configure from terminal").handler(dummy_handler),
                ]),
            keyword("enable", "Enable privileged")
                .mode(ModeFilter::Only(vec![CliModeClass::UserExec]))
                .handler(dummy_handler),
            keyword("copy", "Copy files")
                .mode(ModeFilter::Only(vec![CliModeClass::PrivExec]))
                .children(vec![
                    param("<source>", ParamType::Word, "source file").children(vec![
                        param("<dest>", ParamType::Word, "dest file").handler(dummy_handler),
                    ]),
                ]),
            keyword("terminal", "Terminal params").children(vec![
                keyword("length", "Set length").children(vec![
                    param("<number>", ParamType::Number, "line count").handler(dummy_handler),
                ]),
            ]),
        ]
    }

    // ── Keyword matching ─────────────────────────────────────────────────────

    #[test]
    fn test_keyword_exact_match() {
        let tree = simple_tree();
        let mode = priv_exec_mode();
        let matches = find_matches("show", &tree, &mode);
        assert_eq!(matches.len(), 1);
        assert!(matches[0].help.contains("Show info"));
    }

    #[test]
    fn test_keyword_prefix_match() {
        let tree = simple_tree();
        let mode = priv_exec_mode();
        let matches = find_matches("sh", &tree, &mode);
        assert_eq!(matches.len(), 1);
        match &matches[0].matcher {
            TokenMatcher::Keyword(kw) => assert_eq!(kw, "show"),
            _ => panic!("Expected keyword"),
        }
    }

    #[test]
    fn test_keyword_no_match() {
        let tree = simple_tree();
        let mode = priv_exec_mode();
        let matches = find_matches("xyz", &tree, &mode);
        assert_eq!(matches.len(), 0);
    }

    #[test]
    fn test_keyword_ambiguous() {
        // "co" matches both "configure" and "copy"
        let tree = simple_tree();
        let mode = priv_exec_mode();
        let matches = find_matches("co", &tree, &mode);
        assert_eq!(matches.len(), 2, "co should match configure and copy");
    }

    // ── Parse results ────────────────────────────────────────────────────────

    #[test]
    fn test_parse_empty_input() {
        let tree = simple_tree();
        let mode = priv_exec_mode();
        let result = parse("", &tree, &mode);
        assert!(matches!(result, ParseResult::Empty));
    }

    #[test]
    fn test_parse_whitespace_only() {
        let tree = simple_tree();
        let mode = priv_exec_mode();
        let result = parse("   ", &tree, &mode);
        assert!(matches!(result, ParseResult::Empty));
    }

    #[test]
    fn test_parse_execute() {
        let tree = simple_tree();
        let mode = priv_exec_mode();
        let result = parse("show version", &tree, &mode);
        assert!(matches!(result, ParseResult::Execute { .. }));
    }

    #[test]
    fn test_parse_execute_prefix() {
        // "sh ver" should match "show version"
        let tree = simple_tree();
        let mode = priv_exec_mode();
        let result = parse("sh ver", &tree, &mode);
        assert!(
            matches!(result, ParseResult::Execute { .. }),
            "sh ver should execute show version"
        );
    }

    #[test]
    fn test_parse_incomplete() {
        // "show ip" — "show" has children but "ip" has no match → InvalidInput
        // "show" alone — has children but no handler → Incomplete
        let tree = simple_tree();
        let mode = priv_exec_mode();
        let result = parse("show", &tree, &mode);
        assert!(
            matches!(result, ParseResult::Incomplete),
            "show alone should be Incomplete"
        );
    }

    #[test]
    fn test_parse_invalid_input_caret() {
        // "show bogus" — "bogus" doesn't match any child of "show"
        let tree = simple_tree();
        let mode = priv_exec_mode();
        let result = parse("show bogus", &tree, &mode);
        match result {
            ParseResult::InvalidInput { caret_pos } => {
                // "bogus" starts at offset 5 in "show bogus"
                assert_eq!(caret_pos, 5, "caret should be at 'bogus'");
            }
            _ => panic!("Expected InvalidInput"),
        }
    }

    #[test]
    fn test_parse_ambiguous() {
        // "co" matches both configure and copy
        let tree = simple_tree();
        let mode = priv_exec_mode();
        let result = parse("co", &tree, &mode);
        match result {
            ParseResult::Ambiguous { token, matches } => {
                assert_eq!(token, "co");
                assert_eq!(matches.len(), 2);
            }
            _ => panic!("Expected Ambiguous"),
        }
    }

    // ── Param matching ───────────────────────────────────────────────────────

    #[test]
    fn test_param_word_match() {
        assert!(ParamType::Word.matches("anything"));
        assert!(ParamType::Word.matches("flash:file.bin"));
        assert!(!ParamType::Word.matches(""));
    }

    #[test]
    fn test_param_number_match() {
        assert!(ParamType::Number.matches("42"));
        assert!(ParamType::Number.matches("0"));
        assert!(!ParamType::Number.matches("abc"));
        assert!(!ParamType::Number.matches("12x"));
    }

    #[test]
    fn test_parse_rest_of_line() {
        // RestOfLine param should consume all remaining tokens
        let tree = vec![
            keyword("service", "Service config").children(vec![
                param("<rest>", ParamType::RestOfLine, "rest of line").handler(dummy_handler),
            ]),
        ];
        let mode = config_mode();

        let result = parse("service timestamps debug uptime", &tree, &mode);
        assert!(
            matches!(result, ParseResult::Execute { .. }),
            "RestOfLine should consume all tokens"
        );
    }

    // ── Mode filter ──────────────────────────────────────────────────────────

    #[test]
    fn test_mode_filter_priv_only_hidden_in_user() {
        let tree = simple_tree();
        let mode = user_exec_mode();
        // "configure" is priv-only, should not match in user exec
        let matches = find_matches("configure", &tree, &mode);
        assert_eq!(
            matches.len(),
            0,
            "configure should be hidden in user exec"
        );
    }

    #[test]
    fn test_mode_filter_priv_only_visible_in_priv() {
        let tree = simple_tree();
        let mode = priv_exec_mode();
        let matches = find_matches("configure", &tree, &mode);
        assert_eq!(
            matches.len(),
            1,
            "configure should be visible in priv exec"
        );
    }

    #[test]
    fn test_mode_filter_user_only_hidden_in_priv() {
        let tree = simple_tree();
        let mode = priv_exec_mode();
        let matches = find_matches("enable", &tree, &mode);
        assert_eq!(
            matches.len(),
            0,
            "enable should be hidden in priv exec"
        );
    }

    // ── Help ─────────────────────────────────────────────────────────────────

    #[test]
    fn test_help_subcommands() {
        // "show ?" should list show's children
        let tree = simple_tree();
        let mode = priv_exec_mode();
        let result = help("show ", &tree, &mode);
        match result {
            HelpResult::Subcommands(subs) => {
                let names: Vec<&str> = subs.iter().map(|(k, _)| k.as_str()).collect();
                assert!(names.contains(&"version"), "Should list 'version'");
                assert!(names.contains(&"clock"), "Should list 'clock'");
            }
            _ => panic!("Expected Subcommands"),
        }
    }

    #[test]
    fn test_help_prefix_completion() {
        // "sh?" should list top-level commands starting with "sh"
        let tree = simple_tree();
        let mode = priv_exec_mode();
        let result = help("sh", &tree, &mode);
        match result {
            HelpResult::PrefixMatches(names) => {
                assert!(names.contains(&"show".to_string()), "Should match 'show'");
                // Should not match "configure", "copy", etc.
                assert!(!names.contains(&"configure".to_string()));
            }
            _ => panic!("Expected PrefixMatches"),
        }
    }

    #[test]
    fn test_help_top_level() {
        // "?" alone lists all top-level visible commands
        let tree = simple_tree();
        let mode = priv_exec_mode();
        let result = help("", &tree, &mode);
        match result {
            HelpResult::Subcommands(subs) => {
                let names: Vec<&str> = subs.iter().map(|(k, _)| k.as_str()).collect();
                assert!(names.contains(&"show"));
                assert!(names.contains(&"configure"));
                // "enable" is user-only, should be hidden in priv exec
                assert!(!names.contains(&"enable"));
            }
            _ => panic!("Expected Subcommands"),
        }
    }

    // ── Builder API ──────────────────────────────────────────────────────────

    #[test]
    fn test_builder_keyword() {
        let node = keyword("show", "Show info");
        match &node.matcher {
            TokenMatcher::Keyword(kw) => assert_eq!(kw, "show"),
            _ => panic!("Expected Keyword"),
        }
        assert_eq!(node.help, "Show info");
        assert!(node.handler.is_none());
        assert!(node.children.is_empty());
        assert!(matches!(node.mode_filter, ModeFilter::Any));
    }

    #[test]
    fn test_builder_chain() {
        let node = keyword("show", "Show info")
            .handler(dummy_handler)
            .child(keyword("version", "Version").handler(dummy_handler));
        assert!(node.handler.is_some());
        assert_eq!(node.children.len(), 1);
    }

    #[test]
    fn test_builder_mode() {
        let node = keyword("configure", "Enter config")
            .mode(ModeFilter::Only(vec![CliModeClass::PrivExec]));
        match &node.mode_filter {
            ModeFilter::Only(classes) => assert!(classes.contains(&CliModeClass::PrivExec)),
            _ => panic!("Expected Only"),
        }
    }

    // ── Tokenizer ────────────────────────────────────────────────────────────

    #[test]
    fn test_tokenize_with_offsets() {
        let tokens = tokenize_with_offsets("show version");
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0], ("show".to_string(), 0));
        assert_eq!(tokens[1], ("version".to_string(), 5));
    }

    #[test]
    fn test_tokenize_multiple_spaces() {
        let tokens = tokenize_with_offsets("show  ip   route");
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[0].0, "show");
        assert_eq!(tokens[1].0, "ip");
        assert_eq!(tokens[2].0, "route");
        // Offsets
        assert_eq!(tokens[1].1, 6);  // after "show  "
        assert_eq!(tokens[2].1, 11); // after "show  ip   "
    }
}
