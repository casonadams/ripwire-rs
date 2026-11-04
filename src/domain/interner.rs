use lasso::{Key, Spur, ThreadedRodeo};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Thread-safe string interner backed by `lasso::ThreadedRodeo`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Interner(Arc<ThreadedRodeo>);

impl Interner {
    pub fn new() -> Self {
        Self::default()
    }
    #[inline]
    pub fn intern(&self, s: &str) -> Spur {
        self.0.get_or_intern(s)
    }
    #[inline]
    pub fn get(&self, s: &str) -> Option<Spur> {
        self.0.get(s)
    }
    #[inline]
    pub fn resolve<'a>(&'a self, spur: &Spur) -> &'a str {
        self.0.resolve(spur)
    }
    #[inline]
    pub fn try_resolve<'a>(&'a self, spur: &Spur) -> Option<&'a str> {
        self.0.try_resolve(spur)
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    #[must_use]
    pub fn all_strings(&self) -> Vec<String> {
        let mut out = Vec::with_capacity(self.0.len());
        for i in 0..self.0.len() {
            if let Some(spur) = Spur::try_from_usize(i)
                && let Some(s) = self.0.try_resolve(&spur)
            {
                out.push(s.to_string());
            }
        }
        out
    }
    #[must_use]
    pub fn from_strings(strings: &[String]) -> Self {
        let rodeo = ThreadedRodeo::new();
        for s in strings {
            rodeo.get_or_intern(s);
        }
        Self(Arc::new(rodeo))
    }
}

impl std::ops::Index<Spur> for Interner {
    type Output = str;

    #[inline]
    fn index(&self, spur: Spur) -> &Self::Output {
        self.resolve(&spur)
    }
}

impl std::ops::Index<&Spur> for Interner {
    type Output = str;

    #[inline]
    fn index(&self, spur: &Spur) -> &Self::Output {
        self.resolve(spur)
    }
}
