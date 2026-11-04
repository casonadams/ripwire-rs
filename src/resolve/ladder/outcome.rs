use crate::domain::NodeId;
use serde::{Deserialize, Serialize};

/// How a resolved call edge was bound, weakest evidence first so `max` keeps the strongest.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
pub enum EdgeProv {
    /// One arm of a call split across several definitions.
    #[default]
    Split,
    /// Bound to a primary template whose specialization is another definition (which one runs is
    /// unproven).
    Template,
    /// Bound by the name alone: a same-directory or repository-wide match.
    Name,
    /// Bound by lexical evidence: the caller's own file or enclosing type defines it.
    Local,
    /// A type qualifier matched the definition's scope by its last segment only.
    FinalSegment,
    /// The qualifier is a module alias bound by an import of the definition's file.
    Import,
    Typed,
    Binding,
    /// A compiler-backed SCIP index pinned the call site to this definition.
    Scip,
}

impl EdgeProv {
    #[must_use]
    pub fn is_split(self) -> bool {
        self == Self::Split
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Split => "split",
            Self::Template => "template",
            Self::Name => "name",
            Self::Local => "local",
            Self::FinalSegment => "final-segment",
            Self::Import => "import",
            Self::Typed => "typed",
            Self::Binding => "binding",
            Self::Scip => "scip",
        }
    }

    #[must_use]
    pub fn callee_label(self) -> Option<&'static str> {
        match self {
            Self::Template | Self::Name | Self::Local | Self::Typed => None,
            other => Some(other.label()),
        }
    }

    /// True when only the name bound the edge (`via="name"`).
    #[must_use]
    pub fn is_by_name(self) -> bool {
        self == Self::Name
    }

    /// Suffix on a caller row's `of_def=K`: split arm, primary template, or bound by name alone.
    #[must_use]
    pub fn of_def_suffix(self) -> &'static str {
        match self {
            Self::Split => "~s",
            Self::Template => "~t",
            Self::Name => "~n",
            Self::Local
            | Self::FinalSegment
            | Self::Import
            | Self::Typed
            | Self::Binding
            | Self::Scip => "",
        }
    }
}

/// Which evidence narrowed a reference's candidates before the precedence ladder ran.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Narrowing {
    None,
    CallerScope,
    Owner,
    FinalSegment,
    Import,
}

/// Resolved reference outcome with target nodes and per-target confidence.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedRef {
    pub from: NodeId,
    pub targets: Vec<NodeId>,
    pub conf_per_target: f32,
    pub prov: EdgeProv,
    /// A type mention (`RefRole::Type`): navigable as a reference, but no call flow for PageRank.
    pub type_use: bool,
}

impl ResolvedRef {
    #[must_use]
    pub fn is_ambiguous(&self) -> bool {
        self.prov.is_split()
    }

    pub(super) fn bound(from: NodeId, targets: Vec<NodeId>, prov: EdgeProv) -> Option<Self> {
        if targets.is_empty() {
            return None;
        }
        Some(Self {
            from,
            conf_per_target: 1.0 / targets.len() as f32,
            prov: if targets.len() > 1 {
                EdgeProv::Split
            } else {
                prov
            },
            targets,
            type_use: false,
        })
    }
}

/// Triage classification for a reference during call-graph resolution.
#[derive(Debug, Clone, PartialEq)]
pub enum RefOutcome {
    Ignored,
    Resolved(ResolvedRef),
    Unresolved,
    External,
    /// In-tree candidates exist but no precedence tier could choose among them.
    Declined,
    /// A std/builtin member name whose in-tree namesakes were all rejected.
    Builtin,
}
