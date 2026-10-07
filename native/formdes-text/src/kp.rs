pub const INF: f64 = 10000.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Item {
    Box { w: f64 },
    Glue { w: f64, stretch: f64, shrink: f64 },
    Penalty { w: f64, p: f64, flagged: bool },
}

impl Item {
    pub fn width(&self) -> f64 {
        match *self {
            Item::Box { w } | Item::Glue { w, .. } | Item::Penalty { w, .. } => w,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Params {
    pub tolerance: f64,
    pub line_penalty: f64,
    pub flagged_demerits: f64,
    pub fitness_demerits: f64,
    pub max_hyphens: usize,
    pub end_stretch: f64,
    pub emergency_stretch: f64,
    pub looseness: i32,
}

impl Default for Params {
    fn default() -> Self {
        Params {
            tolerance: 2.0,
            line_penalty: 10.0,
            flagged_demerits: 3000.0,
            fitness_demerits: 1000.0,
            max_hyphens: 0,
            end_stretch: 0.0,
            emergency_stretch: 0.0,
            looseness: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Break {
    pub at: usize,
    pub ratio: f64,
    pub flagged: bool,
}

#[derive(Clone, Copy, Default)]
struct Sums {
    w: f64,
    y: f64,
    z: f64,
    fill: f64,
}

#[derive(Clone)]
struct Node {
    at: usize,
    line: usize,
    fit: usize,
    hyphens: usize,
    total: Sums,
    demerits: f64,
    ratio: f64,
    flagged: bool,
    prev: Option<usize>,
}

pub const FILL: f64 = 1.0e7;

fn after(items: &[Item], b: usize, sums: &Sums) -> Sums {
    let mut s = *sums;
    let mut i = b;
    while i < items.len() {
        match items[i] {
            Item::Box { .. } => break,
            Item::Penalty { p, .. } if p <= -INF && i > b => break,
            Item::Glue { w, stretch, shrink } => {
                s.w += w;
                if stretch >= FILL {
                    s.fill += 1.0;
                } else {
                    s.y += stretch;
                }
                s.z += shrink;
            }
            Item::Penalty { .. } => {}
        }
        i += 1;
    }
    s
}

pub fn ratio_for(natural: f64, stretch: f64, shrink: f64, fill: f64, width: f64) -> f64 {
    let slack = width - natural;
    if slack.abs() < 1e-9 {
        0.0
    } else if slack > 0.0 {
        if fill > 0.0 {
            0.0
        } else if stretch > 1e-9 {
            slack / stretch
        } else {
            INF
        }
    } else if shrink > 1e-9 {
        slack / shrink
    } else {
        -INF
    }
}

fn badness(r: f64) -> f64 {
    if r <= -INF || r >= INF {
        return INF;
    }
    (100.0 * r.abs().powi(3)).min(INF)
}

fn fitness(r: f64) -> usize {
    if r < -0.5 {
        0
    } else if r <= 0.5 {
        1
    } else if r <= 1.0 {
        2
    } else {
        3
    }
}

pub fn breaks(items: &[Item], width: &dyn Fn(usize) -> f64, distinct: usize, params: &Params) -> Option<Vec<Break>> {
    let mut nodes: Vec<Node> = vec![Node {
        at: 0,
        line: 0,
        fit: 1,
        hyphens: 0,
        total: Sums::default(),
        demerits: 0.0,
        ratio: 0.0,
        flagged: false,
        prev: None,
    }];
    let mut active: Vec<usize> = vec![0];
    let mut sums = Sums::default();
    for (b, item) in items.iter().enumerate() {
        let (legal, pen, pw, flagged) = match *item {
            Item::Box { .. } => (false, 0.0, 0.0, false),
            Item::Glue { .. } => (b > 0 && matches!(items[b - 1], Item::Box { .. }), 0.0, 0.0, false),
            Item::Penalty { w, p, flagged } => (p < INF, p, w, flagged),
        };
        if legal {
            let mut candidates: Vec<(f64, Node)> = Vec::new();
            let mut i = 0;
            while i < active.len() {
                let ai = active[i];
                let a = &nodes[ai];
                let line_width = width(a.line);
                let natural = sums.w - a.total.w + pw;
                let stretch = sums.y - a.total.y + params.end_stretch + params.emergency_stretch;
                let shrink = sums.z - a.total.z;
                let fill = sums.fill - a.total.fill;
                let r = ratio_for(natural, stretch, shrink, fill, line_width);
                if r < -1.0 || pen <= -INF {
                    active.remove(i);
                } else {
                    i += 1;
                }
                if r >= -1.0 && r <= params.tolerance {
                    let hy = if flagged { a.hyphens + 1 } else { 0 };
                    if params.max_hyphens > 0 && hy > params.max_hyphens && pen > -INF {
                        continue;
                    }
                    let bad = badness(r);
                    let base = (params.line_penalty + bad).powi(2);
                    let mut d = if pen >= 0.0 { base + pen * pen } else if pen > -INF { base - pen * pen } else { base };
                    if flagged && a.flagged {
                        d += params.flagged_demerits;
                    }
                    let fc = fitness(r);
                    if fc.abs_diff(a.fit) > 1 {
                        d += params.fitness_demerits;
                    }
                    d += a.demerits;
                    let line = a.line + 1;
                    let key_line = line.min(distinct);
                    let idx = candidates.iter().position(|(_, n)| n.fit == fc && n.line.min(distinct) == key_line);
                    let prev = Some(ai);
                    let node = Node {
                        at: b,
                        line,
                        fit: fc,
                        hyphens: hy,
                        total: after(items, b, &sums),
                        demerits: d,
                        ratio: r,
                        flagged,
                        prev,
                    };
                    match idx {
                        Some(k) if candidates[k].0 <= d => {}
                        Some(k) => candidates[k] = (d, node),
                        None => candidates.push((d, node)),
                    }
                }
            }
            for (_, n) in candidates {
                nodes.push(n);
                active.push(nodes.len() - 1);
            }
            if active.is_empty() {
                return None;
            }
        }
        match *item {
            Item::Box { w } => sums.w += w,
            Item::Glue { w, stretch, shrink } => {
                sums.w += w;
                if stretch >= FILL {
                    sums.fill += 1.0;
                } else {
                    sums.y += stretch;
                }
                sums.z += shrink;
            }
            Item::Penalty { .. } => {}
        }
    }
    let last = items.len().checked_sub(1)?;
    let finals: Vec<usize> = active.iter().copied().filter(|&i| nodes[i].at == last).collect();
    let best = finals.iter().copied().min_by(|&a, &b| nodes[a].demerits.total_cmp(&nodes[b].demerits))?;
    let chosen = if params.looseness != 0 {
        let target = nodes[best].line as i64 + params.looseness as i64;
        finals
            .iter()
            .copied()
            .min_by(|&a, &b| {
                let da = (nodes[a].line as i64 - target).abs();
                let db = (nodes[b].line as i64 - target).abs();
                da.cmp(&db).then(nodes[a].demerits.total_cmp(&nodes[b].demerits))
            })
            .unwrap_or(best)
    } else {
        best
    };
    let mut out = Vec::new();
    let mut cur = Some(chosen);
    while let Some(i) = cur {
        if nodes[i].prev.is_none() {
            break;
        }
        out.push(Break { at: nodes[i].at, ratio: nodes[i].ratio, flagged: nodes[i].flagged });
        cur = nodes[i].prev;
    }
    out.reverse();
    Some(out)
}

pub fn greedy(items: &[Item], width: &dyn Fn(usize) -> f64, end_stretch: f64) -> Vec<Break> {
    let mut out = Vec::new();
    let mut line = 0;
    let mut start = 0;
    let mut i = 0;
    let n = items.len();
    while start < n {
        let w = width(line);
        let mut nat = 0.0;
        let mut y = 0.0;
        let mut z = 0.0;
        let mut fill = 0.0;
        let mut last_ok: Option<(usize, f64, bool)> = None;
        let mut forced: Option<(usize, f64, bool)> = None;
        i = i.max(start);
        while i < n {
            let it = items[i];
            let legal = match it {
                Item::Glue { .. } => i > start && matches!(items[i - 1], Item::Box { .. }),
                Item::Penalty { p, .. } => p < INF,
                Item::Box { .. } => false,
            };
            if legal {
                let pw = if let Item::Penalty { w, .. } = it { w } else { 0.0 };
                let r = ratio_for(nat + pw, y + end_stretch, z, fill, w);
                let flagged = matches!(it, Item::Penalty { flagged: true, .. });
                if r >= -1.0 {
                    last_ok = Some((i, r, flagged));
                    if let Item::Penalty { p, .. } = it
                        && p <= -INF
                    {
                        forced = last_ok;
                        break;
                    }
                } else {
                    break;
                }
            }
            match it {
                Item::Box { w: bw } => nat += bw,
                Item::Glue { w: gw, stretch, shrink } => {
                    nat += gw;
                    if stretch >= FILL {
                        fill += 1.0;
                    } else {
                        y += stretch;
                    }
                    z += shrink;
                }
                Item::Penalty { .. } => {}
            }
            i += 1;
        }
        let pick = forced.or(last_ok).or_else(|| {
            let mut j = i.max(start + 1).min(n - 1);
            while j < n && !matches!(items[j], Item::Glue { .. } | Item::Penalty { .. }) {
                j += 1;
            }
            Some((j.min(n - 1), -INF, false))
        });
        let (at, r, flagged) = pick.unwrap_or((n - 1, 0.0, false));
        out.push(Break { at, ratio: r, flagged });
        line += 1;
        let mut s = at + 1;
        while s < n && matches!(items[s], Item::Glue { .. } | Item::Penalty { .. }) && !matches!(items[s], Item::Penalty { p, .. } if p <= -INF) {
            s += 1;
        }
        if at >= n - 1 {
            break;
        }
        start = s;
        i = s;
    }
    out
}
