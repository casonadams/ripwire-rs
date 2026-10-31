use std::borrow::Cow;
use std::io;
use std::path::Path;

pub mod branch_tools;
pub mod commands;
pub mod defs;
pub mod detail;
pub mod doc_tools;
pub mod edit;
mod evidence;
pub mod graph;
pub mod grep;
pub mod map;
pub mod navigation;
pub mod recall;
pub mod retrieval;
pub mod safety;
pub mod search;
pub mod stray_lines;
pub mod task_route;

pub trait SourceReader: Sync {
    fn read(&self, path: &str) -> io::Result<Cow<'_, [u8]>>;
    fn corpus(&self) -> crate::crawler::CorpusWalk;
}

impl<T: SourceReader + ?Sized> SourceReader for &T {
    fn read(&self, path: &str) -> io::Result<Cow<'_, [u8]>> {
        T::read(*self, path)
    }

    fn corpus(&self) -> crate::crawler::CorpusWalk {
        T::corpus(*self)
    }
}

pub struct FileSources<'a> {
    pub root: &'a Path,
}

impl SourceReader for FileSources<'_> {
    fn read(&self, path: &str) -> io::Result<Cow<'_, [u8]>> {
        crate::domain::read_rel_bytes(self.root, path).map(Cow::Owned)
    }

    fn corpus(&self) -> crate::crawler::CorpusWalk {
        crate::crawler::walk_corpus(self.root, &crate::crawler::CrawlerOptions::default())
    }
}

impl SourceReader for [(&str, &[u8])] {
    fn read(&self, path: &str) -> io::Result<Cow<'_, [u8]>> {
        self.iter()
            .find(|(candidate, _)| *candidate == path)
            .map(|(_, bytes)| Cow::Borrowed(*bytes))
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, path.to_string()))
    }

    fn corpus(&self) -> crate::crawler::CorpusWalk {
        let mut rel_paths: Vec<String> = self.iter().map(|(path, _)| (*path).to_string()).collect();
        rel_paths.sort();
        rel_paths.dedup();
        crate::crawler::CorpusWalk {
            rel_paths,
            pruned_dirs: 0,
        }
    }
}

impl<const N: usize> SourceReader for [(&str, &[u8]); N] {
    fn read(&self, path: &str) -> io::Result<Cow<'_, [u8]>> {
        self.as_slice().read(path)
    }

    fn corpus(&self) -> crate::crawler::CorpusWalk {
        self.as_slice().corpus()
    }
}
