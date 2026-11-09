// SPDX-License-Identifier: MIT
// SigmaOS-bash-completion - Programmable Completion System component
// bash-completion-style shell completion for SigmaOS,
// inspired by Arch's bash-completion package.

use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

// ============================================================================
// COMPLETION SPECIFICATION
// ============================================================================

/// A completion specification for a command
#[derive(Debug, Clone)]
pub struct CompletionSpec {
    /// Command name (e.g. "git", "ssh")
    pub command: String,
    /// Subcommands with their own completions
    pub subcommands: HashMap<String, CompletionSpec>,
    /// Options/flags
    pub options: Vec<CompletionOption>,
    /// Arguments completion source
    pub args_source: ArgsSource,
    /// Whether command takes file arguments
    pub file_args: bool,
    /// Whether command takes directory arguments
    pub dir_args: bool,
}

impl CompletionSpec {
    pub fn new(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            subcommands: HashMap::new(),
            options: Vec::new(),
            args_source: ArgsSource::None,
            file_args: true,
            dir_args: false,
        }
    }

    /// Add an option
    pub fn option(mut self, short: Option<char>, long: Option<&str>, desc: &str) -> Self {
        self.options.push(CompletionOption {
            short,
            long: long.map(|s| s.to_string()),
            description: desc.to_string(),
            takes_arg: false,
            arg_hint: None,
        });
        self
    }

    /// Add an option that takes an argument
    pub fn option_with_arg(mut self, short: Option<char>, long: Option<&str>, desc: &str, arg_hint: &str) -> Self {
        self.options.push(CompletionOption {
            short,
            long: long.map(|s| s.to_string()),
            description: desc.to_string(),
            takes_arg: true,
            arg_hint: Some(arg_hint.to_string()),
        });
        self
    }

    /// Add a subcommand
    pub fn subcommand(mut self, name: impl Into<String>, spec: CompletionSpec) -> Self {
        self.subcommands.insert(name.into(), spec);
        self
    }

    /// Set arguments source
    pub fn args(mut self, source: ArgsSource) -> Self {
        self.args_source = source;
        self
    }

    /// Whether this spec handles a given word position
    pub fn handles(&self, words: &[String]) -> bool {
        if words.is_empty() {
            return false;
        }
        // First word should match command name
        if words[0] != self.command {
            return false;
        }
        // If we have subcommands, check if second word matches
        if words.len() > 1 && !self.subcommands.is_empty() {
            if self.subcommands.contains_key(&words[1]) {
                return true;
            }
        }
        true
    }
}

/// A command-line option
#[derive(Debug, Clone)]
pub struct CompletionOption {
    pub short: Option<char>,
    pub long: Option<String>,
    pub description: String,
    pub takes_arg: bool,
    pub arg_hint: Option<String>,
}

impl CompletionOption {
    /// All possible flag forms for this option
    pub fn forms(&self) -> Vec<String> {
        let mut forms = Vec::new();
        if let Some(s) = self.short {
            forms.push(format!("-{}", s));
        }
        if let Some(l) = &self.long {
            forms.push(format!("--{}", l));
        }
        forms
    }

    /// Whether this option matches a given word
    pub fn matches(&self, word: &str) -> bool {
        if let Some(s) = self.short {
            if word == format!("-{}", s) { return true; }
        }
        if let Some(l) = &self.long {
            let full = format!("--{}", l);
            if word == full || (word.len() < full.len() && full.starts_with(word)) { return true; }
        }
        false
    }
}

/// Source for argument completions
#[derive(Debug, Clone)]
pub enum ArgsSource {
    /// No argument completion
    None,
    /// File paths
    Files,
    /// Directory paths
    Directories,
    /// Commands from PATH
    Commands,
    /// Environment variables
    EnvVars,
    /// Custom function
    Custom(String),
    /// Static list of values
    Static(Vec<String>),
}

