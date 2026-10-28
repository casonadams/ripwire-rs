use clap::{Args, Parser};

/// Root command description.
pub const ABOUT: &str = "High-throughput agent context router and static analysis engine";

/// CLI command line options.
#[derive(Parser, Debug, Default, PartialEq)]
#[command(name = "ripwire-rs", version, about = ABOUT)]
pub struct Cli {
    /// Target repository path (default: current directory)
    #[arg(value_name = "PATH")]
    pub path: Option<String>,
    /// Ranking signal: pagerank, authority, hub, degree, ccx, stable
    #[arg(long, default_value = "pagerank")]
    pub rank_by: String,
    /// Streaming budget in bytes (default 65536)
    #[arg(
        long = "pack-budget",
        visible_alias = "pack-budget-bytes",
        value_name = "BYTES"
    )]
    pub pack_budget: Option<usize>,
    /// Token budget (~4 bytes per token, alternative to --pack-budget)
    #[arg(long)]
    pub token_budget: Option<usize>,
    /// Budget the map to ~N tokens (~4 bytes per token, alias for --token-budget)
    #[arg(long = "max-tokens", value_name = "N")]
    pub max_tokens: Option<usize>,
    /// Keep only the N highest-ranked symbols (e.g. 200)
    #[arg(long = "top-k", value_name = "N")]
    pub top_k: Option<usize>,
    /// Schema legend posture: compact (default, in-context schema comment) or none
    #[arg(long, default_value = "compact")]
    pub legend: String,
    /// Overlay SCIP compiler-backed index
    #[arg(long = "scip", global = true, value_name = "FILE")]
    pub scip: Option<String>,

    /// Parsed values of command-plugin flags
    #[arg(skip)]
    pub plugins: PluginArgs,
    /// Top-level global modifiers and batching flags
    #[command(flatten)]
    pub modifiers: TopModifierArgs,
}

