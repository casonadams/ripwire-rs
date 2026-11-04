use super::gauges::GraphGaugeProvider;
use super::paging::PageRequest;
use crate::graph::GraphGauges;

/// Extension trait for concise field extraction from `serde_json::Value`.
pub trait JsonValueExt {
    fn req_str(&self, key: &str) -> Result<&str, String>;
    fn req_str_any(&self, keys: &[&str]) -> Result<&str, String>;
    fn opt_str(&self, key: &str) -> Option<&str>;
    fn opt_str_any(&self, keys: &[&str]) -> Option<&str>;
    fn str_or<'a>(&'a self, key: &str, default: &'a str) -> &'a str;
    fn str_any<'a>(&'a self, keys: &[&str], default: &'a str) -> &'a str;
    fn u64_or(&self, key: &str, default: u64) -> u64;
    fn u64_any(&self, keys: &[&str], default: u64) -> u64;
    fn opt_usize(&self, key: &str) -> Option<usize>;
    fn bool_or(&self, key: &str, default: bool) -> bool;
    fn f64_any(&self, keys: &[&str], default: f64) -> f64;
    fn array_any(&self, keys: &[&str]) -> Option<&Vec<serde_json::Value>>;
    fn str_vec(&self, key: &str) -> Vec<String>;

    fn attach_gauges<G: GraphGaugeProvider>(&mut self, source: &G);
    fn xml_opt_gauge_attrs(&self) -> String;
    /// `limit` / `offset` / `budget` keys carried by a JSON report value.
    fn page_request(&self) -> PageRequest {
        PageRequest {
            limit: self.opt_usize("limit"),
            offset: self.opt_usize("offset"),
            budget: self.opt_usize("budget"),
        }
    }
}

impl JsonValueExt for serde_json::Value {
    #[inline]
    fn req_str(&self, key: &str) -> Result<&str, String> {
        self.get(key)
            .and_then(Self::as_str)
            .ok_or_else(|| format!("missing '{key}' parameter"))
    }

    #[inline]
    fn req_str_any(&self, keys: &[&str]) -> Result<&str, String> {
        self.opt_str_any(keys).ok_or_else(|| {
            let names = keys
                .iter()
                .map(|k| format!("'{k}'"))
                .collect::<Vec<_>>()
                .join(" or ");
            format!("missing {names} parameter")
        })
    }

    #[inline]
    fn opt_str(&self, key: &str) -> Option<&str> {
        self.get(key).and_then(Self::as_str)
    }

    #[inline]
    fn opt_str_any(&self, keys: &[&str]) -> Option<&str> {
        keys.iter()
            .find_map(|&k| self.get(k).and_then(Self::as_str))
    }

    #[inline]
    fn str_or<'a>(&'a self, key: &str, default: &'a str) -> &'a str {
        self.opt_str(key).unwrap_or(default)
    }

    #[inline]
    fn str_any<'a>(&'a self, keys: &[&str], default: &'a str) -> &'a str {
        self.opt_str_any(keys).unwrap_or(default)
    }

    #[inline]
    fn u64_or(&self, key: &str, default: u64) -> u64 {
        self.get(key).and_then(Self::as_u64).unwrap_or(default)
    }

    #[inline]
    fn u64_any(&self, keys: &[&str], default: u64) -> u64 {
        keys.iter()
            .find_map(|&k| self.get(k).and_then(Self::as_u64))
            .unwrap_or(default)
    }

    #[inline]
    fn opt_usize(&self, key: &str) -> Option<usize> {
        self.get(key).and_then(Self::as_u64).map(|v| v as usize)
    }

    #[inline]
    fn bool_or(&self, key: &str, default: bool) -> bool {
        self.get(key).and_then(Self::as_bool).unwrap_or(default)
    }

    #[inline]
    fn f64_any(&self, keys: &[&str], default: f64) -> f64 {
        keys.iter()
            .find_map(|&k| self.get(k).and_then(Self::as_f64))
            .unwrap_or(default)
    }

    #[inline]
    fn array_any(&self, keys: &[&str]) -> Option<&Vec<serde_json::Value>> {
        keys.iter()
            .find_map(|&k| self.get(k).and_then(Self::as_array))
    }

    #[inline]
    fn str_vec(&self, key: &str) -> Vec<String> {
        self.get(key)
            .and_then(Self::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn attach_gauges<G: GraphGaugeProvider>(&mut self, source: &G) {
        let g = source.gauges();
        self["graph_ambiguous"] = serde_json::json!(g.ambiguous);
        self["graph_unresolved"] = serde_json::json!(g.unresolved);
        self["graph_unindexed"] = serde_json::json!(g.unindexed);
    }

    fn xml_opt_gauge_attrs(&self) -> String {
        let Some(amb) = self.get("graph_ambiguous").and_then(Self::as_u64) else {
            return String::new();
        };
        let g = GraphGauges {
            ambiguous: amb as usize,
            unresolved: self.u64_or("graph_unresolved", 0) as usize,
            external: 0,
            declined: 0,
            unindexed: self.u64_or("graph_unindexed", 0) as usize,
        };
        format!(" {}", g.xml_gauge_attrs())
    }
}
