use super::model::QualityRegression;
use crate::gate::baseline::QualitySnapshot;
use crate::gate::rename::RenameMap;

/// `bar=` for the numeric kinds; `None` for the rest.
pub(super) fn numeric_bar(kind: &str) -> Option<u32> {
    NUMERIC_KINDS.iter().find(|k| k.kind == kind).map(|k| k.bar)
}

/// A numeric kind and the bar its `now=` is judged against.
pub struct NumericKind {
    pub kind: &'static str,
    pub bar: u32,
    pub extract: fn(&QualitySnapshot, &str) -> u32,
}

pub static NUMERIC_KINDS: &[NumericKind] = &[
    NumericKind {
        kind: "complexity",
        bar: 15,
        extract: |snap, key| snap.ccx_by_sym.get(key).copied().unwrap_or(0),
    },
    NumericKind {
        kind: "verbosity",
        bar: 60,
        extract: |snap, key| snap.loc_by_sym.get(key).copied().unwrap_or(0),
    },
    NumericKind {
        kind: "nesting",
        bar: 4,
        extract: |snap, key| u32::from(snap.nest_by_sym.get(key).copied().unwrap_or(0)),
    },
    NumericKind {
        kind: "params",
        bar: 5,
        extract: |snap, key| u32::from(snap.params_by_sym.get(key).copied().unwrap_or(0)),
    },
];

/// How a numeric change on a symbol that existed at the baseline reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumericSeverity {
    Major,
    Minor,
}

/// Classifies `was -> now` against `bar`, matching upstream ripwire: over the bar, crossing it or
/// growing by a quarter is major unless the step is a single unit; under it, only a doubling that
/// reaches two thirds of the bar is reported, as minor. `None` means no row.
#[must_use]
pub fn classify_numeric(was: u32, now: u32, bar: u32) -> Option<NumericSeverity> {
    if now <= was {
        return None;
    }
    if now > bar {
        let major = now - was >= 2 && (was <= bar || now * 4 >= was * 5);
        return Some(if major {
            NumericSeverity::Major
        } else {
            NumericSeverity::Minor
        });
    }
    (now >= 2 * was && now * 3 >= bar * 2).then_some(NumericSeverity::Minor)
}

pub(super) fn check_symbol_numeric_regressions(
    key: &str,
    working: &QualitySnapshot,
    baseline: &QualitySnapshot,
    renames: &RenameMap,
    regressions: &mut Vec<QualityRegression>,
) {
    let parts: Vec<&str> = key.split("::").collect();
    let row = QualityRegression {
        sym_name: parts.last().copied().unwrap_or(key).to_string(),
        file_path: parts.first().copied().unwrap_or(key).to_string(),
        line: working.line_by_sym.get(key).copied().unwrap_or(0),
        renamed_from: renames.pair_for(key).map(|p| p.old_name.clone()),
        ..QualityRegression::default()
    };
    let was_key = renames.was_key(key);
    let is_preexisting = baseline.ccx_by_sym.contains_key(was_key);
    for dim in NUMERIC_KINDS {
        let now_val = (dim.extract)(working, key);
        let was_val = if is_preexisting {
            (dim.extract)(baseline, was_key)
        } else {
            0
        };
        let severity = if is_preexisting {
            classify_numeric(was_val, now_val, dim.bar)
        } else {
            (now_val > dim.bar).then_some(NumericSeverity::Minor)
        };
        if let Some(severity) = severity {
            regressions.push(QualityRegression {
                kind: dim.kind.to_string(),
                was_val,
                now_val,
                is_gating: is_preexisting && severity == NumericSeverity::Major,
                is_minor: is_preexisting && severity == NumericSeverity::Minor,
                is_new_symbol: !is_preexisting,
                ..row.clone()
            });
        }
    }
}