impl ArgsSource {
    /// Get completion candidates for this source
    pub fn candidates(&self, prefix: &str) -> Vec<String> {
        match self {
            Self::None => Vec::new(),
            Self::Files | Self::Directories => {
                // In real impl, glob filesystem; here return empty
                Vec::new()
            }
            Self::Commands => {
                // In real impl, search PATH; here return empty
                Vec::new()
            }
            Self::EnvVars => {
                std::env::vars()
                    .map(|(k, _)| k)
                    .filter(|k| k.starts_with(prefix))
                    .collect()
            }
            Self::Custom(_) => Vec::new(),
            Self::Static(list) => {
                list.iter()
                    .filter(|v| v.starts_with(prefix))
                    .cloned()
                    .collect()
            }
        }
    }
}

// ============================================================================
// COMPLETION ENGINE
// ============================================================================

/// The completion engine
pub struct CompletionEngine {
    /// Registered completion specs
    specs: HashMap<String, CompletionSpec>,
    /// Whether to show descriptions
    pub show_descriptions: bool,
    /// Whether to do fuzzy matching
    pub fuzzy: bool,
}

impl CompletionEngine {
    pub fn new() -> Self {
        Self {
            specs: HashMap::new(),
            show_descriptions: true,
            fuzzy: false,
        }
    }

    /// Register a completion spec
    pub fn register(&mut self, spec: CompletionSpec) {
        self.specs.insert(spec.command.clone(), spec);
    }

    /// Register multiple specs
    pub fn register_all(&mut self, specs: Vec<CompletionSpec>) {
        for spec in specs {
            self.register(spec);
        }
    }

    /// Find spec for a command line
    fn find_spec(&self, words: &[String]) -> Option<&CompletionSpec> {
        if words.is_empty() { return None; }
        self.specs.get(&words[0])
    }

    /// Generate completions for a command line
    pub fn complete(&self, line: &str, cursor_pos: usize) -> Vec<Completion> {
        let words: Vec<String> = line.split_whitespace().map(|s| s.to_string()).collect();
        if words.is_empty() {
            return Vec::new();
        }

        let spec = match self.find_spec(&words) {
            Some(s) => s,
            None => return self.complete_command_name(&words.last().unwrap_or(&String::new())),
        };

        // If we're completing a subcommand (or first arg after command)
        if (words.len() == 1 || words.len() == 2) && !spec.subcommands.is_empty() {
            let prefix = if words.len() == 1 { "" } else { words[1].as_str() };
            let subs: Vec<Completion> = spec.subcommands.keys()
                .filter(|k| k.starts_with(prefix))
                .map(|k| Completion {
                    text: k.clone(),
                    description: format!("{} subcommand", words[0]),
                    kind: CompletionKind::Subcommand,
                })
                .collect();
            if !subs.is_empty() && !prefix.starts_with('-') {
                return subs;
            }
        }

        // Get the word being completed (last word)
        let current = words.last().cloned().unwrap_or_default();

        // If current word starts with -, complete options
        if current.starts_with('-') {
            return self.complete_options(spec, &current);
        }

        // Complete arguments
        self.complete_args(spec, &current)
    }

    /// Complete command names (first word)
    fn complete_command_name(&self, prefix: &str) -> Vec<Completion> {
        self.specs.keys()
            .filter(|k| k.starts_with(prefix))
            .map(|k| Completion {
                text: k.clone(),
                description: "command".to_string(),
                kind: CompletionKind::Command,
            })
            .collect()
    }

    /// Complete options for a spec
    fn complete_options(&self, spec: &CompletionSpec, prefix: &str) -> Vec<Completion> {
        let mut completions = Vec::new();
        for opt in &spec.options {
            for form in opt.forms() {
                if form.starts_with(prefix) {
                    completions.push(Completion {
                        text: form,
                        description: opt.description.clone(),
                        kind: CompletionKind::Option,
                    });
                }
            }
        }
        completions
    }

