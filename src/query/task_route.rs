use crate::domain::{IngestResult, Interner, SymKind};
use crate::serialize::escape::escape_xml_str;
use std::collections::BTreeSet;
use std::io::{self, Write};

struct TaskChoice {
    intent: &'static str,
    skill: &'static str,
    reason: &'static str,
    verb: &'static str,
    command: String,
    score: u32,
}

type LexicalRouteSpec = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    u32,
    &'static [(&'static str, u32)],
);

const LEXICAL_ROUTES: &[LexicalRouteSpec] = &[
    (
        "plan-feature",
        "ripwire-rs-before-you-build",
        "prospective feature planning wording",
        "--pack-task",
        8,
        &[
            ("plan", 5),
            ("implementation", 2),
            ("feature", 4),
            ("scope", 4),
            ("multi-symbol", 4),
            ("before building", 4),
            ("new ", 1),
        ],
    ),
    (
        "reuse-one-symbol",
        "ripwire-rs-before-you-build",
        "about-to-write one-symbol wording",
        "--exemplar",
        8,
        &[
            ("about to write", 8),
            ("one helper", 5),
            ("one function", 5),
            ("one class", 5),
            ("exemplar", 8),
            ("helper", 3),
            ("function", 2),
        ],
    ),
    (
        "write-tests",
        "ripwire-rs-change-check",
        "test-gap wording plus a test-writing action",
        "--test-gate",
        6,
        &[
            ("missing coverage", 8),
            ("no tests", 6),
            ("untested", 6),
            ("regression gate", 6),
            ("run tests", 8),
            ("test gate", 8),
            ("which tests", 6),
        ],
    ),
    (
        "find-clones",
        "ripwire-rs-quality-bar",
        "duplicate or clone detection wording",
        "--clones",
        6,
        &[
            ("find clones", 9),
            ("duplicate", 7),
            ("clones", 7),
            ("clone", 6),
        ],
    ),
    (
        "edit-contract",
        "ripwire-rs-change-check",
        "post-edit caller contract check",
        "--edit-check",
        7,
        &[
            ("edited ", 8),
            ("contract", 8),
            ("changed its signature", 9),
            ("break its callers", 8),
        ],
    ),
    (
        "change-impact",
        "ripwire-rs-change-check",
        "blast radius and transitive impact wording",
        "--impact",
        7,
        &[
            ("blast radius", 9),
            ("what breaks if", 9),
            ("what else has to change", 9),
            ("transitive impact", 8),
        ],
    ),
    (
        "locate-task",
        "ripwire-rs-navigate",
        "code-location or symptom wording",
        "--for",
        7,
        &[
            ("find the code", 8),
            ("locate", 7),
            ("where is", 7),
            ("which file implements", 9),
            ("bug", 4),
            ("crash", 5),
            ("symptom", 5),
            ("wrong output", 5),
        ],
    ),
];

pub fn extract_quoted_task_literal(task: &str) -> Option<&str> {
    if let Some(start) = task.find('"')
        && let Some(end) = task[start + 1..].find('"')
    {
        return Some(&task[start + 1..start + 1 + end]);
    }
    let mut rest = task;
    while let Some(start) = rest.find('\'') {
        let is_possessive = rest[..start]
            .chars()
            .next_back()
            .is_some_and(char::is_alphanumeric);
        let after = &rest[start + 1..];
        if !is_possessive
            && let Some(end) = after.find('\'')
            && !after[..end].is_empty()
            && !after[..end].contains(' ')
        {
            return Some(&after[..end]);
        }
        rest = after;
    }
    None
}

fn is_closed_claim_task(trimmed: &str) -> bool {
    trimmed.ends_with(')')
        && [
            "calls(",
            "uses(",
            "unused(",
            "contains(",
            "defines(",
            "reaches(",
        ]
        .iter()
        .any(|p| trimmed.starts_with(p))
}

fn looks_like_trace(lower: &str) -> bool {
    [
        "addresssanitizer:",
        "threadsanitizer:",
        "undefinedbehaviorsanitizer:",
        "stack trace",
        "traceback (most recent call last)",
    ]
    .iter()
    .any(|p| lower.contains(p))
        || (lower.contains("#0 ") && (lower.contains(" in ") || lower.contains(" at ")))
}

