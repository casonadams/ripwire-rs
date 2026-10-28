# Algorithms & Academic Lenses

`ripwire-rs` implements 11 academic and algorithmic lenses over codebase graphs and behavioral histories.

## 1. Personalized PageRank
- **Reference**: [Wikipedia: PageRank](https://en.wikipedia.org/wiki/PageRank)
- **Formulation**: Power iteration over transpose in-edge CSR matrix with dangling mass redistribution:
  $$p^{(k+1)} = \alpha P^T p^{(k)} + (1 - \alpha + \alpha m_{\text{dangling}}) v_{\text{teleport}}$$
- **Optimization**: 1024-block reduction sums with early convergence checking ($L_1 \text{ residual} < 10^{-6}$).

## 2. Kleinberg HITS (Hubs & Authorities)
- **Reference**: [Wikipedia: HITS algorithm](https://en.wikipedia.org/wiki/HITS_algorithm)
- **Formulation**: Coupled iterative eigenvector convergence:
  $$a^{(k+1)} = \frac{W_{\text{in}} h^{(k)}}{\|W_{\text{in}} h^{(k)}\|_2}, \quad h^{(k+1)} = \frac{W_{\text{out}} a^{(k+1)}}{\|W_{\text{out}} a^{(k+1)}\|_2}$$
- **Edge weights by calling file**: each edge $u \to v$ is divided by the number of callers of $v$ in $u$'s file, in both $W_{\text{in}}$ and $W_{\text{out}}$, so many callers in one file count as one file's reliance. Raw HITS concentrates on the densest caller block (the tightly-knit-community effect), so a test file's local helpers outrank code the whole tree calls.
- **Interpretation**: Authorities highlight core utility APIs relied on across files; hubs highlight orchestrators and test harnesses that reach into many files.

## 3. Louvain Community Modularity
- **Reference**: [Wikipedia: Louvain method](https://en.wikipedia.org/wiki/Louvain_method), [Wikipedia: Modularity (networks)](https://en.wikipedia.org/wiki/Modularity_(networks))
- Partitions the call graph into architectural modules by maximizing Newman-Girvan modularity:
  $$\Delta Q = \frac{k_{i,\text{in}}}{2m} - \frac{\Sigma_{\text{tot}} \cdot k_i}{2m^2}$$
- Symmetrizes directed call edges and provides deterministic tie-breaking.

## 4. AST Clone Detector (Type-1, Type-2, Type-3 Gapped)
- **Reference**: [Wikipedia: Duplicate code](https://en.wikipedia.org/wiki/Duplicate_code)
- Token-level normalization stripping comments, formatting, and identifier names.
- Type-1: Exact token matching via 64-bit rolling hash.
- Type-2: Parameter and identifier renaming normalization.
- Type-3: Near-miss duplicate detection using $k$-gram Jaccard prefiltering ($k=5$) and Longest Common Subsequence (LCS). Similarity floor 0.80, raised to 0.85 when the shorter body has fewer than 60 normalized tokens (short chains share tokens by chance).
- Idiom Demotion: Demotes builder chains, threshold ladders, and switch tables to prevent false positives.

## 5. ARISE Intra-Procedural Slicing
- **Reference**: [Wikipedia: Program slicing](https://en.wikipedia.org/wiki/Program_slicing) · [arXiv:2605.03117](https://arxiv.org/abs/2605.03117)
- Traces forward and backward dataflow dependencies from a criterion variable $(s, v)$.
- Identifies reaching definitions, transitive uses, parameter bindings, and assignment propagation.

## 6. Git Churn Decay & Co-Change Mining
- **Reference**: [Wikipedia: Shotgun surgery](https://en.wikipedia.org/wiki/Shotgun_surgery)
- Exponential half-life decay weighting commits over time:
  $$w(t) = 0.5^{\frac{\Delta t}{\tau}}$$
- Detects Fowler's Shotgun Surgery coupling patterns across file co-change history.

## 7. Maintenance Hotspots
- **Reference**: [Wikipedia: Technical debt](https://en.wikipedia.org/wiki/Technical_debt), [Wikipedia: Software maintenance](https://en.wikipedia.org/wiki/Software_maintenance)
- Combines structural complexity with historical change frequency:
  $$\text{Hotspot Score} = \text{Churn} \times \text{Cognitive Complexity}$$

## 8. McCabe Cyclomatic & Essential Complexity
- **Reference**: [Wikipedia: Cyclomatic complexity](https://en.wikipedia.org/wiki/Cyclomatic_complexity), [Wikipedia: Essential complexity](https://en.wikipedia.org/wiki/Essential_complexity)
- Counts independent control flow paths ($M = E - N + 2P$) and essential unstructured jump complexity.

## 9. Posnett Code Readability
- **Reference**: [Wikipedia: Halstead complexity measures](https://en.wikipedia.org/wiki/Halstead_complexity_measures) · [Posnett et al. (MSR 2011)](https://doi.org/10.1145/1985441.1985454)
- Information-theoretic readability metric combining entropy, Halstead volume, line length, and identifier vocabularies.

## 10. Language-Agnostic Embedded SQL Table Lineage
- **Mechanism**: Zero-flag, automated SQL DML extraction across string literals, raw strings, and template strings in all 36 host languages.
- **Reference Roles**:
  - `SELECT ... FROM [ONLY] <table>` & `JOIN <table>` $\rightarrow$ `RefRole::Read`
  - `INSERT INTO <table>`, `UPDATE <table>`, `DELETE FROM [ONLY] <table>` $\rightarrow$ `RefRole::Write`
- **Graph Integration**: Binds extracted table tokens only to `.sql` schema definitions (e.g. `CREATE TABLE`), never to a host-language symbol of the same name, enabling transitive blast radius (`--impact`), caller tracing (`--callers`), and pre-PR verification gates (`--test-gate`, `--affected`, `--situ`) across database and application boundaries. Names the same literal declares as CTEs (`WITH name AS (`) are query-local and emit no reference; a table no schema defines counts as external.