    /// Complete arguments
    fn complete_args(&self, spec: &CompletionSpec, prefix: &str) -> Vec<Completion> {
        match &spec.args_source {
            ArgsSource::Static(list) => {
                list.iter()
                    .filter(|v| v.starts_with(prefix))
                    .map(|v| Completion {
                        text: v.clone(),
                        description: "argument".to_string(),
                        kind: CompletionKind::Argument,
                    })
                    .collect()
            }
            ArgsSource::EnvVars => {
                std::env::vars()
                    .map(|(k, _)| k)
                    .filter(|k| k.starts_with(prefix))
                    .map(|k| Completion {
                        text: k,
                        description: "environment variable".to_string(),
                        kind: CompletionKind::EnvVar,
                    })
                    .collect()
            }
            _ => Vec::new(),
        }
    }

    /// List all registered commands
    pub fn commands(&self) -> Vec<&str> {
        self.specs.keys().map(|s| s.as_str()).collect()
    }

    /// Get spec for a command
    pub fn get_spec(&self, command: &str) -> Option<&CompletionSpec> {
        self.specs.get(command)
    }

    /// Load completion specs from a directory
    pub fn load_from_dir(&mut self, dir: &Path) -> Result<usize, CompletionError> {
        let mut count = 0;
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map(|e| e == "completion").unwrap_or(false) {
                    // In real impl, parse the completion file
                    count += 1;
                }
            }
        }
        Ok(count)
    }
}

impl Default for CompletionEngine {
    fn default() -> Self { Self::new() }
}

/// A single completion candidate
#[derive(Debug, Clone)]
pub struct Completion {
    /// Text to insert
    pub text: String,
    /// Description shown in menu
    pub description: String,
    /// Type of completion
    pub kind: CompletionKind,
}

/// Kind of completion
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionKind {
    Command,
    Subcommand,
    Option,
    Argument,
    File,
    Directory,
    EnvVar,
}

#[derive(Debug)]
pub enum CompletionError {
    Io(String),
    Parse(String),
}

impl fmt::Display for CompletionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(m) => write!(f, "io error: {}", m),
            Self::Parse(m) => write!(f, "parse error: {}", m),
        }
    }
}
impl std::error::Error for CompletionError {}

// ============================================================================
// BUILT-IN COMPLETIONS
// ============================================================================