fn has_identifier_shape(task: &str, tok: &str, start: usize) -> bool {
    let bytes = task.as_bytes();
    let end = start + tok.len();
    let backticked =
        start > 0 && end < bytes.len() && bytes[start - 1] == b'`' && bytes[end] == b'`';
    let called = end + 1 < bytes.len() && bytes[end] == b'(' && bytes[end + 1] == b')';
    let camel_or_snake = tok.contains('_')
        || tok.contains("::")
        || tok
            .as_bytes()
            .windows(2)
            .any(|w| w[0].is_ascii_lowercase() && w[1].is_ascii_uppercase());
    if backticked || called || camel_or_snake {
        return true;
    }
    let prefix = task[..start].to_ascii_lowercase();
    tok.len() >= 3
        && [
            "of ",
            "edited ",
            "calls ",
            "function ",
            "method ",
            "symbol ",
            "understand ",
        ]
        .iter()
        .any(|cue| prefix.ends_with(cue))
}

fn resolve_task_symbols(task: &str, ingest: &IngestResult, interner: &Interner) -> Vec<String> {
    let code_spurs: std::collections::HashSet<lasso::Spur> = ingest
        .symbols
        .iter()
        .filter(|s| s.kind != SymKind::Section)
        .map(|s| s.name)
        .collect();
    let mut resolved = BTreeSet::new();
    let bytes = task.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if !(bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
            i += 1;
            continue;
        }
        let start = i;
        while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
            i += 1;
        }
        let tok = &task[start..i];
        if tok.len() >= 2
            && let Some(spur) = interner.get(tok)
            && code_spurs.contains(&spur)
            && has_identifier_shape(task, tok, start)
        {
            resolved.insert(tok.to_string());
        }
    }
    resolved.into_iter().collect()
}

fn score_phrases(lower: &str, cues: &[(&str, u32)]) -> u32 {
    cues.iter()
        .filter(|(p, _)| lower.contains(p))
        .map(|(_, w)| *w)
        .sum()
}

fn direct_single_sym_route(lower: &str, root: &str, sym: &str) -> Option<TaskChoice> {
    let (intent, skill, reason, verb) =
        if lower.contains("edited") || lower.contains("contract") || lower.contains("signature") {
            (
                "edit-contract",
                "ripwire-rs-change-check",
                "one exact indexed symbol plus post-edit contract wording",
                "--edit-check",
            )
        } else if lower.contains("who calls") || lower.contains("caller") {
            (
                "find-callers",
                "ripwire-rs-navigate",
                "one exact indexed symbol plus caller wording",
                "--callers",
            )
        } else if lower.contains("understand")
            || lower.contains("implementation")
            || lower.contains("how does")
        {
            (
                "understand-symbol",
                "ripwire-rs-navigate",
                "one exact indexed symbol plus understand wording",
                "--expand",
            )
        } else {
            return None;
        };
    Some(TaskChoice {
        intent,
        skill,
        reason,
        verb,
        command: format!("ripwire-rs '{root}' {verb}='{sym}'"),
        score: 100,
    })
}

fn check_direct_task_route(
    trimmed: &str,
    lower: &str,
    root: &str,
    trace: bool,
    syms: &[String],
) -> Option<TaskChoice> {
    if is_closed_claim_task(trimmed) {
        return Some(TaskChoice {
            intent: "verify-claim",
            skill: "ripwire-rs-navigate",
            reason: "closed claim grammar",
            verb: "--verify",
            command: format!("ripwire-rs '{root}' --verify='{trimmed}'"),
            score: 100,
        });
    }
    if trace {
        return Some(TaskChoice {
            intent: "trace-debug",
            skill: "ripwire-rs-navigate",
            reason: "stack-trace shape; pass the trace on stdin",
            verb: "--from-trace",
            command: format!("ripwire-rs '{root}' --from-trace=-"),
            score: 100,
        });
    }
    if let Some(lit) = extract_quoted_task_literal(trimmed) {
        return Some(TaskChoice {
            intent: "exact-grep",
            skill: "ripwire-rs-navigate",
            reason: "quoted literal plus exact-search wording",
            verb: "--grep",
            command: format!("ripwire-rs '{root}' --grep='{lit}' --grep-context=2 --limit=40"),
            score: 100,
        });
    }
    if (trimmed.contains(',') && (lower.contains("connect") || lower.contains("come together")))
        || syms.len() >= 3
    {
        let joined = if syms.len() >= 2 {
            syms.join(",")
        } else {
            trimmed.to_string()
        };
        return Some(TaskChoice {
            intent: "connect-symbols",
            skill: "ripwire-rs-navigate",
            reason: "minimal subgraph connecting named symbols",
            verb: "--connect",
            command: format!("ripwire-rs '{root}' --connect='{joined}'"),
            score: 100,
        });
    }
    (syms.len() == 1).then(|| direct_single_sym_route(lower, root, &syms[0]))?
}