/// Top-level global modifiers and batching flags.
#[derive(Args, Debug, Clone, Default, PartialEq)]
pub struct TopModifierArgs {
    /// Emit JSON instead of XML
    #[arg(long)]
    pub json: bool,
    /// Paginate: maximum number of items to emit
    #[arg(long, value_name = "N")]
    pub limit: Option<usize>,
    /// Paginate: number of items to skip
    #[arg(long, value_name = "M")]
    pub offset: Option<usize>,
    /// Explicit cache file path
    #[arg(long, value_name = "PATH")]
    pub cache: Option<String>,
    /// Disable on-disk binary auto-caching (force cold parse)
    #[arg(long = "no-cache", overrides_with = "no_cache")]
    pub no_cache: bool,
    /// Select output format (e.g. candidates, columnar, json)
    #[arg(long = "format", value_name = "FORMAT")]
    pub format: Option<String>,
    /// Exclude glob or directory pattern from indexing (repeatable; comma-separated lists accepted)
    #[arg(long = "exclude", value_name = "GLOB", action = clap::ArgAction::Append)]
    pub exclude: Vec<String>,
    /// Do not respect .gitignore rules during crawling
    #[arg(long = "no-ignore", overrides_with = "no_ignore")]
    pub no_ignore: bool,
    /// Enclosing symbol or directory filter
    #[arg(long = "in", value_name = "SCOPE")]
    pub in_scope: Option<String>,
    /// Let --doc-drift and --whereis consult git history for deleted names
    #[arg(long = "with-history")]
    pub with_history: bool,
    /// Exclude test files and directories from analysis
    #[arg(long = "ignore-tests", overrides_with = "ignore_tests")]
    pub ignore_tests: bool,
    /// Maximum file size in bytes to index
    #[arg(long = "max-file-size", value_name = "BYTES")]
    pub max_file_size: Option<u64>,
    /// Disable secret redaction on emitted snippets
    #[arg(long = "no-redact", overrides_with = "no_redact")]
    pub no_redact: bool,
    /// Explicit RSS memory guard ceiling (e.g. 512M, 2G)
    #[arg(long = "max-memory", value_name = "N[K|M|G]")]
    pub max_memory: Option<String>,
    /// Target agent profile for output shaping
    #[arg(long = "agent", value_name = "AGENT")]
    pub agent: Option<String>,
    /// Include call-graph edges in task/retrieval output
    #[arg(long = "with-graph")]
    pub with_graph: bool,
    /// Annotate symbols with runtime profile weights
    #[arg(long = "with-profile", value_name = "FILE")]
    pub with_profile: Option<String>,
    /// Execute runtime trace command and map onto symbols
    #[arg(long = "run-trace", value_name = "CMD")]
    pub run_trace: Option<String>,
    /// Timeout in seconds for --run-trace execution
    #[arg(long = "run-timeout", value_name = "SEC")]
    pub run_timeout: Option<u64>,
    /// Pretty-print indented XML (default is minified token-optimized XML)
    #[arg(long = "pretty")]
    pub pretty: bool,
    /// Limit transitive impact traversal to N call hops
    #[arg(long = "impact-depth", global = true, value_name = "N")]
    pub impact_depth: Option<u32>,
    /// Compress output using compact token dialect
    #[arg(long = "compress", overrides_with = "compress")]
    pub compress: bool,
    /// Goal or task description for --plan-lanes
    #[arg(long = "task", value_name = "GOAL")]
    pub task: Option<String>,
    /// Span tier a --grep hit must sit in: code (default) or any
    #[arg(long = "grep-in", value_name = "code|any")]
    pub grep_in: Option<String>,
    /// Where --and/--not terms must appear: line (default) or file
    #[arg(long = "grep-scope", value_name = "line|file")]
    pub grep_scope: Option<String>,
    /// Keep only --grep hits where PAT is also present (repeatable)
    #[arg(long = "and", value_name = "PAT")]
    pub and_pat: Vec<String>,
    /// Drop --grep hits where PAT is present (repeatable)
    #[arg(long = "not", value_name = "PAT")]
    pub not_pat: Vec<String>,
    /// Lines before grep hit
    #[arg(long = "grep-before", value_name = "N")]
    pub grep_before: Option<usize>,
    /// Lines after grep hit
    #[arg(long = "grep-after", value_name = "N")]
    pub grep_after: Option<usize>,
    /// Lines around grep hit
    #[arg(long = "grep-context", value_name = "N")]
    pub grep_context: Option<usize>,
}
impl TopModifierArgs {
    pub fn is_pretty(&self) -> bool {
        self.pretty || self.format.as_deref() == Some("pretty")
    }

    pub fn is_json(&self) -> bool {
        self.json || self.format.as_deref() == Some("json")
    }
}
pub fn compute_effective_budget(
    pack_budget: Option<usize>,
    token_budget: Option<usize>,
    max_tokens: Option<usize>,
) -> Option<usize> {
    token_budget
        .or(max_tokens)
        .map(crate::query::map::token_budget_bytes)
        .or(pack_budget)
        .or(Some(crate::serialize::DEFAULT_PACK_BUDGET_BYTES))
}

impl Cli {
    /// Byte budget for the map; `None` when only `--top-k` is given, so the map is not budgeted.
    #[must_use]
    pub fn effective_budget_bytes(&self) -> Option<usize> {
        if self.top_k.is_some()
            && self.pack_budget.is_none()
            && self.token_budget.is_none()
            && self.max_tokens.is_none()
        {
            None
        } else {
            compute_effective_budget(self.pack_budget, self.token_budget, self.max_tokens)
        }
    }
}

/// Matches for the flags command plugins own; plugins decode their own typed `Args` from it.
#[derive(Debug, Clone, Default)]
pub struct PluginArgs(Option<clap::ArgMatches>);

impl PartialEq for PluginArgs {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl PluginArgs {
    #[must_use]
    pub fn new(matches: clap::ArgMatches) -> Self {
        Self(Some(matches))
    }

    /// The plugin's parsed flags, `None` when the command line was not parsed with that plugin.
    #[must_use]
    pub fn decode<A: clap::FromArgMatches>(&self) -> Option<A> {
        A::from_arg_matches(self.0.as_ref()?).ok()
    }

    /// The subcommand the command line selected, with its own matches.
    #[must_use]
    pub fn subcommand(&self) -> Option<(&str, &clap::ArgMatches)> {
        self.0.as_ref()?.subcommand()
    }
}