/// Create default completion specs for common commands
pub fn default_specs() -> Vec<CompletionSpec> {
    vec![
        CompletionSpec::new("git")
            .option(Some('C'), Some("git-dir"), "Run as if started in <path>")
            .option(Some('c'), Some("config-env"), "Set config env")
            .option(None, Some("version"), "Show version")
            .option(None, Some("help"), "Show help")
            .subcommand("clone", CompletionSpec::new("clone")
                .option(Some('b'), Some("branch"), "Checkout <branch>")
                .option(Some('o'), Some("origin"), "Remote name")
                .option(Some('j'), Some("jobs"), "Number of jobs")
                .option(None, Some("depth"), "Create shallow clone")
                .args(ArgsSource::Directories))
            .subcommand("checkout", CompletionSpec::new("checkout")
                .option(Some('b'), Some("branch"), "Create branch")
                .option(None, Some("detach"), "Detach HEAD")
                .args(ArgsSource::Static(vec!["main".into(), "master".into(), "develop".into()])))
            .subcommand("commit", CompletionSpec::new("commit")
                .option(Some('m'), Some("message"), "Commit message")
                .option(Some('a'), Some("all"), "Stage all")
                .option(Some('s'), Some("signoff"), "Add Signed-off-by")
                .option(None, Some("amend"), "Amend previous commit"))
            .subcommand("push", CompletionSpec::new("push")
                .option(Some('u'), Some("set-upstream"), "Set upstream")
                .option(Some('f'), Some("force"), "Force push")
                .args(ArgsSource::Commands))
            .subcommand("pull", CompletionSpec::new("pull")
                .option(Some('r'), Some("rebase"), "Rebase")
                .option(None, Some("ff-only"), "Fast-forward only"))
            .subcommand("status", CompletionSpec::new("status")
                .option(Some('s'), Some("short"), "Short format")
                .option(None, Some("porcelain"), "Porcelain format")),
        CompletionSpec::new("ssh")
            .option(Some('p'), Some("port"), "Port to connect to")
            .option(Some('i'), Some("identity"), "Identity file")
            .option(Some('v'), Some("verbose"), "Verbose mode")
            .option(Some('N'), Some("no-agent"), "Disable agent")
            .option(None, Some("compress"), "Enable compression")
            .args(ArgsSource::Commands),
        CompletionSpec::new("systemctl")
            .option(None, Some("type"), "Unit type")
            .option(None, Some("state"), "Unit state")
            .option(Some('a'), Some("all"), "Show all units")
            .option(Some('r'), Some("recursive"), "Show recursively")
            .args(ArgsSource::Static(vec![
                "start".into(), "stop".into(), "restart".into(), "status".into(),
                "enable".into(), "disable".into(), "is-active".into(), "is-enabled".into(),
            ])),
        CompletionSpec::new("pacman")
            .option(Some('S'), Some("sync"), "Synchronize")
            .option(Some('R'), Some("remove"), "Remove")
            .option(Some('Q'), Some("query"), "Query")
            .option(Some('U'), Some("upgrade"), "Upgrade")
            .option(None, Some("noconfirm"), "No confirmation")
            .option(None, Some("needed"), "Only if needed")
            .args(ArgsSource::Static(vec![
                "-Syu".into(), "-S".into(), "-R".into(), "-Q".into(), "-Ss".into(),
            ])),
    ]
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_completion() {
        let mut engine = CompletionEngine::new();
        engine.register_all(default_specs());

        let completions = engine.complete("git ", 4);
        assert!(completions.iter().any(|c| c.text == "clone"));
        assert!(completions.iter().any(|c| c.text == "commit"));
    }

    #[test]
    fn test_option_completion() {
        let mut engine = CompletionEngine::new();
        engine.register_all(default_specs());

        let completions = engine.complete("git --", 6);
        assert!(completions.iter().any(|c| c.text == "--version"));
        assert!(completions.iter().any(|c| c.text == "--help"));
    }

    #[test]
    fn test_command_name_completion() {
        let mut engine = CompletionEngine::new();
        engine.register_all(default_specs());

        let completions = engine.complete("gi", 2);
        assert!(completions.iter().any(|c| c.text == "git"));
    }

    #[test]
    fn test_subcommand_completion() {
        let mut engine = CompletionEngine::new();
        engine.register_all(default_specs());

        let completions = engine.complete("git cl", 6);
        assert!(completions.iter().any(|c| c.text == "clone"));
    }

    #[test]
    fn test_spec_registration() {
        let mut engine = CompletionEngine::new();
        engine.register(CompletionSpec::new("test"));
        assert!(engine.get_spec("test").is_some());
        assert!(engine.commands().contains(&"test"));
    }

    #[test]
    fn test_option_forms() {
        let opt = CompletionOption {
            short: Some('v'),
            long: Some("verbose".into()),
            description: "verbose".into(),
            takes_arg: false,
            arg_hint: None,
        };
        assert_eq!(opt.forms(), vec!["-v", "--verbose"]);
        assert!(opt.matches("-v"));
        assert!(opt.matches("--verbose"));
        assert!(opt.matches("--verb")); // prefix match
        assert!(!opt.matches("-x"));
    }

    #[test]
    fn test_args_source_static() {
        let source = ArgsSource::Static(vec!["start".into(), "stop".into(), "restart".into()]);
        let candidates = source.candidates("st");
        assert_eq!(candidates, vec!["start".to_string(), "stop".to_string()]);
    }
}