fn collect_lexical_candidates(
    trimmed: &str,
    lower: &str,
    root: &str,
    dirty: bool,
) -> Vec<TaskChoice> {
    let mut cands = Vec::new();
    if dirty {
        let sc = score_phrases(
            lower,
            &[
                ("before i push", 9),
                ("before push", 9),
                ("ready to push", 9),
                ("safe to merge", 9),
                ("current changes", 6),
                ("my diff", 6),
                ("review my", 4),
            ],
        );
        if sc >= 8 {
            cands.push(TaskChoice {
                intent: "review-diff",
                skill: "ripwire-rs-change-check",
                reason: "dirty worktree plus review/pre-push wording",
                verb: "--situ",
                command: format!("ripwire-rs '{root}' --situ"),
                score: sc,
            });
        }
    }
    for &(intent, skill, reason, verb, floor, cues) in LEXICAL_ROUTES {
        let sc = score_phrases(lower, cues);
        if sc >= floor {
            let command = if matches!(verb, "--pack-task" | "--exemplar" | "--for") {
                format!("ripwire-rs '{root}' {verb}='{trimmed}'")
            } else {
                format!("ripwire-rs '{root}' {verb}")
            };
            cands.push(TaskChoice {
                intent,
                skill,
                reason,
                verb,
                command,
                score: sc,
            });
        }
    }
    cands.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.intent.cmp(b.intent)));
    cands
}

pub fn write_help_task_with_facts<W: Write>(
    task: &str,
    root: &str,
    ingest: &IngestResult,
    interner: &Interner,
    git: bool,
    dirty: bool,
    out: &mut W,
) -> io::Result<()> {
    let trimmed = task.trim();
    let lower = trimmed.to_ascii_lowercase();
    let is_sys_event = [
        "<task-notification>",
        "<system-reminder>",
        "<channel",
        "<agent-message",
    ]
    .iter()
    .any(|tag| trimmed.starts_with(tag));
    let trace = !is_sys_event && looks_like_trace(&lower);
    let syms = if is_sys_event {
        Vec::new()
    } else {
        resolve_task_symbols(trimmed, ingest, interner)
    };
    let mut choices = if is_sys_event {
        Vec::new()
    } else if let Some(d) = check_direct_task_route(trimmed, &lower, root, trace, &syms) {
        vec![d]
    } else {
        collect_lexical_candidates(trimmed, &lower, root, dirty)
    };
    let top_score = choices.first().map_or(0, |c| c.score);
    let margin = match choices.as_slice() {
        [] => 0,
        [only] => only.score,
        [first, second, ..] => first.score.saturating_sub(second.score),
    };
    let (status, confidence, keep) = if choices.is_empty() {
        ("abstain", "none", 0)
    } else if choices.len() == 1 || margin >= 3 {
        ("recommend", "high", 1)
    } else {
        ("ambiguous", "low", 2)
    };
    choices.truncate(keep);
    let t_esc = escape_xml_str(task);
    let rec_attr = choices.first().map_or_else(String::new, |c| {
        format!(" recommended=\"{}\" why=\"{}\"", c.verb, c.reason)
    });
    write!(
        out,
        "<!-- ripwire-rs help-task schema=ripwire-rs.help-task/v1: which verb answers this task: status=recommend|ambiguous|abstain confidence= score= margin=, <facts> the evidence. -->\n<task-route schema=\"ripwire-rs.help-task/v1\" task=\"{t_esc}\" status=\"{status}\" confidence=\"{confidence}\" score=\"{top_score}\" margin=\"{margin}\"{rec_attr}><facts git=\"{}\" dirty=\"{}\" trace=\"{}\" resolved_symbols=\"{}\"/>",
        usize::from(git),
        usize::from(dirty),
        usize::from(trace),
        syms.len()
    )?;
    for c in &choices {
        let cmd_esc = escape_xml_str(&c.command);
        write!(
            out,
            "<choice intent=\"{}\" skill=\"{}\" reason=\"{}\" score=\"{}\"><run>{cmd_esc}</run></choice>",
            c.intent, c.skill, c.reason, c.score
        )?;
    }
    writeln!(out, "</task-route>")
}
