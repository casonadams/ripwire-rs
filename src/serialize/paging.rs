use super::DEFAULT_PACK_BUDGET_BYTES;
use super::budget::row_bytes;

/// A secondary list gets this fraction of its parent's byte budget.
const NESTED_BUDGET_DIVISOR: usize = 4;

/// A caller's `--limit` / `--offset` request and the output budget every default cap is computed from.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct PageRequest {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    /// Byte budget for one list of rows; `None` means [`DEFAULT_PACK_BUDGET_BYTES`].
    pub budget: Option<usize>,
}

impl PageRequest {
    #[must_use]
    pub fn with_budget(self, budget: Option<usize>) -> Self {
        Self { budget, ..self }
    }

    /// A secondary list inside a report: no `--limit` / `--offset`, and a quarter of the byte budget, so a
    /// report's lists together stay near one budget. A list always shows at least one row.
    #[must_use]
    pub fn nested(self) -> Self {
        let budget = self.budget.unwrap_or(DEFAULT_PACK_BUDGET_BYTES);
        Self {
            limit: None,
            offset: None,
            budget: Some(budget / NESTED_BUDGET_DIVISOR),
        }
    }

    /// The window over `total` rows. An explicit `--limit` wins; otherwise the cap is however many rows,
    /// starting at the offset, fit the byte budget when each row costs `row_bytes(index)` (always at least one).
    #[must_use]
    pub fn window_fit(self, total: usize, row_bytes: impl Fn(usize) -> usize) -> PageWindow {
        let begin = self.offset.unwrap_or(0).min(total);
        let cap = match self.limit {
            Some(limit) if limit > 0 => limit,
            _ => fit_rows(
                (begin..total).map(row_bytes),
                self.budget.unwrap_or(DEFAULT_PACK_BUDGET_BYTES),
            ),
        };
        PageWindow::compute(total, cap, self.limit, self.offset)
    }

    /// Like [`Self::window_fit`], but a row is shown only while it fits the budget, so the window may be empty.
    /// Use where an empty list is the truthful answer for a budget too small for any row.
    #[must_use]
    pub fn window_within(self, total: usize, row_bytes: impl Fn(usize) -> usize) -> PageWindow {
        let begin = self.offset.unwrap_or(0).min(total);
        let budget = self.budget.unwrap_or(DEFAULT_PACK_BUDGET_BYTES);
        let mut used = 0usize;
        let mut fitted = 0usize;
        for size in (begin..total).map(row_bytes) {
            used = used.saturating_add(size);
            if used > budget {
                break;
            }
            fitted += 1;
        }
        let cap = match self.limit {
            Some(limit) if limit > 0 => limit,
            _ => fitted,
        };
        PageWindow::compute(total, cap, self.limit, self.offset)
    }
}

/// How many leading rows fit `budget` bytes (at least one when any row exists).
fn fit_rows(sizes: impl Iterator<Item = usize>, budget: usize) -> usize {
    let mut used = 0usize;
    let mut fitted = 0usize;
    for size in sizes {
        used = used.saturating_add(size);
        if used > budget && fitted > 0 {
            break;
        }
        fitted += 1;
    }
    fitted.max(1)
}

/// One list of rows, windowed once: the single place its cap, disclosure attributes and shown rows come from.
pub struct Paged<'a, T> {
    all: &'a [T],
    pub window: PageWindow,
}

impl<'a, T> Paged<'a, T> {
    /// Window `all` under `page`; `cost` is the bytes one row adds to the output.
    #[must_use]
    pub fn new(all: &'a [T], page: PageRequest, cost: impl Fn(&T) -> usize) -> Self {
        let window = page.window_fit(all.len(), |i| cost(&all[i]));
        Self { all, window }
    }

    /// The rows inside the window.
    #[must_use]
    pub fn rows(&self) -> &'a [T] {
        &self.all[self.window.begin..self.window.end]
    }

    /// `shown= capped= total= has_more= next_offset= offset= limit=` for the list's root element.
    #[must_use]
    pub fn attrs(&self) -> String {
        self.window.xml_attrs()
    }
}

/// Write the rows of `all` that fit `page`, each row through `write_row`. The window is computed from the
/// bytes each row measures when minified, so a row costs what it writes.
pub fn write_paged_rows<T, W: std::io::Write>(
    out: &mut W,
    all: &[T],
    page: PageRequest,
    write_row: impl Fn(&mut dyn std::io::Write, &T) -> std::io::Result<()>,
) -> std::io::Result<()> {
    let shown = Paged::new(all, page, |row| row_bytes(|w| write_row(w, row)));
    for row in shown.rows() {
        write_row(out, row)?;
    }
    Ok(())
}

/// The window over `rows`, each row costing the bytes `row` writes (see [`row_bytes`]).
#[must_use]
pub fn window_rows<T>(
    page: PageRequest,
    rows: &[T],
    row: impl Fn(&T, &mut dyn std::io::Write) -> std::io::Result<()>,
) -> PageWindow {
    page.window_fit(rows.len(), |i| row_bytes(|w| row(&rows[i], w)))
}

/// Writes the rows that `window` shows, in order.
pub fn write_window_rows<T, W: std::io::Write + ?Sized>(
    rows: &[T],
    window: &PageWindow,
    out: &mut W,
    row: impl Fn(&T, &mut W) -> std::io::Result<()>,
) -> std::io::Result<()> {
    for r in &rows[window.begin..window.end] {
        row(r, out)?;
    }
    Ok(())
}

/// Computed pagination slice and Ripwire truncation disclosure metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageWindow {
    pub begin: usize,
    pub end: usize,
    pub shown: usize,
    pub total: usize,
    pub capped: bool,
    pub has_more: bool,
    pub next_offset: usize,
    pub limit: usize,
    pub offset: usize,
}

impl PageWindow {
    /// The window of `cap` rows from `offset` over `total`; `limit` and `offset` are recorded for disclosure.
    fn compute(total: usize, cap: usize, limit: Option<usize>, offset: Option<usize>) -> Self {
        let off = offset.unwrap_or(0);
        let lim = limit.unwrap_or(0);
        let begin = off.min(total);
        let end = begin.saturating_add(cap).min(total);
        let shown = end.saturating_sub(begin);
        let has_more = end < total;
        let capped = shown < total;
        Self {
            begin,
            end,
            shown,
            total,
            capped,
            has_more,
            next_offset: end,
            limit: lim,
            offset: off,
        }
    }

    /// Write Ripwire XML pagination disclosure attributes (`shown=`, `capped=`, `total=`, `has_more=`, `next_offset=`).
    pub fn write_xml_attrs<W: std::io::Write>(&self, out: &mut W) -> std::io::Result<()> {
        if self.capped || self.offset > 0 || self.limit > 0 {
            write!(
                out,
                " shown=\"{}\" capped=\"{}\" total=\"{}\" has_more=\"{}\" next_offset=\"{}\" offset=\"{}\" limit=\"{}\"",
                self.shown,
                usize::from(self.capped),
                self.total,
                usize::from(self.has_more),
                self.next_offset,
                self.offset,
                self.limit
            )
        } else {
            write!(out, " shown=\"{}\" capped=\"0\"", self.shown)
        }
    }

    /// [`Self::write_xml_attrs`] as a string.
    #[must_use]
    pub fn xml_attrs(&self) -> String {
        let mut buf = Vec::new();
        let _ = self.write_xml_attrs(&mut buf);
        String::from_utf8(buf).unwrap_or_default()
    }
}
