
(function() {
  var canvas = document.getElementById('c');
  var ctx = canvas.getContext('2d');
  var search = document.getElementById('search');
  var info = document.getElementById('info');
  var cardsEl = document.getElementById('cards');
  var crumbEl = document.getElementById('crumb');
  var depthSlider = document.getElementById('depthSlider');
  var depthVal = document.getElementById('depthVal');
  var W = canvas.width, H = canvas.height;

  // colour by lang. The palette is EMITTED (LANG_COLORS above — htmlexport.h::kLangColors, one entry per
  // model.h Lang enumerator, static_asserted against the roster) and never spelled here: the hand-written
  // copy that used to sit in this spot had 9 keys against langTag's 19, so eleven languages fell through to
  // an unlabelled grey. The legend below is built from the same object, so there is no second list to drift.
  var langColor = LANG_COLORS;

  // ---- the `tested` lens' two channels. H5: colorForNode used to return '#2ecc71' / '#e74c3c' — exactly
  // the red-vs-green discrimination the rampColor comment three lines down refuses for cx/churn, and for the
  // same reason. Under simulated deuteranopia that pair collapses from a perceptual distance of 231 to 82:
  // two olives. So `tested` now carries a channel that is NOT hue — tested nodes are FILLED, untested nodes
  // are hollow with a dashed ring — and the hues move onto the blue-yellow axis the ramp already uses, as
  // reinforcement rather than as the message. A monochrome print of this page is still readable.
  // The two fills are ramp STOPS, not a third palette beside it: they were '#26c6da'/'#ff9800' and then
  // '#2bccc0'/'#ffce1c', stops of two earlier ramps that each became an orphan hue the moment the ramp
  // moved on. They are now stops 1 and 2, the ramp's own BLUE↔AMBER pair — the widest-separated pair it
  // has (340/441 normal, 259 protan, 269 deutan, 257 tritan, against 265/143/166/257 for the next best),
  // which is what a binary channel wants. Untested is the BRIGHTER of the two, on the same "risk is what
  // glows" rule the ramp below states.
  var TESTED_FILL = '#29a0cc', UNTESTED_FILL = '#eb9809';
  var testedStroke = function(n) { return !n.ts; };   // untested ⇒ dashed ring instead of a solid disc

  // ---- --color-by palettes. commColor: 12 categorical dark-bg-friendly hues for the community LENS,
  // where hue IS the message (comm % 12). hullColor is a SEPARATE palette for the module outlines —
  // see its own note below; the two used to be one array and that is exactly the defect it fixes.
  //
  // rampColor: the shared 5-step COOL→HOT ramp for cx/churn over FIXED thresholds (fixed beats quantiles
  // for legend honesty — the same bucket means the same thing in every repo).
  //   cx buckets:    0 | 1-4 | 5-9 | 10-19 | 20+   → boundaries [1,5,10,20]
  //   churn buckets: 0 | 1-2 | 3-9 | 10-29 | 30+   → boundaries [1,3,10,30]
  //
  // DEEP BLUE → MID BLUE → AMBER → ORANGE → PALE YELLOW, monotone in luminance and ordered COOL-DIM →
  // HOT-BRIGHT, which is the direction that makes a metric legible at a glance on a dark ground: the
  // calm majority sits at the dim end and the rare 20+ nodes are the ones that glow. Blue↔orange is the
  // canonical dichromacy-safe axis — red/green confusion does not act on it at all — and amber-on-black
  // is the instrument-panel convention for the same reason a car gauge uses it. Measured with a
  // Brettel/Viénot 1999 CVD simulation:
  //   stop  hex       hue    rel.lum   vs #111    normal/protan/deutan/tritan distance to the NEXT stop
  //   0     #4b81c9   214°   0.2141     4.75:1     87.6 /  66.9 /  61.4 /  68.0
  //   1     #0fa3ff   203°   0.3352     6.93:1    340.2 / 259.1 / 269.4 / 256.6
  //   2     #f9a408    39°   0.4671     9.30:1    141.8 / 150.2 / 145.6 /  61.3
  //   3     #fdcc90    33°   0.6608    12.78:1     63.0 /  68.3 /  61.7 /  64.4
  //   4     #fefabb    56°   0.9303    17.63:1        —
  // Worst ADJACENT pair, which with a monotone ramp is also the worst of all ten pairs: 63.0 normal /
  // 66.9 protan / 61.4 deutan / 61.3 tritan, against the teal-midpoint ramp this replaces at 80.2 /
  // 66.9 / 61.4 / 61.1. Protanopia and deuteranopia are UNCHANGED to the decimal, because on both
  // ramps the pair that sets them is stops 0-1 and those two stops did not move. Normal vision gives up
  // 17 points and tritanopia gains 0.2. Every stop still clears 4.5:1 against the canvas ground at the
  // same 4.75:1 floor, and the greyscale ladder is 1.458 / 1.343 / 1.375 / 1.379 — no step weaker than
  // the 1.342 the previous ramp's weakest step measured.
  //
  // THREE THINGS THE MEASUREMENT DECIDED, none of which were obvious from the ladder written down:
  //   • THE ORANGE HAS TO BE THE LIGHTER OF THE TWO WARM STOPS, and therefore the less saturated. At
  //     full chroma an amber sits at luminance 0.585 and an orange at 0.400 — the hue that reads as
  //     "orange" is intrinsically darker — so "amber then orange" and "monotone in luminance" can only
  //     both hold if the orange is a light one. Under a greyscale-step floor no colour above luminance
  //     0.64 in the orange hue band exceeds 0.47 chroma, so stop 3 is a light orange at 0.43 and that
  //     is the ceiling, not a preference. Ordering the warm run by hue instead (orange, then amber, the
  //     way every saturated heat ramp runs) measures 63.9 / 64.6 / 61.4 / 61.4 — the same to within a
  //     point and a half, so nothing was bought by inverting the ladder that was asked for.
  //   • THE TOP STOP IS PALE BY BLUE, NOT BY DESATURATION. Keeping the previous ramp's '#fff794' above
  //     a light orange collapses the top pair to 39.2/441 under deuteranopia — below the gate's 45 bar
  //     — because the two differ by five points of blue and almost nothing else. '#fefabb' is paler AND
  //     further away (61.7) precisely because its paleness comes from a blue channel at 187: blue is
  //     the one channel protanopia and deuteranopia keep intact.
  //   • THE TEAL WAS LOAD-BEARING AND IS NOT MISSED. A cyan midpoint separates from both neighbours
  //     across the whole spectrum (171/205/204 to the next stop), which is why the ramp before this one
  //     could afford a pale top. Three adjacent warm stops cannot do that, and the cost is confined to
  //     NORMAL vision, where 63.0/441 is still eight times the JND and four times the 16/441 at which
  //     two swatches start to look alike.
  // Thresholds stay FIXED. A quantile ramp would let a cold corpus manufacture a hot node by making the
  // same swatch mean 20+ in one repository and 3 in another. The gate does not take any of this on
  // trust — test/htmlrendercheck.sh arm (Q) re-derives the luminance, the greyscale step and the three
  // CVD simulations from the stops the page actually emits, each with its own mutation control.
  var commColor = ['#4a90d9','#e67e22','#2ecc71','#e74c3c','#9b59b6','#f4c542',
                   '#1abc9c','#e84393','#00acd7','#a3d977','#dea584','#7f8c8d'];
  var rampColor = ['#005ec9','#29a0cc','#eb9809','#f0ce48','#fffcd1'];
  // hullColor: the module OUTLINES, and the reason they are not commColor any more. Identity is carried
  // by containment now (see draw()'s hull block and loadSubset's), so the outline's hue says nothing the
  // outline and its label do not already say — it is decoration. Borrowed from commColor it was
  // decoration ON THE RAMP'S OWN AXES: a saturated blue (#4a90d9) and a saturated amber (#f4c542) drawn
  // over a picture whose metric runs from blue to amber. Measured against the ramp above, the nearest of
  // those twelve sits 22.0/441 from a ramp stop — closer than two ADJACENT cx buckets are to each other
  // (63.0) — so nothing in the picture could tell a reader whether a colour meant a module or a
  // complexity. These twelve are a narrow desaturated violet→rose band, chosen by maximin over that band
  // so the closest two are still 36.9/441 apart (a hull is a REGION; two adjacent ones must not read as
  // one), every one is at most 0.196 chromatic against the ramp's 0.263 floor, the nearest is 56.8/441
  // from any ramp stop, and every one clears 4.71:1 on the #111 ground because the module's NAME is
  // drawn in it. The band is 240-355°, well clear of both the ramp's blue (203-214°) and its warm run
  // (33-56°). Consecutive ids alternate dim/bright so two neighbouring modules differ by 52/441 or more.
  var hullColor = ['#768188','#8c7b7c','#848994','#978d87','#91949f','#a49393','#96a1aa','#ae9b9c','#a4aeb6','#b8a2a6','#c1b0a8','#afb8c5'];
  var CX_STEPS = [1,5,10,20], CHURN_STEPS = [1,3,10,30];
  function rampStep(v, steps) {
    var s = 0;
    for (var i = 0; i < steps.length; i++) if (v >= steps[i]) { s = i + 1; }
    return s;
  }

  // current colour mode — initialized from the baked COLOR_MODE, switched live by the #colorMode select
  var mode = COLOR_MODE;
  function colorForNode(n) {
    if (mode === 'community') return n.comm < 0 ? '#666' : commColor[n.comm % 12];
    if (mode === 'cx') return rampColor[rampStep(n.cx, CX_STEPS)];
    if (mode === 'churn') return CHURN_OK ? rampColor[rampStep(FCHURN[n.file] || 0, CHURN_STEPS)] : '#666';
    if (mode === 'tested') return n.ts ? TESTED_FILL : UNTESTED_FILL;
    return langColor[n.lang] || langColor['?'];
  }

  // C1 — how many LINKS carry the per-edge split-arm flag. Counted ONCE: it is a property of the
  // baked payload, not of the current view, and the legend clause below must not claim a per-view number.
  var AMB_LINKS = 0;
  for (var _k = 0; _k < LINKS.length; _k++) if (LINKS[_k].a) { AMB_LINKS++; }

  // legend for the CURRENT mode, rendered into the #legend span.
  //
  // Every mode now NAMES ITS METRIC AND ITS UNITS. It used to emit a bare `0 1-4 5-9 10-19 20+` — five
  // swatches and five number ranges, with nothing anywhere on the page saying ranges of WHAT. The same
  // five buckets are cyclomatic complexity in one mode and git commits in another, and a reader landing on
  // a screenshot could not tell which, nor that "3-9" meant three commits inside an 18-month window rather
  // than three commits ever. The window is not spelled here either: it comes from CHURN_WINDOW, which the
  // C++ fills with the string it actually handed mineChurnPerFile.
  function legendSwatch(c) { return '<span style="background:' + c + '"></span>'; }
  function legendRing(c) { return '<span style="background:transparent;border:2px dashed ' + c + ';box-sizing:border-box"></span>'; }
  function legendName(t) { return '<span class="lg">' + t + '</span> '; }

  function legendBuckets(labels) {
    var html = '';
    for (var i = 0; i < 5; i++) html += legendSwatch(rampColor[i]) + labels[i] + ' ';
    return html;
  }

  function legendCommunity() {
    var maxComm = -1;
    for (var i = 0; i < NODES.length; i++) if (NODES[i].comm > maxComm) { maxComm = NODES[i].comm; }
    var shown = Math.min(maxComm + 1, hullColor.length);
    var html = legendName('module (community):');
    for (i = 0; i < shown; i++) html += legendSwatch(commColor[i]) + 'm' + i + ' ';
    return html + legendSwatch('#666') + 'none';
  }

  function legendChurn() {
    if (!CHURN_OK) return 'churn unavailable (no git history)';
    return legendName('commits (' + (CHURN_WINDOW || 'window not recorded') + '), per FILE:') +
           legendBuckets(['0','1-2','3-9','10-29','30+']);
  }

  function legendLang() {
    var html = legendName('language:');
    for (var k in langColor) { if (Object.prototype.hasOwnProperty.call(langColor, k)) { html += legendSwatch(langColor[k]) + (k === '?' ? 'unknown' : k) + ' '; } }
    return html;
  }

  function legendHtml() {
    if (mode === 'community') return legendCommunity();
    if (mode === 'cx') return legendName('cyclomatic complexity:') + legendBuckets(['0','1-4','5-9','10-19','20+']);
    if (mode === 'churn') return legendChurn();
    if (mode === 'tested') return legendName('has a test:') + legendSwatch(TESTED_FILL) + 'tested ' + legendRing(UNTESTED_FILL) + 'untested (hollow)';
    return legendLang();
  }

  function renderLegend() {
    var el = document.getElementById('legend');
    var html = legendHtml();
    // C1 EDGE-CONFIDENCE clause. The node swatches above colour the node legend; this names the one thing
    // that is true of the LINES. Emitted only when the payload actually has such an edge — a corpus that
    // resolved cleanly gets no clause, because a legend for a stroke nobody can see is noise, not honesty.
    // The count is of the SELECTED MAP (the top-K subgraph this document baked), not of the whole graph
    // and not of the current view: renderProv's clause is the per-view number, and the two are labelled
    // apart because an ego view draws a handful of these and the map holds all of them.
    if (AMB_LINKS > 0) {
      html += '<span class="ec"> \u2014 dashed edge: the resolver could not choose between same-name'
            + ' definitions and split the call over all of them (' + AMB_LINKS + ' of ' + LINKS.length
            + ' in this map); read the source before trusting one.</span>';
    }
    el.innerHTML = html;
  }

  // ---- global adjacency over ALL selected nodes (NODES/LINKS indices), for BFS ego-graphs and
  // for the "entry points" / cross-link computations. Built once; every view is a FILTERED render
  // over this shared graph, never a re-fetch.
  //
  // IT IS KEPT DIRECTED. LINKS is a directed call graph — `s` is the caller and `t` the callee, built
  // in writeHtml straight off the CSR's outOff/outTargets — and this block used to open by throwing
  // that away: ONE adjacency list, each edge pushed into it from both ends, after which nothing
  // downstream could tell a caller from a callee. Measured on the pages this tool actually emits, the direction
  // discarded there is almost never ambiguous: MUTUAL pairs (A calls B and B calls A, the only case
  // where an undirected edge loses nothing) are 1 of 243 on the default page, 5 of 4721 at --top-k=2000
  // and 0 of 646 on another corpus, and self-calls are 0 everywhere. So 99.6-100% of edges had exactly
  // one true direction and the renderer symmetrised all of them.
  //
  // Two things were wrong with that, and only the second is invisible. draw() had no direction to draw
  // (fixed there, with a head). And egoGraph walked the symmetrised list, so a depth-2 neighbourhood
  // reached "my caller's other callees" by the same step as "my callee's callees" and presented them
  // identically — the sibling-of-a-caller arrives looking like a grandchild. The walk below still
  // crosses both directions, because a symbol's neighbourhood genuinely is its callers AND its callees;
  // what it no longer does is forget which was which, so the node view can state the split.
  var GN = NODES.length, GL = LINKS.length;

  var gout = [], gin = [];
  for (var i = 0; i < GN; i++) { gout.push([]); gin.push([]); }
  for (var k = 0; k < GL; k++) {
    var s = LINKS[k].s, t = LINKS[k].t;
    if (s !== t) { gout[s].push(t); gin[t].push(s); }
  }

  // k-hop BFS ego graph around `centre` (a NODES index), depth 1-3. Returns
  // {ids, edges: [{s,t}], callees, callers} where ids/edges are expressed in ORIGINAL NODES/LINKS index
  // space (the caller remaps to a local 0..n-1 index for the sim) and callees/callers are the DEPTH-1
  // counts in each direction. This is the Sourcetrail-style recenter: computed fresh at every click,
  // over the full in-memory LINKS array — no server round-trip.
  function egoExpand(frontier, seen) {
    var next = [];
    for (var fi = 0; fi < frontier.length; fi++) {
      var around = (gout[frontier[fi]] || []).concat(gin[frontier[fi]] || []);
      for (var j = 0; j < around.length; j++) if (!seen.has(around[j])) { seen.add(around[j]); next.push(around[j]); }
    }
    return next;
  }

  function egoEdges(idSet) {
    var edges = [];
    for (var k = 0; k < GL; k++) {
      // the RECORD, not a fresh {s,t}: a rebuilt pair drops the split-arm bit this edge was emitted
      // with, and an edge that silently loses its own doubt draws solid — a false statement, in the one
      // view where a reader is looking closely at a handful of edges
      if (idSet.has(LINKS[k].s) && idSet.has(LINKS[k].t)) edges.push(LINKS[k]);
    }
    return edges;
  }

  function egoGraph(centre, depth) {
    var seen = new Set([centre]);
    var frontier = [centre];
    var callees = (gout[centre] || []).length, callers = (gin[centre] || []).length;
    for (var d = 0; d < depth; d++) {
      frontier = egoExpand(frontier, seen);
      if (!frontier.length) break;
    }
    var ids = Array.from(seen).sort(function(a,b){ return a-b; });
    return { ids: ids, edges: egoEdges(seen), callees: callees, callers: callers };
  }

  // ---- breadcrumb trail (Sourcetrail "recenter" history): an array of visited #node/ID hashes with
  // a position pointer, so back/forward walk it without re-deriving from browser history (which the
  // hash router also updates, but the trail is the authoritative, always-visible UI). ----
  var trail = [];       // array of nodeIdx
  var trailPos = -1;

  function renderCrumb() {
    if (trailPos < 0) { crumbEl.style.display = 'none'; crumbEl.innerHTML = ''; return; }
    crumbEl.style.display = 'block';
    var html = '';
    html += '<a href="#" data-act="back">← back</a>';
    html += '<a href="#" data-act="fwd">→ forward</a>';
    html += '<span class="sep">|</span>';
    for (var i = 0; i < trail.length; i++) {
      if (i > trailPos) break;   // only show the trail up to the current position
      var n = NODES[trail[i]];
      if (i > 0) html += '<span class="sep">/</span>';
      var cls = (i === trailPos) ? ' style="color:#fff;font-weight:600"' : '';
      html += '<a href="#node/' + trail[i] + '"' + cls + '>' + escHtml(n.label) + '</a>';
    }
    crumbEl.innerHTML = html;
    crumbEl.querySelector('[data-act="back"]').addEventListener('click', function(e){ e.preventDefault(); goBack(); });
    crumbEl.querySelector('[data-act="fwd"]').addEventListener('click', function(e){ e.preventDefault(); goForward(); });
  }
  function pushTrail(nodeIdx) {
    trail = trail.slice(0, trailPos + 1);
    trail.push(nodeIdx);
    trailPos = trail.length - 1;
  }
  function goBack() { if (trailPos > 0) { trailPos--; location.hash = '#node/' + trail[trailPos]; } }
  function goForward() { if (trailPos < trail.length - 1) { trailPos++; location.hash = '#node/' + trail[trailPos]; } }

  function escHtml(s) {
    return String(s).replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/"/g,'&quot;');
  }

  // -- end of section 1 --

  // ---- sim state: rebuilt by each view's render() over a FILTERED node/edge subset. `local` nodes
  // carry {gid, x, y, vx, vy, label, type, lang, rank, comm, cx, ts, file}; `gid` maps back to the
  // NODES index for lookups; the last four feed colorForNode. ----
  var nodes = [], links = [], N = 0, L = 0, nbr = [];
  // Self-calls the sim cannot draw, counted per load (see loadSubset's edge loop) so the caption can
  // state them. A subset view's L was never expected to equal EDGE_TOTAL; the whole-map view's is.
  var selfEdgesDropped = 0;
  var ambEdges = 0;              // in-view edges the resolver could not pin — the caption states the count
  var labelSet = new Set();      // local indices that get a persistent text label (see loadSubset's rule)
  // ---- module HULLS: the groups draw() outlines, rebuilt per load (see loadSubset's hull block).
  var hullGroups = [];           // [{ comm, name, idx: [local indices] }], largest first
  var hullsTotal = 0;            // modules with >= MIN_HULL_MEMBERS in view — the DENOMINATOR the caption states
  var hullsDrawn = 0;            // ...and how many survived draw()'s two geometry tests and the cap
  var hullsThin = 0;             // ...dropped by the SHAPE test (see HULL_COMPACTNESS) — captioned separately,
  var hullsImpure = 0;           // ...and by the PURITY test (see HULL_PURITY): two reasons, two numbers
  var nodesInFrame = 0;          // nodes whose centre is inside the canvas rect — what a ZOOMED export shows,
  var provStamp = '';            // ...against the caption's loaded-subset totals (see draw()'s closing block)
  var MAX_HULLS = 12, MIN_HULL_MEMBERS = 3;
  var HULL_PAD_PX = 16;          // how far the outline stands off its outermost members, in SCREEN px
  // A convex hull only means "these belong together" if the group IS spatially together, and Leiden
  // communities are not always laid out that way: on this repository's own default map the three largest
  // are `size`/`find`/`empty` — name-based hubs whose members are scattered across the whole picture —
  // and their hulls came out as huge overlapping lenses covering most of the canvas. That is not
  // containment, it is a wash, and it made the picture WORSE than no outline at all.
  //
  // The test is PURITY, not size, because purity is what containment actually claims: an outline is
  // drawn only when most of what it encloses belongs to it. Measured over the hull's bounding box rather
  // than the hull itself — a point-in-polygon test per node per group is O(N x groups x vertices) every
  // frame and this is O(N x groups), which is what keeps it affordable at the 5000-node ceiling. The box
  // is the looser test of the two, so it errs toward drawing, never toward a silent drop.
  //
  // HULL_CANDIDATES bounds that cost from the other side: only the largest few groups are ever
  // considered, because they are the only ones whose outline tells a reader anything at this scale.
  var HULL_PURITY = 0.5, HULL_CANDIDATES = 24;
  // ...and the second test is SHAPE, because MIN_HULL_MEMBERS counts POINTS and a hull is a REGION.
  // Three members that happen to sit near a line pass a node count and produce a hull with almost no
  // area: it draws as a thin coloured streak across the picture and reads as a scratch on the lens, not
  // as containment. Three of them were visible on the django/db/migrations figure at rrf/top-k=120 —
  // `resolve_model_field_relations` (a 107x0 px line whose hull area is 4 px^2), `add_operation`
  // (178x8 px) and `reload_model` (137x13 px) — and the first of those was being dropped SILENTLY,
  // because a fully collinear group makes convexHull return a 2-point ring and the caller just skipped
  // it without counting it anywhere.
  //
  // The measure is the isoperimetric ratio 4*pi*A/P^2 of the RAW hull ring. The hull is convex by
  // construction, and on a convex ring that ratio is exactly thinness: 1.0 for a circle, 0.785 for a
  // square, 0.605 for an equilateral triangle — the roundest a three-member hull can be — and it falls
  // to pi/(2*aspect) for a thin one, which reproduces the three measurements above to three decimals.
  // Chosen over the two alternatives on their own terms. An AREA floor is the wrong predicate twice: it
  // is not scale-invariant, and it would drop a small ROUND module (`varint`, 23x18 px, perfectly
  // legible once HULL_PAD_PX has stood the outline off it) while the streaks it is aimed at are long
  // enough to survive one. An OBB aspect ratio measures the same thing but needs rotating calipers,
  // O(ring^2), where this is one O(ring) pass over vertices draw() already walks.
  //
  // 0.20 IS MEASURED, not picked. Over both corpora at the figure argv — 23 groups with 3+ members in
  // view — the ratios form a low cluster {0.0011, 0.0718, 0.1493} and a body from 0.2879 up, and the
  // two widest adjacent gaps in the entire distribution (2.08x and 1.93x) bracket exactly that band;
  // 0.20 is its geometric midpoint. In shape terms it is a triangle about 8:1 base-to-height. Measured
  // on the raw ring rather than the padded one drawn, so it is scale-invariant: HULL_PAD_PX is a screen
  // quantity, and testing the padded shape would make an outline appear and disappear as the reader
  // zooms — and the caption's count change with it.
  var HULL_COMPACTNESS = 0.20;
  var labelDegreeOrder = [];     // the same set as an ARRAY in descending importance, so the declutter in
                                 // draw() places the ones that matter first and drops the collisions
  var MAX_LABELS = 24;
  var MIN_LABEL_DEGREE = 2;      // rule 3: a node with one in-view edge is fringe and its name buys nothing
  // A node's on-screen radius band (see nodeRadiusPx). The FLOOR moved 3.0 -> 4.0 when kind became a
  // shape: shape is only a channel above roughly 8 px across, and a floor of 3.0 drew every zero-degree
  // node — which is every `sec` and every `var`, the two kinds that most need to be told apart from a
  // function — as a 6 px mark where a square, a bar and a circle are the same speck. 4.0 puts the
  // smallest mark at 8 px and leaves the ceiling alone, so the degree signal the band carries is
  // unchanged everywhere it was already visible.
  var MIN_NODE_PX = 4.0, MAX_NODE_PX = 15.0;
  var LABEL_HALO_PX = 3.0;       // the dark outline stroked behind every label (see placeLabel)
  var LABEL_CULL_PX = 2000;      // a label anchored further than this off-canvas is skipped (see placeLabel)
  var LABEL_H = 13;              // how tall a drawn label actually is — placeTextAt reserves rows with it
  var LABEL_GAP_PX = 3;          // placeLabel's gap between a node's edge and the first glyph of its name
  var LABEL_BASE_PX = 4;         // ...and how far BELOW the node centre that name's baseline sits
  // THE INTEGRATOR'S STABILITY LIMIT, and the freeze that came of not having one.
  //
  // The spring term is LINEAR in distance with no cap, so a node's effective stiffness scales with its
  // DEGREE, and explicit Euler at this damping is stable up to roughly degree 100 and unstable past it.
  // Measured on this repository's own map: at --top-k=500 (max degree 70) the layout converges to a
  // 5.7e3 extent; at --top-k=1000 (max degree 130) it grows exponentially — 6.3e3 after one tick, 1.3e94
  // after 280 — and at --top-k=3000 it reaches Infinity outright.
  //
  // What that DID is much worse than an ugly layout. Once a coordinate passes 2^53, adding 1 to it is a
  // no-op in a double, so placeLabel's `for (c = c0; c <= c1; c++)` cell walk stops advancing and NEVER
  // TERMINATES: the tab freezes solid, with no error, no console message and no recovery. It was found
  // by bisection — the same map renders instantly with the sim running and the label block skipped —
  // and it would have been reachable before this view existed, through a depth-3 neighbourhood of any
  // degree-130 hub; a view over the whole map just makes it reachable by opening the page.
  //
  // Capping the DISPLACEMENT one node may take in one tick is the standard remedy and it restores
  // stability rather than merely bounding the damage: at top-k 1000 the peak step falls from 6.7e100 to
  // 6.1e3 and the layout converges to 8.0e3; at top-k 3000, from Infinity to 1.4e4. It binds only during
  // the first violent ticks of a crowded seed — 0.27% of integrations on a 120-node map, whose settled
  // extent moves 2.71e3 to 2.72e3, a difference no picture shows. placeLabel's own cull is the second
  // half: this stops the divergence, that stops any future one from being able to freeze anything.
  var MAX_STEP = 500;            // world units a node may move in one tick
  var ox = 0, oy = 0, scale = 1, autoFit = true;
  var dragging = -1, panStart = null, hovered = -1, selected = -1;
  var searchSet = null;
  var SIM_STEPS = 0, MAX_SIM = 300;
  // The canvas paints its OWN background before anything else. The #111 used to live only on <body>, so the
  // canvas itself was transparent — which is invisible on screen and fatal on export: toDataURL composites
  // onto WHITE, where the #d6d9de labels and the 0.28-alpha edges simply disappear. A picture of this graph
  // was not obtainable from the page that draws it.
  var CANVAS_BG = '#111111';
  // Zoom band. `scale *= factor` was unbounded: a wheel flick reached 0 or Infinity with no way back, and
  // the only recovery was a reload. The floor is below fitView's own 0.05 clamp so a fitted view is never
  // pinned against it; the ceiling is where a 4 px node fills the viewport.
  var SCALE_MIN = 0.02, SCALE_MAX = 8;
  // Pre-paint settling budget (item 4). The sim is 300 O(n^2) steps: 0.2 s at n=239, 0.5 s at n=850, ~11.5 s
  // at n=5000. Running it to completion before the first paint makes the picture appear finished and makes a
  // screenshot reproducible; running it unconditionally would hang the tab on the big end. So it is a budget,
  // and blowing the budget DEGRADES to the old progressive draw rather than freezing — disclosed in the
  // provenance caption, never silent.
  var SETTLE_BUDGET_MS = 2000;
  var settleTimedOut = false;
  // ...and the TOTAL allowance, progressive tail included. The pre-paint budget above bounds how long the
  // page BLOCKS; on its own it bounds nothing about how much work the layout goes on to do, because step()
  // then runs every remaining tick no matter how long they take. That gap was survivable while every view
  // was a subset and only survivable then: at the 5000-node ceiling the remainder is ~20 s of sim (65 ms a
  // tick, measured) plus a full 5000-node/13819-edge redraw per frame, and a page opened there pegged its
  // tab past five minutes — not a degrade, a hang, in the view the page now BOOTS into. Same clock, same
  // flag, same caption: when the allowance is gone the layout stops where it is and renderProv states the
  // step it stopped at, so an under-converged picture is labelled as one rather than passed off as final.
  var LAYOUT_BUDGET_MS = 6000;
  var layoutStopped = false;
  var layoutT0 = 0;
  var DPR = 1;                   // devicePixelRatio at the last resize(); the backing store is scaled by it
  var seed = 42;
  function rng() { seed = (seed * 1664525 + 1013904223) & 0xffffffff; return (seed >>> 0) / 4294967296; }

  // load a subset {ids: [NODES idx...], edges: [{s,t} in NODES idx space]} into the sim, remapped to a
  // local 0..n-1 index space. Resets camera/sim state — called on every view switch.
  function seedNodes(ids) {
    var gidToLocal = new Map();
    nodes = [];
    // seed spread floored at 300 world units: a zero-sized viewport at boot (hidden tab/iframe — W=H=0)
    // used to seed every node at the SAME point, and coincident nodes have dx=dy=0 so the repulsion
    // force is zero forever — the cluster could never separate. autoFit reframes whatever spread we pick.
    // ...and seeded in the VIEWPORT'S OWN ASPECT, not a square. The spread used to be one number,
    // min(W,H)*0.7, so every layout started inside a square and — a force sim being a local optimiser
    // that mostly preserves the gross shape of its seed — settled to a roughly circular cloud. Measured
    // on the README hero at 1600x900: the settled graph used 86% of the canvas HEIGHT and 37% of its
    // WIDTH, so nearly half the picture was empty margin on a 16:9 frame. Seeding in the frame's own
    // proportions costs nothing (the physics is untouched, and the seeded LCG is still the same seeded
    // LCG) and fills it.
    var SPREAD_X = Math.max(W*0.7, 300), SPREAD_Y = Math.max(H*0.7, 300);
    for (var i = 0; i < ids.length; i++) {
      var gid = ids[i], src = NODES[gid];
      gidToLocal.set(gid, i);
      nodes.push({ gid: gid, label: src.label, type: src.type, lang: src.lang, rank: src.rank,
                   comm: src.comm, cx: src.cx, ts: src.ts, file: src.file,
                   x: W/2 + (rng()-0.5)*SPREAD_X, y: H/2 + (rng()-0.5)*SPREAD_Y, vx: 0, vy: 0 });
    }
    N = nodes.length;
    return gidToLocal;
  }

  function buildLinks(edges, gidToLocal) {
    links = [];
    selfEdgesDropped = 0;
    ambEdges = 0;
    for (var k = 0; k < edges.length; k++) {
      var s = gidToLocal.get(edges[k].s), t = gidToLocal.get(edges[k].t);
      if (s === undefined || t === undefined) continue;
      // A self-call is a real edge — EDGE_TOTAL counts it — and a force layout has nowhere to put it: a
      // spring from a node to itself has zero length and zero direction. Dropping it is right; dropping
      // it SILENTLY was fine only while every view was a subset, where nobody expects L to equal
      // EDGE_TOTAL. The whole-map view is the one place a reader checks the caption's two edge counts
      // against each other, so the difference is counted here and stated by renderProv rather than left
      // as an unexplained gap between two numbers on the same screen.
      if (s === t) { selfEdgesDropped++; continue; }
      // C1: the per-edge SPLIT-ARM bit rides through into the sim record. NORMALISED to 0/1 here rather
      // than passed on as undefined-or-1, because two consumers compare against it — draw()'s pass
      // selector and the caption's count — and a record that never carried the key would otherwise make
      // each of them handle the absent case separately.
      var amb = edges[k].a ? 1 : 0;
      if (amb) { ambEdges++; }
      links.push({ s: s, t: t, a: amb });
    }
    L = links.length;
  }

  function buildAdjacency() {
    nbr = [];
    for (var i = 0; i < N; i++) nbr.push([]);
    for (var k = 0; k < L; k++) { nbr[links[k].s].push(links[k].t); nbr[links[k].t].push(links[k].s); }
    // IN-VIEW degree, resolved once per load: it drives label selection AND node radius, and both used to
    // read `rank` instead — a GLOBAL score that says nothing about this view.
    for (var j = 0; j < N; j++) { nodes[j].deg = nbr[j].length; }
  }

  // ---- persistent labels. Three rules, and the measurement that chose them.
  //
  // The old rule was "top 24 by rank". On this repository's own README cut — the depth-2 neighbourhood of
  // lexicalScoresTiered, 239 nodes — it spent 22 of its 24 labels on container methods (size, empty, buf,
  // push_back, find, clear, end, reserve, begin, emplace_back, back, pop_back, grow, min), 13 of them from
  // src/infra/svector.h alone, and left exactly 2 for functions this repository is actually about.
  //
  // Ranking by in-view DEGREE alone does not fix that, and neither does rank x degree: those container
  // methods ARE the hubs (size has in-view degree 111, push_back 59, empty 49), so both rules re-elect
  // them. The measurable culprit is something else — 13 NAMES covered 28 of those nodes. Three different
  // `find`, three `empty`, two each of `buf`/`end`/`begin`/`back`/`data`/`push_back`: the label set was
  // spending its budget printing the same word again and again over different symbols, which is not only
  // clutter but ambiguous (nothing on the page said which `find`).
  //
  //   1. order by IN-VIEW degree (rank breaks ties) — importance in THIS picture, not in the repository
  //   2. at most one label per distinct NAME — the rule that actually frees the budget
  //   3. drop degree-1 nodes — a leaf with one edge is fringe; its name costs a slot and explains nothing
  //
  // Measured on that same cut: 24 labels over 24 distinct names, and 11 of them domain functions
  // (lexicalScoresTiered, namingLensChecks, resolveAtSeed, gitLogFileSets, buildPreciseIncludeAdj,
  // lexicalScoresNameExactTiered, getIndex, resolveAllByNameQualified, declaredFieldsFor,
  // joinNormalizeLookup, lexicalNormalize) against 2 before. rank x degree scored 1.
  //
  // Rule 2 hides symbols, so the page must SAY so rather than let a reader infer "one find exists": the
  // provenance caption states the rule, the tooltip and info line name the FILE, and hover/search/select
  // still label any node on demand. The centre of a #node view is always labelled (draw() forces it).
  function selectLabels() {
    labelSet = new Set();
    labelDegreeOrder = [];
    var byDegree = [];
    for (var i = 0; i < N; i++) { if (nodes[i].deg >= MIN_LABEL_DEGREE) { byDegree.push(i); } }
    byDegree.sort(function(a,b){ return nodes[b].deg - nodes[a].deg || nodes[b].rank - nodes[a].rank || a - b; });
    var labelSeenNames = new Set();
    for (var j = 0; j < byDegree.length && labelDegreeOrder.length < MAX_LABELS; j++) {
      var li = byDegree[j];
      if (labelSeenNames.has(nodes[li].label)) continue;
      labelSeenNames.add(nodes[li].label);
      labelDegreeOrder.push(li);
      labelSet.add(li);
    }
  }

  // ---- module HULLS: which groups get an outline.
  //
  // WHAT THIS REPLACES. Module identity was carried by hue alone, `commColor[comm % 12]` — twelve
  // colours over 26 modules on this repository's own default page, 31 on the README hero, and 299 and
  // 188 on two larger corpora. At --top-k=2000 that is 24-25 distinct modules sharing every hue, while
  // the legend printed "m0 ... m11" and said nothing about the collision, so two adjacent
  // same-coloured clusters read as one module and there was no way to tell from the picture that they
  // were not. Twelve categorical hues is roughly the ceiling for a colour channel and the module count
  // is unbounded, so no palette fixes this: CONTAINMENT is the only channel that can honestly express
  // 26-299 groups, and it is also the single largest "this hairball has structure" win available.
  //
  // The outline is independent of --color-by. It says WHICH MODULE; the fill inside still says
  // whatever lens the reader picked, so a `cx` view shows hot symbols AND the module boundaries they
  // sit inside instead of making the reader choose.
  //
  // A group needs MIN_HULL_MEMBERS points before a hull means anything (two points are a line), and at
  // most MAX_HULLS are drawn because 26 overlapping outlines is the hairball again in a second
  // channel. Both the cap and the total are handed to the caption — an undisclosed cap would be this
  // page implying it had drawn every module.
  function buildHullGroups() {
    hullGroups = [];
    hullsTotal = 0;
    if (typeof MODULES === 'undefined' || !MODULES.length) return;
    var byComm = new Map();
    for (var hi = 0; hi < N; hi++) {
      var hc = nodes[hi].comm;
      if (hc < 0 || hc >= MODULES.length) { continue; }
      if (!byComm.has(hc)) { byComm.set(hc, []); }
      byComm.get(hc).push(hi);
    }
    var groups = [];
    byComm.forEach(function(idx, c) {
      if (idx.length >= MIN_HULL_MEMBERS) { groups.push({ comm: c, name: MODULES[c].name, idx: idx }); }
    });
    hullsTotal = groups.length;
    groups.sort(function(a,b){ return b.idx.length - a.idx.length || a.comm - b.comm; });
    hullGroups = groups.slice(0, HULL_CANDIDATES);   // draw() applies the purity test and the cap — both need geometry
  }

  function loadSubset(ids, edges) {
    seed = 42;
    var gidToLocal = seedNodes(ids);
    buildLinks(edges, gidToLocal);
    buildAdjacency();
    selectLabels();
    buildHullGroups();

    ox = 0; oy = 0; scale = 1; autoFit = true;
    dragging = -1; panStart = null; hovered = -1; selected = -1; searchSet = null;
    SIM_STEPS = 0;
    info.textContent = '';
    if (N === 0) { paintBackdrop(); ctx.fillStyle='#888'; ctx.font='18px sans-serif'; ctx.fillText('No nodes', 40, 40); renderProv(); return; }
    settle();
    renderProv();
  }

  // frame everything the page DRAWS into the viewport. Called each settling frame (while autoFit) so the
  // graph is visible no matter how far the force sim spreads it; the user taking control stops it.
  //
  // It used to fit the bounding box of node CENTRES against a flat 70 px pad, and that is not the same
  // set as what lands on the canvas. A node is a disc of up to MAX_NODE_PX, and a labelled node also
  // carries its name to the RIGHT, in screen-constant 11 px text — `generate_deleted_fields` is 130 px
  // of glyphs that the centre-box knows nothing about. The flat pad absorbed the discs and roughly one
  // short name, so the defect only appears when a wide label happens to sit on the right edge: the
  // README's own lens figure shipped with a name sliced in half by the frame. Worse, it is not fixable
  // downstream — the frame is what the stated command produces, so a hand-panned screenshot would be a
  // picture the README's own reproduction line does not make.
  //
  // Reserve the drawn EXTENT per node instead, and solve for the largest scale that fits all of them.
  // The extents are screen-constant (radii and label metrics are both in screen px, deliberately — see
  // placeTextAt), so they do not move as `scale` does, which is what makes the solve exact rather than
  // iterative-until-it-looks-right: for a given s, node i occupies [x_i*s + ox - Lft_i, x_i*s + ox +
  // Rgt_i], so the admissible ox is an interval, feasibility is that interval being non-empty, and
  // feasibility is monotone in s. Binary-search s, then take the interval's midpoint, which centres the
  // drawn content — not the centres — in the frame.
  //
  // Hull NAMES are not reserved: their anchors are computed in screen space during the hull pass, which
  // needs the camera this function is choosing. The right margin a labelled node reserves absorbs most
  // of that, and a clipped hull name is one word of a region title rather than a symbol's identity.
  // The frame's own breathing room, on top of every reserved box — a FRACTION of the shorter side, not a
  // pixel count, so a thumbnail and a full-width figure get the same visual air rather than the same
  // number of pixels. A fixed pad reads as generous at 430 px and as a hairline at 1600. Floored so a
  // very small canvas still gets a margin at all; the graph is meant to sit IN the frame, not against it.
  var FIT_PAD_FRAC = 0.13, FIT_PAD_MIN_PX = 18;
  function fitPad() { return Math.max(FIT_PAD_MIN_PX, FIT_PAD_FRAC*Math.min(W, H)); }
  var FIT_SCALE_MIN = 0.05, FIT_SCALE_MAX = 2.0;

  function fitBoxes() {
    ctx.font = '11px sans-serif';                       // placeLabel's font — measure what IT will draw
    var b = { lft: new Float64Array(N), rgt: new Float64Array(N), top: new Float64Array(N), bot: new Float64Array(N) };
    for (var i = 0; i < N; i++) {
      var rp = nodeRadiusPx(nodes[i]);
      b.lft[i] = rp; b.rgt[i] = rp; b.top[i] = rp; b.bot[i] = rp;
    }
    for (var li = 0; li < labelDegreeOrder.length; li++) {
      // The declutter grid may still drop some of these; reserving for a label that is then skipped only
      // zooms out a hair, while NOT reserving for one that is drawn slices it. Over-reserve, deliberately.
      var k = labelDegreeOrder[li], rpk = nodeRadiusPx(nodes[k]);
      b.rgt[k] = Math.max(b.rgt[k], rpk + LABEL_GAP_PX + ctx.measureText(nodes[k].label).width);
      b.top[k] = Math.max(b.top[k], LABEL_H - LABEL_BASE_PX);
      b.bot[k] = Math.max(b.bot[k], LABEL_BASE_PX + 2);
    }
    return b;
  }

  // The admissible offset interval on one axis at scale s, or null when no offset fits every box.
  function fitOffsetRange(b, s, pad, axisY, extent) {
    var lo = -Infinity, hi = Infinity;
    for (var i = 0; i < N; i++) {
      var q = ( axisY ? nodes[i].y : nodes[i].x ) * s;
      var before = axisY ? b.top[i] : b.lft[i], after = axisY ? b.bot[i] : b.rgt[i];
      var l = pad + before - q;                      if ( l > lo ) { lo = l; }
      var h = extent - pad - after - q;              if ( h < hi ) { hi = h; }
    }
    return lo <= hi ? [lo, hi] : null;
  }

  function fitFits(b, pad, s) {
    return fitOffsetRange(b, s, pad, false, W) !== null && fitOffsetRange(b, s, pad, true, H) !== null;
  }

  // PADDING IS A PREFERENCE; CONTAINMENT IS THE PROPERTY. On a narrow canvas one long name can be most
  // of the width — `_get_altered_foo_together_operations` measures ~200 px against a 430 px figure — so
  // the proportional pad plus that label can be wider than the frame, at which point NO scale fits and
  // the graph would fall back to the old clipping fit. Give the pad up before giving up containment:
  // halve it until a fit exists. The generous margin survives wherever there is room for it, which is
  // every full-width figure, and a cramped thumbnail loses air rather than losing a symbol's name.
  function fitPadFor(b) {
    var pad = fitPad();
    while (pad > 0.5 && !(fitOffsetRange(b, FIT_SCALE_MIN, pad, false, W) && fitOffsetRange(b, FIT_SCALE_MIN, pad, true, H))) { pad /= 2; }
    if (pad <= 0.5) { pad = 0; }
    return pad;
  }

  // Even the floor cannot hold the drawn boxes (a canvas smaller than one label, say). Fall back to
  // the centre-box fit rather than emitting a NaN camera, and let the picture overflow visibly.
  function fitCentreBox() {
    var minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
    for (var j = 0; j < N; j++) {
      var n = nodes[j];
      if (n.x < minX) minX = n.x;  if (n.x > maxX) maxX = n.x;
      if (n.y < minY) minY = n.y;  if (n.y > maxY) maxY = n.y;
    }
    scale = FIT_SCALE_MIN;
    ox = (W - scale*(minX+maxX))/2;
    oy = (H - scale*(minY+maxY))/2;
  }

  function fitScale(b, pad) {
    var loS = FIT_SCALE_MIN, hiS = FIT_SCALE_MAX;
    if (!fitFits(b, pad, hiS)) {
      for (var it = 0; it < 24; it++) {                 // 2.0 halved 24 times resolves scale to ~1e-7
        var mid = (loS + hiS)/2;
        if (fitFits(b, pad, mid)) { loS = mid; } else { hiS = mid; }
      }
    } else {
      loS = hiS;                                       // a tiny graph: the 2.0 ceiling is the binding limit
    }
    return loS;
  }

  function fitView() {
    if (!N) return;
    var b = fitBoxes();
    var pad = fitPadFor(b);
    if (!fitFits(b, pad, FIT_SCALE_MIN)) { fitCentreBox(); return; }
    scale = fitScale(b, pad);
    var rx = fitOffsetRange(b, scale, pad, false, W), ry = fitOffsetRange(b, scale, pad, true, H);
    ox = (rx[0] + rx[1])/2;                            // centre the DRAWN content, not the centres
    oy = (ry[0] + ry[1])/2;
  }

  // --- simulation ---
  // simTick() is ONE integration step and paints nothing. It was previously fused with the draw + rAF
  // driver, which is why the page could only ever be watched settling: there was no way to ask for the
  // finished layout. settle() below runs it in a tight loop; step() keeps the progressive path for the
  // over-budget case.
  // repulsion O(n²) — the hottest loop on the page. Node i's position and its two force accumulators
  // are invariant in j and are lifted out of the inner loop. The accumulators are the load-bearing
  // half: N-1 typed-array read-modify-writes per i collapse to one read and one write.
  //
  // Lifting ax[i] is SAFE for a reason worth stating, because it looks like it should not be. ax[i] is
  // also written by EARLIER outer iterations, as ax[j] with j == i — but every i' < i has finished
  // before iteration i begins, and iteration i writes only ax[j] for j > i. So the addition ORDER is
  // unchanged, and a Float64Array element is a double either way: the result is bit-identical, verified
  // by running the emitted page's own script with and without these hoists and comparing every drawn
  // node centre (equal step counts in both arms — settle() stops on budget OR MAX_SIM, and the faster
  // arm otherwise simply gets further, which reads as a difference and is not one).
  //
  // Bit-identity is the bar rather than "close enough" because this layout is chaotic: one ulp of
  // difference is two different pictures 300 steps later, and the label rule in loadSubset and the
  // radius rule in nodeRadiusPx were both calibrated on specific settled pictures. A rewrite that
  // REORDERS these sums is not free even when it is faster — see the integrate loop below.
  //
  // WHY THIS STAYS O(n²). Barnes-Hut was implemented and measured against this loop, and declined.
  // The tempting summary — "BH is 3-8x faster but still misses SETTLE_BUDGET_MS at n=5000" — is
  // circular, because that budget is a constant in this file chosen for how long a page may block
  // before painting, not a yardstick for an algorithm. Measured in the unit that means something,
  // median node displacement from that arm's OWN settled layout at first paint (n=5000), the exact
  // loop lands 6.3% of the graph's span from settled and BH 0.3%: BH genuinely wins at the ceiling,
  // and this is a trade rather than a rout. It is declined because the benefit is confined to the
  // --top-k=5000 ceiling while the cost is paid at every n — BH is an APPROXIMATION, so it perturbs
  // every layout including the default 200-node picture the label and radius rules were calibrated
  // on, and it puts ~120 lines of quadtree into a page whose value is being self-contained and
  // auditable. The hoists directly above bought 57 -> 141 steps at that same ceiling (2.45x) for no
  // layout change at all, which is the cheaper half of the same win. If the whole-map view at 5000
  // nodes ever becomes the common case rather than a reachable one, BH is the right next step and
  // these are the numbers to start from. Step counts are wall-clock and machine-dependent; the
  // ratios and the bit-identity are not.
  function simRepel(ax, ay) {
    var repulse = 1500;
    for (var i = 0; i < N; i++) {
      var ni = nodes[i], nix = ni.x, niy = ni.y, axi = ax[i], ayi = ay[i];
      for (var j = i+1; j < N; j++) {
        var nj = nodes[j];
        var dx = nix - nj.x, dy = niy - nj.y;
        var d2 = dx*dx + dy*dy + 1;
        var f = repulse / d2;
        var fx = f*dx, fy = f*dy;
        axi += fx; ayi += fy;
        ax[j] -= fx; ay[j] -= fy;
      }
      ax[i] = axi; ay[i] = ayi;
    }
  }

  // spring along links — links[k], nodes[s] and nodes[t] were each loaded twice per edge
  function simSprings(ax, ay) {
    var spring = 0.04, rest = 80;
    for (var k = 0; k < L; k++) {
      var lk = links[k], s = lk.s, t = lk.t, ns = nodes[s], nt = nodes[t];
      var dx = nt.x - ns.x, dy = nt.y - ns.y;
      var d = Math.sqrt(dx*dx+dy*dy)+0.001;
      var f = spring*(d-rest)/d;
      var gx = f*dx, gy = f*dy;
      ax[s] += gx; ay[s] += gy;
      ax[t] -= gx; ay[t] -= gy;
    }
  }

  // Gravity to centre, in the VIEWPORT'S OWN PROPORTIONS. An isotropic pull produces a circular
  // cloud, and this page is drawn on a 16:9 canvas: measured on the README hero at 1600x900, the
  // settled graph used 86% of the height and 37% of the width, so the flagship picture was almost half
  // empty margin. Seeding in the frame's aspect (loadSubset) starts it right and an isotropic well
  // pulls it back round over 300 steps, so the well is elliptical too — the pull is weaker along the
  // long axis. The RATIO is the calibration and it was measured, not derived: at gravY/gravX = (W/H)^2
  // the settled cloud came out at aspect 3.6 in a 1.98 frame, i.e. the cloud's aspect tracks the
  // gravity ratio almost linearly rather than as the g^(-1/3) a linear-force-against-1/r^2 argument
  // predicts (the springs dominate). So the ratio is set to the FRAME's aspect, which lands the cloud
  // just inside it and leaves a margin instead of stretching the picture flat.
  //
  // WHAT IT COSTS, stated because it is a real cost. A force layout's one claim is "near means
  // related", and it is rotation-invariant: an anisotropic well makes "near" mildly direction-
  // dependent, so a horizontal gap and a vertical gap of the same pixel length no longer mean quite
  // the same thing. The exponent keeps that mild at the aspects a browser window actually has, and it
  // is the deliberate trade for a picture that uses the frame it is drawn in.
  function simGravity(ax, ay) {
    var gravity = 0.025;
    var cx = W/2, cy = H/2;
    var wellA = Math.sqrt(Math.max(0.4, Math.min(2.5, (W > 0 && H > 0) ? W/H : 1)));
    var gravX = gravity/wellA, gravY = gravity*wellA;
    for (var i = 0; i < N; i++) {
      var ng = nodes[i];
      ax[i] += gravX*(cx-ng.x);
      ay[i] += gravY*(cy-ng.y);
    }
  }

  // integrate, with the per-tick displacement capped at MAX_STEP (see its declaration for the
  // exponential divergence this bounds, and the frozen tab that divergence produced)
  // nodes[i] was loaded four times per node. The reciprocal in the clamp is deliberately left ALONE:
  // q = MAX_STEP/sp then vx *= q is one division instead of two, but a*(b/c) and a*b/c differ by an
  // ulp, so unlike the three hoists above it is not value-preserving. It is already the shipped
  // behaviour and it binds on a fraction of a percent of integrations, so this is a note and not a
  // change — only that the two are different KINDS of edit, and the difference is the reason the
  // hoists above could be verified bit-identical and this one could not be.
  function simIntegrate(ax, ay) {
    var dampen = 0.82;
    for (var i = 0; i < N; i++) {
      if (i === dragging) continue;
      var nv = nodes[i];
      var vx = (nv.vx + ax[i])*dampen, vy = (nv.vy + ay[i])*dampen;
      var sp = Math.sqrt(vx*vx + vy*vy);
      if (sp > MAX_STEP) { var q = MAX_STEP/sp; vx *= q; vy *= q; }
      nv.vx = vx; nv.vy = vy;
      nv.x += vx;  nv.y += vy;
    }
  }

  function simTick() {
    SIM_STEPS++;
    var ax = new Float64Array(N), ay = new Float64Array(N);
    simRepel(ax, ay);
    simSprings(ax, ay);
    simGravity(ax, ay);
    simIntegrate(ax, ay);
  }

  function now() { return (window.performance && window.performance.now) ? window.performance.now() : Date.now(); }

  // Run the whole sim before the first paint, under a wall-clock budget. Over budget, hand the rest to the
  // progressive rAF path so a very large graph degrades instead of hanging the tab (non-negotiable: a
  // recoverable limit is a degrade, and it is disclosed — renderProv prints "settling…" while it is true).
  function settle() {
    layoutT0 = now();
    settleTimedOut = false;
    layoutStopped = false;
    while (SIM_STEPS < MAX_SIM) {
      simTick();
      if (now() - layoutT0 > SETTLE_BUDGET_MS) { settleTimedOut = true; break; }
    }
    if (autoFit) fitView();
    draw();
    if (settleTimedOut) requestAnimationFrame(step);
  }

  // the progressive driver — the pre-settle behaviour, now reached only past the pre-paint budget, and
  // itself bounded by the TOTAL allowance (see LAYOUT_BUDGET_MS). It watches the same clock settle()
  // started, so "2 s before the first paint, 6 s of layout in all" is one budget with two checkpoints
  // rather than two budgets that can disagree.
  function step() {
    if (SIM_STEPS >= MAX_SIM) { settleTimedOut = false; renderProv(); return; }
    if (now() - layoutT0 > LAYOUT_BUDGET_MS) {
      layoutStopped = true; settleTimedOut = false;
      if (autoFit) fitView();
      draw();
      renderProv();
      return;
    }
    simTick();
    if (autoFit) fitView();
    draw();
    requestAnimationFrame(step);
  }

  // -- end of section 2 --

  // ---- node SIZE. Radius reads IN-VIEW DEGREE, with rank as a tiebreak. The old `4 + 60*sqrt(rank)`
  // spanned 4.60-11.78 px with a MEAN of 5.10 on the README cut — every node a ~5 px dot — while in-view
  // degree over the same nodes spanned 1 to 111. The picture carried a hub/leaf distinction it never
  // drew, so a hairball was the honest rendering of it. sqrt keeps the growth sub-linear so a degree-111
  // hub is ~4x a degree-2 leaf and not 55x; the small rank term separates equal-degree nodes without ever
  // reordering different-degree ones.
  function nodeRadiusPx(n) { return Math.max(MIN_NODE_PX, Math.min(MAX_NODE_PX, 3 + 1.5*Math.sqrt(n.deg || 0) + 5*Math.sqrt(n.rank))); }

  // ---- node SHAPE, one table, two readers.
  //
  // SYM_SHAPES (emitted above — htmlexport.h::kSymShapes, one entry per model.h SymKind enumerator,
  // static_asserted against the roster) maps the `type` string every NODES record already carries to a
  // key of this object; each entry carries BOTH the canvas path that draws the mark and the glyph the
  // caption's shape key prints for it, so the picture and its own legend cannot describe different
  // shapes. `type` was in the payload from the beginning and reached nothing but a hover tooltip.
  //
  // Every path is written in terms of one radius `r` and normalised to the SAME AREA as a circle of that
  // radius, so shape says kind and only size says degree. Without that a square would read as a bigger
  // node than a circle at identical degree — a second variable smuggled into a nominal channel:
  //   square   half-side  0.89r   (4q^2      = pi r^2)
  //   diamond  half-diag  1.25r   (2d^2      = pi r^2)
  //   triangle circumrad  1.55r   (1.299R^2  = pi r^2)
  //   bar      1.48r x 0.53r      (4wh       = pi r^2, at 2.8:1 so it reads as a rule and not a box)
  //   plus/cross  arm half-thickness 0.40r, half-length 1.18r   (8ab - 4a^2 = pi r^2)
  var PLUS_PTS = [[-0.40,-1.18],[0.40,-1.18],[0.40,-0.40],[1.18,-0.40],[1.18,0.40],[0.40,0.40],
                  [0.40,1.18],[-0.40,1.18],[-0.40,0.40],[-1.18,0.40],[-1.18,-0.40],[-0.40,-0.40]];
  function plusPath(p, x, y, r, rot) {
    for (var pi = 0; pi < 12; pi++) {
      var dx = PLUS_PTS[pi][0]*r, dy = PLUS_PTS[pi][1]*r;
      if (rot) { var tq = (dx - dy)*0.70710678; dy = (dx + dy)*0.70710678; dx = tq; }
      if (pi) { p.lineTo(x + dx, y + dy); } else { p.moveTo(x + dx, y + dy); }
    }
    p.closePath();
  }
  var SHAPES = {
    circle:   { glyph: '●', reach: 1.00, path: function(p,x,y,r){ p.moveTo(x+r, y); p.arc(x, y, r, 0, 2*Math.PI); } },
    diamond:  { glyph: '◆', reach: 1.25, path: function(p,x,y,r){ var q = r*1.25; p.moveTo(x, y-q); p.lineTo(x+q, y); p.lineTo(x, y+q); p.lineTo(x-q, y); p.closePath(); } },
    square:   { glyph: '■', reach: 1.26, path: function(p,x,y,r){ var q = r*0.89; p.moveTo(x-q, y-q); p.lineTo(x+q, y-q); p.lineTo(x+q, y+q); p.lineTo(x-q, y+q); p.closePath(); } },
    triangle: { glyph: '▲', reach: 1.55, path: function(p,x,y,r){ var q = r*1.55; p.moveTo(x, y-q); p.lineTo(x + q*0.866, y + q*0.5); p.lineTo(x - q*0.866, y + q*0.5); p.closePath(); } },
    bar:      { glyph: '▬', reach: 1.57, path: function(p,x,y,r){ var w = r*1.48, h = r*0.53; p.moveTo(x-w, y-h); p.lineTo(x+w, y-h); p.lineTo(x+w, y+h); p.lineTo(x-w, y+h); p.closePath(); } },
    plus:     { glyph: '✚', reach: 1.25, path: function(p,x,y,r){ plusPath(p, x, y, r, 0); } },
    cross:    { glyph: '✖', reach: 1.25, path: function(p,x,y,r){ plusPath(p, x, y, r, 1); } }
  };
  // A node's mark. The `|| SHAPES.circle` is a rendering guard, not a policy: kSymShapes is
  // static_asserted to cover every SymKind, so it can only fire on a page whose payload predates a new
  // enumerator — and the caption's key is built from the same lookup, so such a page would say so.
  function shapeFor(n) { return SHAPES[SYM_SHAPES[n.type]] || SHAPES.circle; }

  // ---- module hull geometry.
  //
  // Monotone-chain convex hull over [{x,y}] — O(n log n), and free beside the O(n^2) force sim that
  // placed the points in the first place. Returns the hull ring; fewer than three non-collinear points
  // has no hull, and the caller skips those groups rather than drawing a line and calling it a region.
  function convexHull(pts) {
    if (pts.length < 3) { return []; }
    var p = pts.slice().sort(function(a,b){ return a.x - b.x || a.y - b.y; });
    function cross(o,a,b){ return (a.x-o.x)*(b.y-o.y) - (a.y-o.y)*(b.x-o.x); }
    var lo = [], up = [], i;
    for (i = 0; i < p.length; i++) {
      while (lo.length >= 2 && cross(lo[lo.length-2], lo[lo.length-1], p[i]) <= 0) { lo.pop(); }
      lo.push(p[i]);
    }
    for (i = p.length - 1; i >= 0; i--) {
      while (up.length >= 2 && cross(up[up.length-2], up[up.length-1], p[i]) <= 0) { up.pop(); }
      up.push(p[i]);
    }
    lo.pop(); up.pop();
    return lo.concat(up);
  }
  // '#rrggbb' + alpha -> 'rgba(r,g,b,a)'. The palettes are hex because that is the form a CSS legend
  // swatch needs; a translucent canvas fill needs the channels.
  function hexRgba(hex, a) {
    var v = parseInt(hex.slice(1), 16);
    return 'rgba(' + ((v >> 16) & 255) + ',' + ((v >> 8) & 255) + ',' + (v & 255) + ',' + a + ')';
  }

  // -- end of section 3a --

  // The canvas's own background, painted as the FIRST op of every frame. clearRect leaves transparent
  // pixels; on screen the body's #111 shows through and it looks fine, but every export path (toDataURL,
  // the PNG button, a browser "save image") composites transparency onto white, where this page's light
  // labels and 28%-alpha edges vanish. Painting it is what makes the picture exportable at all.
  function paintBackdrop() {
    ctx.setTransform(DPR, 0, 0, DPR, 0, 0);
    ctx.fillStyle = CANVAS_BG;
    ctx.fillRect(0, 0, W, H);
  }

  function highlightSet() {
    if (selected < 0) return null;
    var hl = new Set(nbr[selected]);
    hl.add(selected);
    return hl;
  }

  // ---- module HULLS, behind everything. Containment is the channel; see loadSubset's hull block for
  // the 12-hues-over-26-modules collision it replaces and why no palette could have fixed it. The hue
  // is what is LEFT after containment took the job, so it comes from hullColor — a deliberately quiet
  // band measured against rampColor rather than the community lens' twelve saturated hues, which put
  // decoration on the two axes the metric uses. See hullColor's note for the numbers.
  //
  // Each outline is pushed HULL_PAD_PX (a screen quantity, converted here like every other one on this
  // canvas) outward from the group's centroid so it clears its own members instead of threading
  // through them, and it is drawn as a rounded blob — quadratic curves through the edge midpoints,
  // one pass over the same vertices — because a hard polygon reads as a diagram of a region and a
  // rounded one reads as the region. The fill is deliberately faint: the outline is a second channel
  // sitting UNDER whatever --color-by the reader chose, and a hull that competes with the node colours
  // has taken the lens away from them.
  function hullGroupPoints(grp) {
    var gpts = [];
    for (var mi = 0; mi < grp.idx.length; mi++) { gpts.push({ x: nodes[grp.idx[mi]].x, y: nodes[grp.idx[mi]].y }); }
    return gpts;
  }

  // THE SHAPE TEST — see HULL_COMPACTNESS for the three streaks that made it necessary and for why
  // 0.20. It runs FIRST because it is one O(ring) pass and the purity test below is O(N): a group
  // that cannot be a region at all never costs a walk over every node in view. A ring of fewer than
  // three points is the extreme case of the same defect — a fully collinear group, whose hull has no
  // area whatsoever — and it lands in the same counted branch, where it used to be skipped silently.
  function hullIsThin(ring) {
    var hullArea = 0, hullPerim = 0, hk;
    for (hk = 0; hk < ring.length; hk++) {
      var rb = ring[(hk + 1) % ring.length], rdx = rb.x - ring[hk].x, rdy = rb.y - ring[hk].y;
      hullArea  += ring[hk].x*rb.y - rb.x*ring[hk].y;
      hullPerim += Math.sqrt(rdx*rdx + rdy*rdy);
    }
    hullArea = Math.abs(hullArea)/2;
    return ring.length < 3 || !(hullPerim > 0) || 4*Math.PI*hullArea/(hullPerim*hullPerim) < HULL_COMPACTNESS;
  }

  function hullOutline(ring) {
    var gcx = 0, gcy = 0, hj;
    for (hj = 0; hj < ring.length; hj++) { gcx += ring[hj].x; gcy += ring[hj].y; }
    gcx /= ring.length; gcy /= ring.length;
    var padw = HULL_PAD_PX/scale, ex = [], topY = Infinity, topX = 0;
    for (hj = 0; hj < ring.length; hj++) {
      var vx = ring[hj].x - gcx, vy = ring[hj].y - gcy, vd = Math.sqrt(vx*vx + vy*vy) || 1;
      var pxw = ring[hj].x + vx/vd*padw, pyw = ring[hj].y + vy/vd*padw;
      ex.push({ x: pxw, y: pyw });
      if (pyw < topY) { topY = pyw; topX = pxw; }
    }
    return { ex: ex, topX: topX, topY: topY };
  }

  // THE PURITY TEST — see HULL_PURITY for the three lenses that made it necessary. If most of what
  // this outline would enclose is NOT this module, the outline is not saying "these belong
  // together", it is saying nothing over the top of everything else.
  function hullIsPure(grp, ex) {
    var bx0 = Infinity, by0 = Infinity, bx1 = -Infinity, by1 = -Infinity;
    for (var hj = 0; hj < ex.length; hj++) {
      if (ex[hj].x < bx0) bx0 = ex[hj].x;  if (ex[hj].x > bx1) bx1 = ex[hj].x;
      if (ex[hj].y < by0) by0 = ex[hj].y;  if (ex[hj].y > by1) by1 = ex[hj].y;
    }
    var inside = 0;
    for (var qi = 0; qi < N; qi++) {
      var qn = nodes[qi];
      if (qn.x >= bx0 && qn.x <= bx1 && qn.y >= by0 && qn.y <= by1) { inside++; }
    }
    return !(inside > 0 && grp.idx.length/inside < HULL_PURITY);
  }

  function strokeHullRegion(ex, hcol) {
    ctx.beginPath();
    ctx.moveTo((ex[ex.length-1].x + ex[0].x)/2, (ex[ex.length-1].y + ex[0].y)/2);
    for (var hj = 0; hj < ex.length; hj++) {
      var nxt = ex[(hj + 1) % ex.length];
      ctx.quadraticCurveTo(ex[hj].x, ex[hj].y, (ex[hj].x + nxt.x)/2, (ex[hj].y + nxt.y)/2);
    }
    ctx.closePath();
    ctx.fillStyle = hexRgba(hcol, 0.085);
    ctx.fill();
    ctx.strokeStyle = hexRgba(hcol, 0.42);
    ctx.lineWidth = 1.4/scale;
    ctx.stroke();
  }

  // Returns the name anchor for a drawn hull, or null when the group is dropped.
  // The name is a module's TOP-RANKED MEMBER (MODULES[].name — the same string the overview card
  // uses), so on a repository whose hubs are container primitives it can read as an ordinary symbol
  // name, and the picture already has a node labelled exactly that a few pixels away. The member
  // count is what makes the two legible as different KINDS of thing — "clone ·9" is a group of
  // nine, "clone" is a symbol — and it is the number a reader wants from a region anyway.
  function drawHull(grp) {
    var ring = convexHull(hullGroupPoints(grp));
    if (hullIsThin(ring)) { hullsThin++; return null; }
    if (hullsDrawn >= MAX_HULLS) return null;
    var shape = hullOutline(ring);
    if (!hullIsPure(grp, shape.ex)) { hullsImpure++; return null; }
    hullsDrawn++;
    var hcol = hullColor[grp.comm % 12];
    strokeHullRegion(shape.ex, hcol);
    return { sx: shape.topX*scale + ox, sy: shape.topY*scale + oy - 6,
             text: grp.name + ' ·' + grp.idx.length, color: hcol };
  }

  function drawHulls() {
    var anchors = [];
    hullsDrawn = 0; hullsThin = 0; hullsImpure = 0;
    for (var gi = 0; gi < hullGroups.length; gi++) {
      var anchor = drawHull(hullGroups[gi]);
      if (anchor) { anchors.push(anchor); }
    }
    return anchors;
  }

  // ---- edges, WITH THEIR DIRECTION DRAWN.
  //
  // LINKS is a directed call graph and this loop used to render it as `moveTo(source) lineTo(target)`
  // with nothing at either end, so a picture of a call graph could not answer "which of these two
  // calls the other" — the single most basic question the graph exists to answer. It is a near-free
  // thing to draw here because the direction is near-unambiguous in the data: mutual pairs are 1 of
  // 243 on the default page, 5 of 4721 at --top-k=2000, 0 of 646 on another corpus, and self-calls
  // (which loadSubset drops and the caption counts) are 0 everywhere. A head is therefore a true
  // statement about 99.6-100% of the edges it is drawn on.
  //
  // THE HEAD IS A SCREEN QUANTITY, converted to world units here — the same rule node radii and label
  // glyphs already follow, and for the same reason: the page auto-fits a settled map at scale ~0.10 to
  // ~0.15, where a world-space head sized to look right at 1:1 is a third of a pixel and simply is not
  // there. The tip is backed off the target's own radius so it points AT the node instead of being
  // buried under it, and an edge whose VISIBLE shaft is shorter than MIN_ARROW_SHAFT_PX gets no head
  // at all: on a dense cluster those are all head and no line, which reads as noise and costs a fill
  // per edge to draw.
  //
  // Both passes are ONE path each rather than a beginPath/stroke per edge. Same pixels, and it is what
  // makes a second pass over up to 13819 edges affordable at all.
  // A SPLIT-ARM SHAFT IS DASHED. `a` is the per-edge bit writeHtml reads straight off Graph::outProv
  // (3 = one arm of a k-way split the resolver could not choose between — the same fact the XML map
  // spells prov="split"), so the picture and the data say the same thing about the same edge. It is
  // drawn as a different KIND of line and not as a shade, because a faded solid line is
  // indistinguishable from a distant one and a dashed line is not. Two passes because a dash pattern
  // is canvas STATE and cannot vary inside one path; the alternative is a beginPath/stroke per edge,
  // which is what this loop was before and what makes 13819 edges unaffordable. The pass order is
  // fixed, so the picture is stable.
  var ARROW_LEN_PX = 7.0, ARROW_HALF_PX = 3.2, MIN_ARROW_SHAFT_PX = 13.0;
  var DASH_ON_PX = 4.0, DASH_OFF_PX = 3.5;

  function edgeVisible(k, hl) {
    return !hl || hl.has(links[k].s) || hl.has(links[k].t);
  }

  function strokeEdgePass(hl, dashed) {
    ctx.setLineDash(dashed ? [DASH_ON_PX/scale, DASH_OFF_PX/scale] : []);
    ctx.beginPath();
    for (var k = 0; k < L; k++) {
      if (!edgeVisible(k, hl)) continue;
      if ((links[k].a === 1) !== dashed) continue;
      ctx.moveTo(nodes[links[k].s].x, nodes[links[k].s].y);
      ctx.lineTo(nodes[links[k].t].x, nodes[links[k].t].y);
    }
    ctx.stroke();
  }

  function arrowHeadPath(a, b, alen, ahalf) {
    var dx = b.x - a.x, dy = b.y - a.y;
    var d = Math.sqrt(dx*dx + dy*dy);
    if (!(d > 0)) return;
    var rs = nodeRadiusPx(a)/scale, rt = nodeRadiusPx(b)/scale;
    if ((d - rs - rt)*scale < MIN_ARROW_SHAFT_PX) return;
    var ux = dx/d, uy = dy/d;
    var tipX = b.x - ux*rt, tipY = b.y - uy*rt;
    var baseX = tipX - ux*alen, baseY = tipY - uy*alen;
    ctx.moveTo(tipX, tipY);
    ctx.lineTo(baseX - uy*ahalf, baseY + ux*ahalf);
    ctx.lineTo(baseX + uy*ahalf, baseY - ux*ahalf);
    ctx.closePath();
  }

  function drawEdges(hl) {
    var alen = ARROW_LEN_PX/scale, ahalf = ARROW_HALF_PX/scale;
    ctx.strokeStyle = '#8a8f98';
    ctx.lineWidth = 0.8/scale;
    ctx.globalAlpha = 0.26;
    strokeEdgePass(hl, false);
    strokeEdgePass(hl, true);
    ctx.setLineDash([]);
    ctx.fillStyle = '#c3c9d2';
    ctx.globalAlpha = 0.62;
    ctx.beginPath();
    for (var k = 0; k < L; k++) {
      if (edgeVisible(k, hl)) arrowHeadPath(nodes[links[k].s], nodes[links[k].t], alen, ahalf);
    }
    ctx.fill();
    ctx.globalAlpha = 1.0;
  }

  // ---- nodes. Node size is a SCREEN quantity converted to world units here, exactly the way the labels
  // already are — a hub has to look like a hub at every zoom, and it must never grow into a blob that
  // swallows the labels around it. Two earlier shapes both failed on this cut and are worth naming: a
  // per-node max against a 3.5-px-over-scale floor FLATTENED everything (at the 0.125 scale a 239-node
  // view fits at, the floor term exceeded every radius, so all 239 nodes drew at the same 3.5 px and the
  // degree signal was invisible in the one picture that needed it); a uniform per-frame multiplier fixed
  // the small end and blew up the large one, turning degree-111 hubs into 36 px discs that occluded four
  // labels. A screen-space band does both jobs and is zoom-invariant.
  // In `tested` mode an UNTESTED node is drawn hollow with a dashed ring instead of a filled disc — the
  // second, non-hue channel H5 is about. Every other mode fills as before.
  function isDimmed(i, hl) {
    return (hl && !hl.has(i)) || (searchSet && !searchSet.has(i));
  }

  function strokeHollowNode(n, r) {
    ctx.fillStyle = CANVAS_BG; ctx.fill();
    ctx.setLineDash([3/scale, 2.5/scale]);
    ctx.strokeStyle = colorForNode(n); ctx.lineWidth = Math.max(1.4/scale, r*0.3); ctx.stroke();
    ctx.setLineDash([]);
  }

  function drawNode(i, hl, hollow) {
    var n = nodes[i];
    var r = nodeRadiusPx(n)/scale;   // screen px -> world units, inside the transformed draw
    ctx.globalAlpha = isDimmed(i, hl) ? 0.18 : 1.0;
    ctx.beginPath();
    shapeFor(n).path(ctx, n.x, n.y, r);
    if (hollow && testedStroke(n)) {
      strokeHollowNode(n, r);
    } else {
      ctx.fillStyle = colorForNode(n);
      ctx.fill();
    }
    if (i === selected || i === hovered || n.gid === centreGid) {
      ctx.strokeStyle = '#fff'; ctx.lineWidth = 2/scale; ctx.stroke();
    }
  }

  function drawNodes(hl) {
    var hollow = (mode === 'tested');
    for (var i = 0; i < N; i++) drawNode(i, hl, hollow);
  }

  // ---- labels, decluttered by a greedy occupancy grid, drawn in SCREEN space.
  //
  // They used to be drawn in local-index order with no collision test at all, so two labels whose boxes
  // overlapped simply printed on top of each other: `release`/`resize`/`size` came out as "reisleaze",
  // `empty`/`end` as "eand". Text that is unreadable is worse than absent — it still costs the pixels and
  // now also costs the reader's trust in the ones that ARE legible.
  //
  // The grid is a Set of "col:row" cells over SCREEN space (constant cell size, so the declutter behaves
  // the same at every zoom). Placement is in descending importance — labelDegreeOrder first, then the
  // always-shown hovered/selected/centre — so when two labels collide the more important one is the one
  // that survives. A skipped label is not lost: hover, click or search brings any node's name back.
  //
  // THE TEXT IS DRAWN OUTSIDE THE WORLD TRANSFORM, and this is not a tidiness preference. It used to be
  // drawn inside it at `(11/scale) px` with a `LABEL_HALO_PX/scale` halo — a screen-constant size
  // expressed as a world quantity, which renders identically and costs whatever the zoom says. At the
  // scale a settled 1000-node map fits at, fitView pins `scale` on its 0.05 floor, so those became 220 px
  // glyphs stroked with a 60 px round-join pen, twenty-four of them per frame: the tab stopped responding
  // and never came back. Bisected to this block (a build with the sim on and the labels skipped renders
  // the same map instantly). Every coordinate below is the screen coordinate the occupancy grid was
  // ALREADY computing, so the picture is unchanged pixel for pixel — the difference is that the cost no
  // longer scales with 1/zoom. Node radii were converted to screen units for the same reason one commit
  // earlier; the labels were the half that was left behind, and only a whole-map view was large enough
  // to show it.
  //
  // CELL_H is the grid's row pitch; LABEL_H is how tall a drawn label actually is. They are different
  // numbers and conflating them was a real bug: reserving only the row the BASELINE lands in leaves two
  // labels 5 px apart claiming different rows either side of a 16 px boundary. Measured in a browser on
  // the README cut, that still left 2 overprinting pairs after the grid went in — one of them over the
  // centre symbol's own name. The box now spans every row the text touches, and its width is MEASURED
  // rather than estimated from the character count (an estimate that runs short reserves less than it
  // draws, which is the same collision by a second route).
  var CELL_W = 8, CELL_H = 16;   // LABEL_H is hoisted — fitView reserves with the same number

  function reserveLabelCells(cells, c0, c1, r0, r1) {
    for (var rr = r0; rr <= r1; rr++) { for (var c = c0; c <= c1; c++) { if (cells.has(c + ':' + rr)) return false; } }
    for (var rr2 = r0; rr2 <= r1; rr2++) { for (var c2 = c0; c2 <= c1; c2++) { cells.add(c2 + ':' + rr2); } }
    return true;
  }

  // placeTextAt is the whole placement rule — cull, reserve, halo, draw — with no idea what the text
  // is FOR. It was placeLabel(i) and nothing else until the hulls needed names too; a second copy for
  // hull labels would have been a second occupancy grid's worth of behaviour that could disagree with
  // this one about what overlaps what, which is precisely the overprinting this grid exists to stop.
  function placeTextAt(cells, text, sx, sy, fill, alpha) {
    var wpx = ctx.measureText(text).width;                // screen px directly — the font is screen px now
    // Text anchored well off the canvas is not drawn — and this is a GUARD, not an optimisation.
    // The cell walk below advances an integer counter held in a double, and above 2^53 `c++` stops
    // advancing, so ONE label at an extreme coordinate turns `for (c = c0; c <= c1; c++)` into a loop
    // that cannot terminate: no error, no console message, a permanently frozen tab. MAX_STEP stops
    // the divergence that produced such coordinates; this makes the loop's termination unconditional
    // rather than contingent on the sim staying well behaved. The `!(...)` form rejects NaN too.
    if (!(sx > -LABEL_CULL_PX && sx < W + LABEL_CULL_PX && sy > -LABEL_CULL_PX && sy < H + LABEL_CULL_PX)) { return false; }
    var c0 = Math.floor(sx/CELL_W), c1 = Math.floor((sx + wpx)/CELL_W);
    var r0 = Math.floor((sy - LABEL_H)/CELL_H), r1 = Math.floor((sy + 2)/CELL_H);
    if (c1 - c0 > 200) c1 = c0 + 200;                     // a pathological label cannot monopolise the grid
    if (!reserveLabelCells(cells, c0, c1, r0, r1)) return false;
    ctx.globalAlpha = alpha;
    // A HALO first. The occupancy grid stops labels colliding with each OTHER; it says nothing about
    // what is underneath them, and in a dense view a name lands on top of a node disc — #d6d9de on a
    // bright teal is barely readable, which is the same lost label by a different route. Stroking the
    // glyphs in the canvas background before filling them makes every label legible over anything.
    ctx.lineJoin = 'round';
    ctx.lineWidth = LABEL_HALO_PX;
    ctx.strokeStyle = 'rgba(10,10,10,0.9)';
    ctx.strokeText(text, sx, sy);
    ctx.fillStyle = fill;
    ctx.fillText(text, sx, sy);
    return true;
  }

  function placeNodeLabel(cells, i, hl) {
    var n = nodes[i], rpx = nodeRadiusPx(n);              // rpx is already a SCREEN radius
    return placeTextAt(cells, n.label, n.x*scale + ox + rpx + LABEL_GAP_PX, n.y*scale + oy + LABEL_BASE_PX,
                       '#e6e9ee', (hl && !hl.has(i)) ? 0.25 : 0.95);
  }

  // Hull names go in FIRST, through the same grid: a region's name outranks any single symbol's,
  // because it is the only thing on the canvas that says what a whole cluster IS. Drawn in the hull's
  // own colour so the name and the outline are visibly the same object.
  function placeHullNames(cells, hullAnchors) {
    ctx.font = '600 12px sans-serif';
    for (var ha = 0; ha < hullAnchors.length; ha++) {
      placeTextAt(cells, hullAnchors[ha].text, hullAnchors[ha].sx, hullAnchors[ha].sy, hullAnchors[ha].color, 0.95);
    }
    ctx.font = '11px sans-serif';
  }

  function forcedLabelNodes() {
    var forced = [];
    if (hovered  >= 0) forced.push(hovered);
    if (selected >= 0) forced.push(selected);
    for (var i = 0; i < N; i++) { if (nodes[i].gid === centreGid) { forced.push(i); break; } }
    return forced;
  }

  function placeNodeLabels(cells, hl) {
    var forced = forcedLabelNodes();
    for (var fi = 0; fi < forced.length; fi++) placeNodeLabel(cells, forced[fi], hl);   // never decluttered away
    if (searchSet) {
      // a live query REPLACES the standing set — matches are what you are looking for
      var shown = 0;
      for (var i = 0; i < N && shown < 80; i++) { if (searchSet.has(i) && placeNodeLabel(cells, i, hl)) shown++; }
    } else {
      for (var li = 0; li < labelDegreeOrder.length; li++) placeNodeLabel(cells, labelDegreeOrder[li], hl);
    }
  }

  function drawLabels(hl, hullAnchors) {
    var cells = new Set();
    ctx.font = '11px sans-serif';
    placeHullNames(cells, hullAnchors);
    placeNodeLabels(cells, hl);
    ctx.globalAlpha = 1.0;
  }

  // tooltip — now names the FILE. 13 label names covered 28 nodes on the README cut (three different
  // `empty`, three `find`), and the page emitted a FILES array it never read, so the one question a
  // duplicate name raises was the one question the UI could not answer.
  function drawTooltip() {
    if (hovered < 0) return;
    var n = nodes[hovered];
    var sx = n.x*scale+ox, sy = n.y*scale+oy;
    var msg = n.label + ' [' + n.type + ']  ' + fileOf(n);
    ctx.fillStyle = 'rgba(0,0,0,0.82)';
    ctx.font = '12px ui-monospace,SFMono-Regular,Menlo,monospace';
    var tw = ctx.measureText(msg).width;
    ctx.fillRect(sx+8, sy-18, tw+10, 22);
    ctx.fillStyle = '#fff';
    ctx.fillText(msg, sx+13, sy-2);
  }

  // ---- WHAT THIS FRAME ACTUALLY CONTAINS, and re-caption if it moved.
  //
  // The caption's node and edge counts are the LOADED subset. The camera is free to sit anywhere
  // inside it, and a picture exported after a zoom therefore stamped "120 nodes / 183 edges" across a
  // frame holding fifteen — a bitmap overstating its own contents, in the one artifact the caption
  // exists to travel in. (Found cutting the README's crop figures.) The count is of NODE CENTRES
  // inside the canvas rect: a mark half off the edge is in frame, and counting it is the reading that
  // errs toward the larger number rather than toward flattering the crop.
  //
  // The re-render is guarded on a CHANGE because the numbers renderProv states — this count and the
  // three hull counts — are all computed here, one frame after the caption that reports them. On a
  // settled page that is invisible; on an export taken straight after a zoom it is the previous view's
  // numbers stamped onto the new one. Rebuilding the caption unconditionally would put an innerHTML
  // write inside the settle loop at sixty frames a second, so it happens only when a stated number
  // actually moved.
  function updateFrameCaption() {
    nodesInFrame = 0;
    for (var fi = 0; fi < N; fi++) {
      var fsx = nodes[fi].x*scale + ox, fsy = nodes[fi].y*scale + oy;
      if (fsx >= 0 && fsx <= W && fsy >= 0 && fsy <= H) { nodesInFrame++; }
    }
    var provNow = nodesInFrame + ':' + hullsDrawn + ':' + hullsThin + ':' + hullsImpure;
    if (provNow !== provStamp) { provStamp = provNow; renderProv(); }
  }

  function draw() {
    paintBackdrop();
    ctx.save();
    ctx.translate(ox, oy); ctx.scale(scale, scale);
    var hl = highlightSet();
    var hullAnchors = drawHulls();
    drawEdges(hl);
    drawNodes(hl);
    ctx.globalAlpha = 1.0;
    ctx.restore();
    drawLabels(hl, hullAnchors);
    drawTooltip();
    updateFrameCaption();
  }

  // FILES[n.file] — the path, or an honest blank when the payload has no entry for it. FILES was emitted
  // and never read anywhere in this script (2241 bytes of dead payload on this repository's own map).
  function fileOf(n) { return (FILES && n.file < FILES.length) ? FILES[n.file] : ''; }

  // canvas → world coords
  function toWorldXY(px, py) { return { x: (px-ox)/scale, y: (py-oy)/scale }; }
  function hitTest(px, py) {
    var w = toWorldXY(px, py);
    for (var i = N-1; i >= 0; i--) {
      // The DRAWN size, so a click matches what the eye aimed at — and since a mark is no longer always
      // a circle, that size is the shape's own REACH (its farthest point from centre, the one number the
      // SHAPES table has to carry for this) and not the nominal radius. A flat 1.2x was right while
      // everything was a disc and under-covered a triangle (1.55) and a bar (1.57) the moment it wasn't.
      var n = nodes[i], r = nodeRadiusPx(n)*shapeFor(n).reach*1.15/scale;
      var dx = w.x-n.x, dy = w.y-n.y;
      if (dx*dx+dy*dy <= r*r) return i;
    }
    return -1;
  }

  // -- end of section 3b --

  // ---- resize. Two defects lived here.
  //
  // H10 (sharpness): the backing store was set to the CSS pixel count, so on any retina display the whole
  // canvas was rendered at 1x and scaled up by the compositor — every node edge and every label soft. The
  // backing store is now devicePixelRatio times the CSS size, with the CSS size pinned in style so the page
  // still lays out in CSS pixels; paintBackdrop() re-establishes the DPR transform each frame, so all
  // drawing code keeps working in CSS-pixel coordinates and nothing else had to change.
  //
  // H8 (a stranded camera): step() early-returns once SIM_STEPS >= MAX_SIM, so after settling fitView could
  // never run again — and resize() set canvas.width, which CLEARS the canvas, then redrew with the old
  // camera. Measured: a viewport change dropped the graph from 238788 to 27174 lit pixels, and in one case
  // left a fully blank canvas while the info bar read "221 nodes in view", recoverable only by reload.
  // Re-fitting whenever autoFit is still on restores the invariant the settling loop used to maintain.
  function chromeTop() {
    var p = document.getElementById('prov');
    return 36 + ((p && p.offsetHeight) ? p.offsetHeight : 0);
  }
  function resize() {
    var top = chromeTop();
    var cssW = Math.max(1, window.innerWidth), cssH = Math.max(1, window.innerHeight - top);
    DPR = Math.max(1, Math.min(4, window.devicePixelRatio || 1));
    canvas.style.marginTop = top + 'px';
    canvas.style.width  = cssW + 'px';
    canvas.style.height = cssH + 'px';
    canvas.width  = Math.round(cssW*DPR);
    canvas.height = Math.round(cssH*DPR);
    W = cssW; H = cssH;                      // the drawing code's coordinate space stays CSS pixels
    var el;
    if ((el = document.getElementById('crumb'))) el.style.top = top + 'px';
    if ((el = document.getElementById('hits')))  el.style.top = top + 'px';
    stackCardsBelowHits();   // cards clear the results panel when it is open (see its comment)
    if (autoFit) fitView();
    draw();
  }
  window.addEventListener('resize', resize);

  // mouse events. A click that HITS a node re-centres the ego graph on it (Sourcetrail-style) UNLESS
  // it is already the sole centre of a #node view (double-click-to-reselect toggles the 1-hop
  // highlight instead, same as the old single-view behaviour, so hovering/inspecting still works).
  canvas.addEventListener('mousedown', function(e) {
    autoFit = false;   // user takes control of the camera
    var hit = hitTest(e.clientX, e.clientY);
    if (hit >= 0) { dragging = hit; }
    else { panStart = { x: e.clientX - ox, y: e.clientY - oy }; }
  });
  canvas.addEventListener('mousemove', function(e) {
    if (dragging >= 0) {
      var w = toWorldXY(e.clientX, e.clientY);
      nodes[dragging].x = w.x; nodes[dragging].y = w.y;
      nodes[dragging].vx = 0; nodes[dragging].vy = 0;
      draw(); return;
    }
    if (panStart) { ox = e.clientX - panStart.x; oy = e.clientY - panStart.y; draw(); return; }
    var h = hitTest(e.clientX, e.clientY);
    if (h !== hovered) { hovered = h; draw(); }
  });
  var mouseMoved = false;
  canvas.addEventListener('mousemove', function() { mouseMoved = (dragging >= 0); });
  canvas.addEventListener('mouseup', function(e) {
    var wasDragging = dragging >= 0 && mouseMoved;
    dragging = -1; panStart = null; mouseMoved = false;
    if (wasDragging) return;   // a drag-release is not a click-to-recenter
    var hit = hitTest(e.clientX, e.clientY);
    if (hit < 0) { selected = -1; info.textContent = ''; draw(); return; }
    var n = nodes[hit];
    if (n.gid === centreGid) {
      // clicking the current ego-graph centre toggles 1-hop highlight instead of a no-op recenter
      selected = (selected === hit) ? -1 : hit;
      info.textContent = selected >= 0 ? (n.label + ' · ' + n.type + ' · rank=' + n.rank) : '';
      draw();
    } else {
      location.hash = '#node/' + n.gid;
    }
  });
  // H12: the zoom is CLAMPED. `scale *= factor` compounded without bound, so a few wheel flicks reached
  // 0 or Infinity and there was no gesture that came back — the graph was gone and the only fix was a
  // reload. The factor is recomputed from the clamped scale so the cursor stays the anchor at the rails.
  canvas.addEventListener('wheel', function(e) {
    e.preventDefault();
    autoFit = false;   // user takes control of the camera
    var want = scale * (e.deltaY < 0 ? 1.1 : 0.91);
    var next = Math.max(SCALE_MIN, Math.min(SCALE_MAX, want));
    var factor = next / scale;
    if (factor === 1) return;
    var cx = e.clientX, cy = e.clientY - chromeTop();
    ox = cx - (cx - ox)*factor; oy = cy - (cy - oy)*factor;
    scale = next;
    draw();
  }, { passive: false });

  // ---- search. On the canvas views it filters the CURRENT node set. On the OVERVIEW it used to do
  // nothing at all: renderOverview never calls loadSubset, so N === 0 and this loop ran zero times — the
  // box you land on matched nothing, always, and said nothing about it. That is the same silent-failure
  // shape as H1, in a control the user's eye lands on first. It now searches the WHOLE symbol set (the
  // only set the landing page has) and renders the hits as links into the node view.
  search.addEventListener('input', function() {
    var q = search.value.trim().toLowerCase();
    if (currentView === 'overview') { overviewSearch(q); return; }
    if (!q) { searchSet = null; draw(); return; }
    searchSet = new Set();
    for (var i = 0; i < N; i++)
      if (nodes[i].label.toLowerCase().indexOf(q) >= 0) searchSet.add(i);
    draw();
  });

  // #hits and #cards are both position:fixed at the chrome's bottom edge, so the results panel would sit
  // ON TOP of the first row of module cards. Measured in a browser: hits occupied 76-105 px and cards
  // started at 76. The cards start below whatever the panel currently occupies.
  function stackCardsBelowHits() {
    var h = document.getElementById('hits');
    var visible = h && h.style.display === 'block';
    cardsEl.style.top = (chromeTop() + (visible ? h.offsetHeight : 0)) + 'px';
  }

  function overviewSearch(q) {
    var el = document.getElementById('hits');
    if (!q) { el.style.display = 'none'; el.innerHTML = ''; renderOverviewCards(null); stackCardsBelowHits(); return; }
    var hits = [];
    for (var i = 0; i < NODES.length; i++) {
      if (NODES[i].label.toLowerCase().indexOf(q) >= 0) hits.push(i);
    }
    var html = '<span class="n">' + hits.length + ' symbol' + (hits.length === 1 ? '' : 's') + ' match "' + escHtml(q) + '"' +
               (hits.length > 40 ? ' (first 40)' : '') + '</span>';
    for (var h = 0; h < hits.length && h < 40; h++) {
      var nd = NODES[hits[h]];
      html += '<a href="#node/' + hits[h] + '/2">' + escHtml(nd.label) + '</a>';
    }
    if (!hits.length) html += '<span class="n">nothing in this map — the page holds the top ' + NODE_TOTAL + ' of ' + SYM_TOTAL + ' symbols</span>';
    el.innerHTML = html;
    el.style.display = 'block';
    renderOverviewCards(q);
    stackCardsBelowHits();
  }

  // depth slider (node-view ego graph radius, 1-3)
  var egoDepth = 2;
  depthSlider.addEventListener('input', function() {
    egoDepth = parseInt(depthSlider.value, 10) || 2;
    depthVal.textContent = String(egoDepth);
    if (currentView === 'node') renderNode(centreGid, false);
  });

  // -- end of section 4 --

  // ---- ROUTER: #graph (home) | #overview | #module/ID | #node/ID[/DEPTH] ----
  var currentView = null;
  var centreGid = -1;   // NODES index the current #node view is centred on (-1 outside node view)

  function setChrome(showCards, showCanvas) {
    cardsEl.className = showCards ? 'show' : '';
    canvas.style.display = showCanvas ? 'block' : 'none';
    depthSlider.parentElement.style.visibility = (currentView === 'node') ? 'visible' : 'hidden';
    if (!showCards) { document.getElementById('hits').style.display = 'none'; }
  }

  // ---- the caption, in TWO HALVES, because they answer different questions and travel to different
  // places.
  //
  // FACTS is what this picture IS: the root it was built from, the ranker whose scores set the sizes,
  // the top-k that bounded the selection and what fraction of the repository that is, the counts in the
  // current view, the colour metric, and any state that makes the picture provisional. This tool states
  // every truncation it makes in its XML header and the page used to state nothing at all about itself —
  // a screenshot could not be audited, which is exactly the disclosure the rest of the tool is for.
  //
  // METHOD is how to READ it: which way an arrow points, what a dashed shaft means and at what
  // threshold, which nodes got labels, what each mark shape is, and how many module outlines were drawn
  // of how many — with each truncation's own count and its own reason.
  //
  // WHY THEY SPLIT. Both halves used to be one block and stampProvenance burned all of it into the
  // exported bitmap, which had reached three dense lines. A figure in a README gets three to five
  // seconds, and the stamp font is FITTED to the bitmap width against an 8 px readability floor
  // (stampProvenance), so every methodology clause added to it made the provenance it exists to carry
  // physically smaller. Method is what a caption UNDERNEATH a figure says once, in prose; provenance is
  // what has to survive the picture being lifted out of the page it came from. So the bitmap carries
  // FACTS, and the export writes METHOD beside it as a .txt (see sidecarText) — where a README author
  // can lift the wording verbatim rather than paraphrase it.
  //
  // Non-negotiable #3 is why the last FACTS line exists at all: it names the companion file AND every
  // channel whose rule moved into it. A trimmed bitmap that said nothing about the trim would be a
  // picture drawing arrowheads, dashes, shapes and outlines with no way to read any of them — the quiet
  // omission this whole block was added to prevent, re-introduced by the fix for it.
  function provKey(t) { return '<span class="k">' + t + '</span> '; }

  function provMetric() {
    return { lang: 'language', community: 'module (community)', cx: 'cyclomatic complexity',
             churn: 'commits (' + (CHURN_WINDOW || 'window not recorded') + ')', tested: 'has a test' }[mode] || mode;
  }

  function provViewName() {
    if (currentView === 'overview') return MODULES.length + ' modules';
    if (currentView === 'graph') return 'whole map';
    if (currentView === 'module') return 'module subgraph';
    return 'depth-' + egoDepth + ' neighbourhood';
  }

  // R-R/PRIV: the caption shows the TAIL of the root, not the whole path -- and by the time it runs,
  // the home pair is already gone, stripped in C++ by stripHomePair() before `const ROOT` was written
  // (see the emit site for why the file, not just the pixels, is the boundary). This function is the
  // PRESENTATION half: it shortens a long root to '…/a/b' so the caption fits. It re-applies the same
  // structural strip anyway, because it is cheap, it is a no-op on stripped input, and it keeps this
  // half correct on its own terms rather than correct-because-of-something-upstream.
  //
  // The caption matters because stampProvenance burns it into every exported PNG, and a PNG is the
  // thing people share. Shipping the absolute path there published the operator's filesystem layout,
  // and their home directory often carries their real name. Verified on this repo's own README
  // figures, which went to a public branch stamped with the operator's own absolute home path;
  // `strings` finds nothing, because it is rendered as pixels, so no secret scanner would ever have
  // flagged it. Taking the last two segments alone is not enough:
  // for `~/myproject` -- probably the most common layout there is -- the home directory IS one of
  // those two, so the caption published the username anyway. Measured on the first version (written
  // here in the Linux spelling; the macOS one differs only in the leading segment, which is why the
  // check below is on `users` OR `home` and is case-folded first):
  //     /home/jane.doe/src/myproject  -> …/src/myproject   clean
  //     /home/jane.doe/myproject      -> …/jane.doe/…      LEAKED
  //     /home/jane.doe                -> whole path        LEAKED  (the <=2 guard passed it through)
  //     C:\Users\Bob.Jones\code       -> whole path        LEAKED  (split was on '/' only)
  // The leaking segment is always the one after that home root, and its position is knowable, so
  // remove it by structure rather than hoping the tail misses it.
  function provRootShort(r) {
    if (!r) { return ROOT_NAME || '.'; }
    var parts = r.replace(/[\/\\]+$/, '').split(/[\/\\]+/).filter(function(x){ return x.length && x !== '.'; });
    if (parts.length && /^[A-Za-z]:$/.test(parts[0])) { parts.shift(); }
    var lead = (parts[0] || '').toLowerCase();
    if ((lead === 'users' || lead === 'home') && parts.length >= 2) { parts.splice(0, 2); }
    else if (lead === 'root') { parts.splice(0, 1); }
    if (!parts.length) { return ROOT_NAME || '.'; }   // the name, never '~': a tilde reads as the home directory
    return (parts.length > 2 ? '…/' : '') + parts.slice(-2).join('/');
  }

  function provIdentityLine() {
    var pct = SYM_TOTAL ? Math.round(1000*NODE_TOTAL/SYM_TOTAL)/10 : 0;
    return provKey('root') + '<b>' + escHtml(provRootShort(ROOT)) + '</b>  ' +
           provKey('ranker') + '<b>' + escHtml(RANKER) + '</b>  ' +
           provKey('top-k') + '<b>' + TOPK + '</b> of ' + SYM_TOTAL + ' symbols (' + pct + '%)  ' +
           provKey('map') + '<b>' + NODE_TOTAL + '</b> nodes / <b>' + EDGE_TOTAL + '</b> call edges' +
           (AT ? '  ' + provKey('commit') + '<b>' + escHtml(AT) + '</b>' : '') +
           (AT.indexOf('+shallow') >= 0 ? ' <b>(shallow clone: churn counts only the commits present)</b>' : '') +
           '  ' + provKey('ripwire') + '<b>' + escHtml(VERSION) + '</b>';
  }

  function provSettleClause() {
    if (settleTimedOut) return '  <b>settling…</b> (layout over the ' + SETTLE_BUDGET_MS + ' ms budget, still converging)';
    if (layoutStopped) return '  <b>layout stopped at step ' + SIM_STEPS + ' of ' + MAX_SIM + '</b> (the ' + LAYOUT_BUDGET_MS +
                              ' ms layout budget is spent — positions are under-converged, drag to adjust)';
    return '';
  }

  // ...and how much of that the CAMERA is on. Absent when the camera frames the whole view, which is
  // the auto-fit default and the case where the counts above already describe the frame; present the
  // moment a zoom or a pan makes them describe more than the picture does (see draw()'s closing block).
  function provViewLine(isGraph) {
    // Self-calls are counted in EDGE_TOTAL and cannot be drawn (loadSubset says why). This clause is
    // method-shaped but it stays with the FACTS, because it RECONCILES two counts that are both on the
    // bitmap: in the whole-map view EDGE_TOTAL and L sit on the same screen over the same node set, and
    // an unexplained difference between them is precisely the silent inconsistency this block prevents.
    var drops = ( isGraph && selfEdgesDropped > 0 )
              ? ' (' + selfEdgesDropped + ' self-call' + (selfEdgesDropped === 1 ? '' : 's') + ' not drawn)' : '';
    var framing = ( isGraph && nodesInFrame < N ) ? ', camera framing <b>' + nodesInFrame + '</b>' : '';
    return provKey('view') + '<b>' + provViewName() + '</b>' + (currentView === 'overview' ? '' : ': ' + N + ' nodes / ' + L + ' edges' + drops + framing) + '  ' +
           provKey('colour') + '<b>' + escHtml(provMetric()) + '</b>' + (mode === 'churn' && !CHURN_OK ? ' <b>unavailable (no git history)</b>' : '') +
           provSettleClause();
  }

  // ---- CAPTION FACTS. stampProvenance burns THIS half, and only this half, into the exported PNG.
  function provFactLines(isGraph) {
    var lines = [provIdentityLine(), provViewLine(isGraph)];
    if (isGraph) {
      lines.push( provKey('method') + 'arrow, dash, label, shape and module-outline rules → <b>' + escHtml(exportBase()) +
                  '.txt</b> (saved with this image)' );
    }
    return lines;
  }

  function provHullClause() {
    // EVERY HULL TRUNCATION, EACH WITH ITS OWN COUNT AND ITS OWN REASON. There are three and they are
    // different failures: the cap (26 overlapping regions is the hairball again in a second channel),
    // the SHAPE test (a near-collinear group draws a streak, not a region — see HULL_COMPACTNESS), and
    // the PURITY test (an outline enclosing mostly other modules says nothing). Pooling them into one
    // number would tell a reader that outlines are missing without telling them why, and a reader
    // counting outlines against the module count is exactly who this line is for.
    var hullWhy = [];
    if (hullsThin > 0)   { hullWhy.push(hullsThin + ' dropped as too thin to read as a region'); }
    if (hullsImpure > 0) { hullWhy.push(hullsImpure + ' dropped as enclosing mostly other modules'); }
    return 'module outlines <b>' + hullsDrawn + '</b> of ' + hullsTotal + ' modules with ' + MIN_HULL_MEMBERS +
           '+ nodes in view (cap ' + MAX_HULLS + (hullWhy.length ? '; ' + hullWhy.join('; ') : '') + ')';
  }

  // ---- CAPTION METHOD. Rendered on the page and written to the companion .txt; NOT stamped.
  function provMethodClauses(isGraph) {
    var clauses = [];
    if (!isGraph) return clauses;
    // "caller → callee" is stated for the same reason every other clause here is: the picture draws a
    // direction, and a reader must not have to guess which end of an arrow is the one doing the calling.
    clauses.push( 'arrow points caller → callee' );
    // The dashed shafts need a key, and it has to name WHAT the resolver could not do: "some of these
    // edges are uncertain" is not a disclosure, it is a mood. Stated as a count so a reader can weigh
    // it — and stated at zero too, because a zero here means the resolver pinned every drawn call to
    // exactly one definition, which is a result.
    //
    // THIS COUNT IS THE VIEW'S, and the legend's AMB_LINKS is the MAP'S. They are two scopes of the
    // same fact and they differ in every ego and module view, so each says which set it is counting;
    // one number reported twice under two scopes would be the silent inconsistency this block exists
    // to prevent.
    clauses.push( '<b>' + ambEdges + '</b> of ' + L + ' shafts dashed in this view = the resolver could not choose'
                  + ' between same-name definitions and split the call over all of them' );
    // rule 2 of loadSubset deliberately shows one label per name; a reader must not infer that from the picture
    clauses.push( 'labels top ' + MAX_LABELS + ' by in-view degree, one per name' );
    // The shape key is built from SYM_SHAPES, the identical lookup draw() marks a node with, so it
    // cannot name a shape the picture does not draw; and it lists only the kinds actually IN THIS
    // VIEW, because a fixed roster would print marks a reader can hunt for and never find.
    clauses.push( 'shapes ' + shapeKey() );
    if (hullsTotal > 0) clauses.push( provHullClause() );
    return clauses;
  }

  function renderProv() {
    var el = document.getElementById('prov');
    if (!el) return;
    var isGraph = ( currentView !== 'overview' && N > 0 );
    var methodClauses = provMethodClauses(isGraph);
    el.innerHTML = '<div id="provfacts">' + provFactLines(isGraph).join('<br>') + '</div>' +
                   ( methodClauses.length ? '<div id="provmethod">' + provKey('how to read') +
                     '<span class="mc">' + methodClauses.join('</span> · <span class="mc">') + '</span></div>' : '' );
  }

  // the shape key's text: one entry per shape present, naming every kind that shares it (class/struct/
  // interface collapse onto the square, var/field onto the plus — the collapses kSymShapes chose).
  function shapeKey() {
    var order = [], kinds = {};
    for (var i = 0; i < N; i++) {
      var t = nodes[i].type, sh = SYM_SHAPES[t] || 'circle';
      if (!Object.prototype.hasOwnProperty.call(kinds, sh)) { kinds[sh] = []; order.push(sh); }
      if (kinds[sh].indexOf(t) < 0) { kinds[sh].push(t); }
    }
    var out = [];
    for (var j = 0; j < order.length; j++) {
      out.push('<b>' + (SHAPES[order[j]] || SHAPES.circle).glyph + '</b> ' + escHtml(kinds[order[j]].join('/')));
    }
    return out.join('  ');
  }

  // the module cards, optionally filtered to modules whose name or any MEMBER matches `q` (the overview
  // half of the search fix — a card list that ignores the query would be the same inert control again)
  function moduleMatches(mod, q) {
    if (mod.name.toLowerCase().indexOf(q) >= 0) { return true; }
    for (var j = 0; j < mod.members.length; j++) {
      if (NODES[mod.members[j]].label.toLowerCase().indexOf(q) >= 0) { return true; }
    }
    return false;
  }

  function moduleCardHtml(mod, m) {
    var html = '<div class="card module-card" data-module-card="1" data-mid="' + m + '">';
    html += '<h2>' + escHtml(mod.name) + '</h2>';
    html += '<div class="meta">' + mod.fileCount + ' files · ' + mod.symCount + ' symbols · in ' + mod.inCross + ' / out ' + mod.outCross + '</div>';
    html += '<ul>';
    for (var i = 0; i < mod.top.length; i++) {
      var nd = NODES[mod.top[i]];
      html += '<li>' + escHtml(nd.label) + ' <span style="color:#666">[' + nd.type + ']</span></li>';
    }
    return html + '</ul></div>';
  }

  function renderOverviewCards(q) {
    var html = '', kept = 0;
    for (var m = 0; m < MODULES.length; m++) {
      if (q && !moduleMatches(MODULES[m], q)) { continue; }
      kept++;
      html += moduleCardHtml(MODULES[m], m);
    }
    if (MODULES.length === 0) html = '<div style="color:#999;padding:20px">No multi-symbol modules detected — the graph is too small or too sparse for community grouping.</div>';
    else if (q && kept === 0) html = '<div style="color:#999;padding:20px">No module contains a symbol matching "' + escHtml(q) + '".</div>';
    cardsEl.innerHTML = html;
    var els = cardsEl.querySelectorAll('[data-module-card]');
    for (var i = 0; i < els.length; i++) {
      els[i].addEventListener('click', function() { location.hash = '#module/' + this.getAttribute('data-mid'); });
    }
    info.textContent = (q ? kept + ' of ' + MODULES.length : MODULES.length) + ' modules';
  }

  function renderOverview() {
    currentView = 'overview'; centreGid = -1;
    crumbEl.style.display = 'none';
    setChrome(true, false);
    var q = search.value.trim().toLowerCase();
    if (q) { overviewSearch(q); } else { document.getElementById('hits').style.display = 'none'; renderOverviewCards(null); stackCardsBelowHits(); }
    renderProv();
  }

  // ---- #graph — the WHOLE selected node set, drawn. This is the page's home view.
  //
  // WHY IT HAD TO EXIST. --html's own --help line calls the artifact a "force-directed call graph", and
  // until this route the page had no view that drew one: #module/ID and #node/ID[/DEPTH] are both proper
  // SUBSETS, and the landing view was a grid of Leiden cards. Two consequences, and the second is the
  // one that mattered.
  //
  //   1. The cards are a weak first screen on their own terms. Measured on this repository's own map:
  //      65% of them are single-file, 35% carry `in 0 / out 0` (not one cross-module edge), the median
  //      card holds 2 symbols, and the largest are titled `size`, `find`, `empty`, `push_back` — a
  //      community named after its highest-ranked member is named after a container primitive.
  //   2. NO --html PICTURE WAS REPRODUCIBLE BY A SINGLE COMMAND. Every hero needed a fragment pasted in
  //      after the run, and the README's was `#node/431/2` — a POSITION in the rank-sorted array, so not
  //      a name and not stable across --top-k. Name-addressable routes fixed half of that; a run whose
  //      output is the picture fixes the other half. `ripwire DIR --rank-by=rrf --color-by=cx --html=F`
  //      IS the image now, with nothing to paste and nothing to remember.
  //
  // The module overview loses nothing but the boot slot: same route, same bar link, same search-into-
  // modules path. A view that answers "how is this repository grouped" is a good second screen and was a
  // poor first one, because it is an ANSWER about the map and the map itself was never on screen.
  //
  // COST, AND WHY IT DEGRADES RATHER THAN HANGS. This is the largest view the page can build — up to the
  // 5000-node ceiling writeHtml selects at — against a sim that is 300 O(n^2) steps, ~38 ms per step at
  // n=5000. Nothing new is needed for that and nothing new is added: loadSubset already settles under one
  // wall-clock budget and hands the remainder to the progressive rAF path, and renderProv says
  // "settling…" for exactly as long as that is true. A second budget here would be a second thing to
  // keep honest and a second thing to get wrong.
  function renderGraph() {
    currentView = 'graph'; centreGid = -1;
    crumbEl.style.display = 'none';
    setChrome(false, true);
    var ids = [];
    for (var i = 0; i < GN; i++) { ids.push(i); }
    loadSubset(ids, LINKS);   // renders the caption itself, once N/L are known
    info.textContent = GN + ' symbols · ' + MODULES.length + ' modules · click a node to open its neighbourhood';
  }

  function renderModule(mid) {
    var mod = MODULES[mid];
    currentView = 'module'; centreGid = -1;
    crumbEl.style.display = 'none';
    setChrome(false, true);
    if (!mod) { loadSubset([], []); return; }
    var ids = mod.members.slice().sort(function(a,b){ return a-b; });
    var idSet = new Set(ids);
    var edges = egoEdges(idSet);
    loadSubset(ids, edges);
    // entry points: members with an in-edge from OUTSIDE this module (high outside in-degree first)
    var entryCount = new Map();
    for (var i = 0; i < ids.length; i++) {
      var gid = ids[i], cnt = 0, froms = gin[gid] || [];
      for (var j = 0; j < froms.length; j++) if (!idSet.has(froms[j])) cnt++;
      if (cnt > 0) entryCount.set(gid, cnt);
    }
    var entries = Array.from(entryCount.keys()).sort(function(a,b){ return entryCount.get(b)-entryCount.get(a) || a-b; }).slice(0,5);
    var neighNames = mod.neigh.map(function(nid){ return MODULES[nid].name; });
    var msg = mod.name + ' · ' + ids.length + ' symbols · entry points: ' + (entries.map(function(g){return NODES[g].label;}).join(', ') || 'none');
    if (neighNames.length) msg += ' · neighbours: ' + neighNames.join(', ');
    info.textContent = msg;
  }

  function renderNode(gid, addToTrail) {
    currentView = 'node'; centreGid = gid;
    setChrome(false, true);
    var eg = egoGraph(gid, egoDepth);
    loadSubset(eg.ids, eg.edges);
    if (addToTrail !== false) pushTrail(gid);
    renderCrumb();
    var n = NODES[gid];
    // The caller/callee split is stated because the walk that built this view crosses BOTH directions
    // and the picture would otherwise leave a reader to count arrowheads to find out how much of a
    // neighbourhood is "who needs me" and how much is "what I need".
    info.textContent = n.label + ' · ' + n.type + ' · ' + fileOf(n) + ' · rank=' + n.rank + ' · depth=' + egoDepth +
                       ' · ' + eg.ids.length + ' nodes in view · ' + eg.callers + ' callers / ' + eg.callees + ' callees at depth 1';
    renderProv();
  }

  // ---- #node/WHAT[/DEPTH] — WHAT is a symbol NAME, or (still) a numeric NODES index.
  //
  // The index is a position in the rank-sorted NODES array, so `#node/431/2` is not a name, is not stable
  // across --top-k, and is not something a document can be written against: the README's own hero link is
  // exactly that, and it silently addresses a different symbol the moment the map is regenerated with a
  // different ceiling. Resolving by name first makes `#node/lexicalScoresTiered/2` mean what it says, and
  // keeps every existing numeric link working. A name that appears on several symbols resolves to the
  // highest-ranked one — deterministic (NODES is rank-sorted, so it is the first match) and disclosed in
  // the info line, which names the file the winner came from.
  function gidForRoute(what) {
    if (/^[0-9]+$/.test(what)) { var ix = parseInt(what, 10); return (ix >= 0 && ix < NODES.length) ? ix : -1; }
    var want = decodeURIComponent(what);
    for (var i = 0; i < NODES.length; i++) { if (NODES[i].label === want) return i; }
    var lower = want.toLowerCase();
    for (var j = 0; j < NODES.length; j++) { if (NODES[j].label.toLowerCase() === lower) return j; }
    return -1;
  }

  // ---- ROUTER. Boot sets the hash, and setting it fires hashchange, so route() used to run TWICE at
  // boot: harmless when the landing view was a card grid, and a doubled full-map settle now that it is
  // not. routedHash makes route() idempotent on the hash it last rendered. Nothing real is suppressed —
  // every navigation on this page changes the hash, and the browser fires no hashchange for a set to
  // the value already there (which is why click-the-current-node already went through its own branch).
  var routedHash = null;
  function route() {
    var h = location.hash.replace(/^#/, '');
    if (h === routedHash) { return; }
    routedHash = h;
    var parts = h.split('/');
    if (parts[0] === 'overview') {
      renderOverview();
    } else if (parts[0] === 'module' && parts[1] !== undefined) {
      renderModule(parseInt(parts[1], 10) || 0);
    } else if (parts[0] === 'node' && parts[1] !== undefined) {
      if (parts[2] !== undefined) { egoDepth = Math.max(1, Math.min(3, parseInt(parts[2],10)||2)); depthSlider.value = String(egoDepth); depthVal.textContent = String(egoDepth); }
      var gid = gidForRoute(parts[1]);
      if (gid < 0) {
        // an unresolvable route is SAID, not silently redirected to node 0 (which is what parseInt||0 did).
        // It lands on the home view — the whole map, which is the thing you can then search — rather than
        // on whichever view happened to be the landing page when this branch was written.
        renderGraph();
        info.textContent = 'no symbol named "' + decodeURIComponent(parts[1]) + '" in this map (top ' + NODE_TOTAL + ' of ' + SYM_TOTAL + ')';
        return;
      }
      var already = trail[trailPos] === gid;
      renderNode(gid, !already);
    } else {
      renderGraph();   // '#graph', an empty hash, and any hash this router does not recognise
    }
  }
  window.addEventListener('hashchange', route);

  // colour-mode selector: initial value from the baked COLOR_MODE; a change re-renders legend + canvas
  var modeSel = document.getElementById('colorMode');
  modeSel.value = COLOR_MODE;
  modeSel.addEventListener('change', function() { mode = modeSel.value; renderLegend(); renderProv(); draw(); });
  renderLegend();

  // ---- PNG export. With the canvas painting its own background and its backing store scaled by DPR, this
  // is now the correct way to get a picture of the graph out of the page: an OS screenshot is capped at the
  // display's own resolution and includes the browser chrome, whereas toDataURL hands back exactly the
  // backing store — DPR x the CSS size, background composited, nothing else in the frame.
  // The exported bitmap is COMPOSED, not the raw canvas. The provenance caption is DOM, so a plain
  // canvas.toDataURL() hands back a picture that states nothing about itself — the exact defect the
  // caption was added to fix, surviving into the one artifact the caption exists for. A README image, a
  // bug report, a slide: every one of them travels without the page around it. So the export draws the
  // graph into its own bitmap with the caption stamped underneath, in the page's own colours.
  // The stamp's height is DERIVED from how many caption lines there are, not pinned at the two there
  // used to be. A constant here silently truncated the caption the day it grew a third line — the shape
  // key — which is the clipping defect STAMP_FONT_MIN below already exists to stop, arriving by the
  // other axis.
  var STAMP_MAX_LINES = 4, STAMP_LINE_H = 17, STAMP_TOP = 19, STAMP_BOT = 8;
  var STAMP_PAD = 14, STAMP_FONT_MAX = 13, STAMP_FONT_MIN = 8;
  // #provfacts, not #prov: the bitmap carries what this picture IS and the companion .txt carries how to
  // read it (renderProv says why the two halves split). Reading the whole caption here is what put three
  // dense lines of methodology into every exported figure, shrinking the fitted stamp font against its
  // 8 px floor until the provenance the stamp exists for was the smallest thing in the image.
  function stampLines() {
    var el = document.getElementById('provfacts');
    return el ? el.innerText.split('\n').slice(0, STAMP_MAX_LINES) : [];
  }
  function stampHeight() { var n = stampLines().length; return n ? STAMP_TOP + STAMP_LINE_H*(n - 1) + STAMP_BOT : 0; }
  function stampFont(px) { return px + 'px ui-monospace,SFMono-Regular,Menlo,monospace'; }
  // The caption is one long unwrapped line of monospace, and the exported bitmap is as wide as whatever
  // viewport produced it. At a fixed 13 px the first real export CLIPPED it at the frame edge: the root
  // path stopped mid-word and the colour metric — the thing the picture is coloured BY — was gone
  // entirely, cut off past the right margin with nothing to say so. That is the export existing to carry
  // provenance and silently dropping half of it, which is worse than not stamping at all, because the
  // remaining half looks complete. The font is fitted to the widest line instead (never larger than 13,
  // floored at 8 where monospace stops being readable), and a line that STILL does not fit is elided
  // with a marker so the loss is visible in the image rather than at its edge.
  function stampProvenance(g, w, y) {
    var lines = stampLines();
    g.fillStyle = '#0b0b0b';  g.fillRect(0, y, w, stampHeight());
    g.fillStyle = '#2a2a2a';  g.fillRect(0, y, w, 1);
    var avail = w - 2*STAMP_PAD, widest = 0, i;
    g.font = stampFont(STAMP_FONT_MAX);
    for (i = 0; i < lines.length; i++) { widest = Math.max(widest, g.measureText(lines[i]).width); }
    var px = STAMP_FONT_MAX;
    if (widest > avail && widest > 0) { px = Math.max(STAMP_FONT_MIN, Math.floor(STAMP_FONT_MAX*avail/widest)); }
    g.font = stampFont(px);
    g.fillStyle = '#9aa1aa';
    for (i = 0; i < lines.length; i++) {
      var t = lines[i];
      if (g.measureText(t).width > avail) {
        while (t.length > 1 && g.measureText(t + '…').width > avail) { t = t.slice(0, -1); }
        t = t + '…';
      }
      g.fillText(t, STAMP_PAD, y + STAMP_TOP + i*STAMP_LINE_H);
    }
  }
  function exportBitmap() {
    var out = document.createElement('canvas');
    out.width  = canvas.width;
    out.height = canvas.height + Math.round(stampHeight()*DPR);
    var g = out.getContext('2d');
    g.setTransform(DPR, 0, 0, DPR, 0, 0);
    g.fillStyle = CANVAS_BG;  g.fillRect(0, 0, out.width/DPR, out.height/DPR);
    g.setTransform(1, 0, 0, 1, 0, 0);
    g.drawImage(canvas, 0, 0);
    g.setTransform(DPR, 0, 0, DPR, 0, 0);
    stampProvenance(g, out.width/DPR, canvas.height/DPR);
    return out;
  }
  // ONE basename for both files the export writes. It was inline in the click handler and is a function
  // now because renderProv's last FACTS line has to name the .txt the export is about to write: two
  // independently-built names are two names that drift, and a drifted one leaves a picture pointing at a
  // file that is not the one beside it — a disclosure that reads as precise and is wrong.
  function exportBase() {
    var slug = (currentView === 'node' && centreGid >= 0) ? NODES[centreGid].label : (currentView || 'graph');
    return 'ripwire-' + String(slug).replace(/[^A-Za-z0-9_.-]+/g, '_') + '-' + mode;
  }
  // The COMPANION FILE: the whole caption as plain text, provenance first and then the method half the
  // bitmap no longer carries. Both halves, because this is the file a README author lifts the wording out
  // of, and a sidecar holding only the part that moved would make them reassemble the sentence by hand.
  //
  // Built from the SAME DOM the page renders, not from a second set of strings: a plain-text copy of
  // these clauses maintained beside the HTML ones is a second thing that can disagree with the picture,
  // which is the argument that gave the hulls and the labels one placeTextAt instead of two.
  function sidecarText() {
    function textOf(id) { var e = document.getElementById(id); return e ? e.innerText : ''; }
    var out = ['ripwire --html — provenance and method for ' + exportBase() + '.png', ''];
    out = out.concat(textOf('provfacts').split('\n'));
    var spans = document.querySelectorAll('#provmethod .mc');
    if (spans.length) {
      out.push('', 'how to read this picture');
      for (var i = 0; i < spans.length; i++) { out.push('  ' + spans[i].innerText); }
    }
    return out.join('\n') + '\n';
  }
  function download(name, href) {
    var a = document.createElement('a');
    a.download = name; a.href = href;
    document.body.appendChild(a); a.click(); document.body.removeChild(a);
  }
  document.getElementById('savePng').addEventListener('click', function() {
    if (canvas.style.display === 'none') { info.textContent = 'nothing to export from the module list — open Graph, a module or a symbol first'; return; }
    draw();                                     // guarantee the frame is current, not a stale hover state
    var base = exportBase();
    download(base + '.png', exportBitmap().toDataURL('image/png'));
    download(base + '.txt', 'data:text/plain;charset=utf-8,' + encodeURIComponent(sidecarText()));
  });

  // boot
  if (GN === 0) { setChrome(false, true); resize(); paintBackdrop(); ctx.fillStyle='#888'; ctx.font='18px sans-serif'; ctx.fillText('No nodes', 40, 40); renderProv(); return; }
  resize();
  if (!location.hash) location.hash = '#graph';   // the boot default: the map itself (see renderGraph)
  route();
  resize();   // the caption's height is only knowable once it has content; re-measure so the canvas fits
})();
