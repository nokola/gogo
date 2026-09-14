//! The placed tree as flat tables — per node its placed parent, first
//! child and next sibling, a second sibling chain in paint order, and an
//! `off` mark on the repeat slots not shown — built once per resolve
//! that moved a seat, changed a repeat's rows or a raise, and read by
//! every walk after it: layout, draw, hit, input.
//!
//! Needed because the arena's index ranges are not subtrees: children
//! are placed before their own children, so `id+1..subtree_end` holds
//! later siblings, and the rows a repeat clones after the build sit
//! past every ancestor's range. A range test stood in for a tree walk
//! four times (todo-bugs.md B37, the font strip's names standing while
//! the sheet slid). With the tables there is nothing else to walk, and
//! the walks that each built their own child lists (the retained layout,
//! the draw cache, the hit-test mover) share one build.
//!
//! No walk here allocates: children follow a sibling chain, a subtree
//! is a pre-order walk that climbs by the parent table instead of
//! keeping a stack.

use crate::page::{NO_NODE, NodeId, Page};

/// See the module doc.
#[derive(Debug, Default)]
pub struct PlacedTree {
    parent: Vec<NodeId>,
    first: Vec<NodeId>,
    next: Vec<NodeId>,
    /// The sibling chain in paint order: siblings by their raise, stable
    /// (equal raises keep the tree's order). Same links as `first`/`next`
    /// under a parent with no raised child.
    paint_first: Vec<NodeId>,
    paint_next: Vec<NodeId>,
    /// A repeat slot cloned once and not shown now (its key left, or it
    /// awaits a new one): in the raw chains (a layout forgets its rects
    /// with the rest of a re-measured subtree), skipped by the walks
    /// that draw, hit and lay out.
    off: Vec<bool>,
    /// Scratch for the build: each parent's chain tail, then a done mark.
    last: Vec<NodeId>,
    /// Scratch for the paint sort under a parent with raised children.
    sort: Vec<(i32, NodeId)>,
}

/// The tables only: the build scratch (`last`, `sort`) is not part of
/// what a tree says, and a copy (`clone_from`) does not carry it.
impl PartialEq for PlacedTree {
    fn eq(&self, other: &PlacedTree) -> bool {
        self.parent == other.parent && self.first == other.first && self.next == other.next && self.paint_first == other.paint_first && self.paint_next == other.paint_next && self.off == other.off
    }
}

impl Clone for PlacedTree {
    fn clone(&self) -> PlacedTree {
        let mut out = PlacedTree::default();
        out.clone_from(self);
        out
    }

    /// Field-wise, so a host's per-frame copy of `Resolved` reuses the
    /// buffers it already holds (the derived `clone_from` would allocate).
    fn clone_from(&mut self, from: &PlacedTree) {
        self.parent.clone_from(&from.parent);
        self.first.clone_from(&from.first);
        self.next.clone_from(&from.next);
        self.paint_first.clone_from(&from.paint_first);
        self.paint_next.clone_from(&from.paint_next);
        self.off.clone_from(&from.off);
    }
}

impl PlacedTree {
    /// How many nodes the tables cover — 0 until the first `build`.
    pub fn len(&self) -> usize {
        self.parent.len()
    }

    pub fn is_empty(&self) -> bool {
        self.parent.is_empty()
    }

    /// Rebuilds every table from the page and the resolve's seat table,
    /// live rows per repeat and raises. Linear in the node count, four
    /// passes, and it allocates only when the page grew.
    ///
    /// A parent's chain, in order: its declared children not seated
    /// away, then the nodes seated under it (a `Seat`), then a repeat's
    /// live rows in display order, then the repeat's other slots marked
    /// `off` — the order `Resolved::children` gave before the tables.
    pub fn build(&mut self, page: &Page, seat: &[NodeId], repeat_rows: &[(NodeId, Vec<NodeId>)], raised: &[(NodeId, i32)]) {
        let n: usize = page.nodes.len();
        for t in [&mut self.parent, &mut self.first, &mut self.next, &mut self.last] {
            t.clear();
            t.resize(n, NO_NODE);
        }
        self.off.clear();
        self.off.resize(n, true);
        for i in 0..n {
            let id = NodeId::from_index(i);
            self.parent[i] = match seat.get(i).copied().unwrap_or(NO_NODE) {
                NO_NODE => page.declared_parent(id),
                host => host,
            };
            if self.parent[i] == NO_NODE {
                self.off[i] = false; // a root is always shown
            }
        }
        // Pass 1: declared children still under the parent that wrote them.
        for p in 0..n {
            for c in page.children(NodeId::from_index(p)) {
                if seat.get(c.index()).copied().unwrap_or(NO_NODE) == NO_NODE {
                    self.link(NodeId::from_index(p), c);
                }
            }
        }
        // Pass 2: the nodes a seat moved, under their host, in index order.
        for i in 0..n {
            let host = seat.get(i).copied().unwrap_or(NO_NODE);
            if host != NO_NODE && host.index() < n {
                self.link(host, NodeId::from_index(i));
            }
        }
        // Pass 3: a repeat's live rows, in display order. A row a seat
        // moved is already under its host (pass 2), never linked twice.
        for (repeat, rows) in repeat_rows {
            if repeat.index() >= n {
                continue;
            }
            for r in rows {
                if r.index() < n && self.off[r.index()] {
                    self.link(*repeat, *r);
                }
            }
        }
        // Pass 4: everything still unlinked — a repeat's slots not shown
        // — joins its parent's chain marked `off`.
        for i in 0..n {
            let p = self.parent[i];
            if self.off[i] && p != NO_NODE && p.index() < n {
                self.link_off(p, NodeId::from_index(i));
            }
        }
        self.paint_first.clone_from(&self.first);
        self.paint_next.clone_from(&self.next);
        // The paint chains: a parent with a raised child gets its chain
        // stable-sorted by raise. `last` doubles as the done mark.
        for (node, _) in raised {
            if node.index() >= n || self.off[node.index()] {
                continue;
            }
            let p = self.parent[node.index()];
            if p == NO_NODE || self.last[p.index()] == NO_NODE {
                continue;
            }
            self.last[p.index()] = NO_NODE;
            self.sort.clear();
            let mut c = self.first[p.index()];
            while c != NO_NODE {
                let raise: i32 = raised.iter().find(|(r, _)| *r == c).map(|(_, v)| *v).unwrap_or(0);
                self.sort.push((raise, c));
                c = self.next[c.index()];
            }
            self.sort.sort_by_key(|(raise, _)| *raise);
            let mut prev = NO_NODE;
            for k in 0..self.sort.len() {
                let c = self.sort[k].1;
                if prev == NO_NODE {
                    self.paint_first[p.index()] = c;
                } else {
                    self.paint_next[prev.index()] = c;
                }
                prev = c;
            }
            if prev != NO_NODE {
                self.paint_next[prev.index()] = NO_NODE;
            }
        }
    }

    fn link(&mut self, p: NodeId, c: NodeId) {
        self.link_off(p, c);
        self.off[c.index()] = false;
    }

    fn link_off(&mut self, p: NodeId, c: NodeId) {
        let tail = self.last[p.index()];
        if tail == NO_NODE {
            self.first[p.index()] = c;
        } else {
            self.next[tail.index()] = c;
        }
        self.last[p.index()] = c;
    }

    /// The placed parent (`NO_NODE` for a root or an id past the build).
    pub fn parent(&self, id: NodeId) -> NodeId {
        self.parent.get(id.index()).copied().unwrap_or(NO_NODE)
    }

    /// A repeat slot cloned once and not shown now.
    pub fn is_off(&self, id: NodeId) -> bool {
        self.off.get(id.index()).copied().unwrap_or(false)
    }

    /// Whether `node` is `root` or sits anywhere under it in the placed
    /// tree — a parent walk, never an index range.
    pub fn is_under(&self, node: NodeId, root: NodeId) -> bool {
        let mut n = node;
        while n != NO_NODE {
            if n == root {
                return true;
            }
            n = self.parent(n);
        }
        false
    }

    /// The shown children of `id`, in placed order.
    pub fn children(&self, id: NodeId) -> Children<'_> {
        Children { tree: self, cur: self.first.get(id.index()).copied().unwrap_or(NO_NODE), paint: false, all: false }
    }

    /// Every child of `id`, the off slots too (a layout forgetting rects).
    pub fn all_children(&self, id: NodeId) -> Children<'_> {
        Children { tree: self, cur: self.first.get(id.index()).copied().unwrap_or(NO_NODE), paint: false, all: true }
    }

    /// The shown children in paint order (siblings by their raise).
    pub fn paint_children(&self, id: NodeId) -> Children<'_> {
        Children { tree: self, cur: self.paint_first.get(id.index()).copied().unwrap_or(NO_NODE), paint: true, all: false }
    }

    /// `id` and every shown node under it, parents before children
    /// (pre-order), siblings in placed order.
    pub fn subtree(&self, id: NodeId) -> Subtree<'_> {
        Subtree { tree: self, root: id, cur: if id.index() < self.len() { id } else { NO_NODE }, paint: false, all: false }
    }

    /// `subtree` with siblings in paint order.
    pub fn paint_subtree(&self, id: NodeId) -> Subtree<'_> {
        Subtree { tree: self, root: id, cur: if id.index() < self.len() { id } else { NO_NODE }, paint: true, all: false }
    }

    /// `subtree` with the off slots too.
    pub fn all_subtree(&self, id: NodeId) -> Subtree<'_> {
        Subtree { tree: self, root: id, cur: if id.index() < self.len() { id } else { NO_NODE }, paint: false, all: true }
    }

    fn sibling_after(&self, id: NodeId, paint: bool) -> NodeId {
        if paint { self.paint_next[id.index()] } else { self.next[id.index()] }
    }

    fn first_child_of(&self, id: NodeId, paint: bool) -> NodeId {
        if paint { self.paint_first[id.index()] } else { self.first[id.index()] }
    }

    /// From `c` along the sibling chain to the first node the walk shows.
    fn shown_from(&self, mut c: NodeId, paint: bool, all: bool) -> NodeId {
        while c != NO_NODE && !all && self.off[c.index()] {
            c = self.sibling_after(c, paint);
        }
        c
    }
}

/// One parent's children along its sibling chain.
pub struct Children<'a> {
    tree: &'a PlacedTree,
    cur: NodeId,
    paint: bool,
    all: bool,
}

impl Iterator for Children<'_> {
    type Item = NodeId;

    fn next(&mut self) -> Option<NodeId> {
        let c = self.tree.shown_from(self.cur, self.paint, self.all);
        if c == NO_NODE {
            self.cur = NO_NODE;
            return None;
        }
        self.cur = self.tree.sibling_after(c, self.paint);
        Some(c)
    }
}

/// A pre-order walk of one subtree without a stack: down to the first
/// child, else across to the next sibling, else up by the parent table
/// until a sibling is found or the root is reached.
pub struct Subtree<'a> {
    tree: &'a PlacedTree,
    root: NodeId,
    cur: NodeId,
    paint: bool,
    all: bool,
}

impl Iterator for Subtree<'_> {
    type Item = NodeId;

    fn next(&mut self) -> Option<NodeId> {
        let c = self.cur;
        if c == NO_NODE {
            return None;
        }
        let t = self.tree;
        let mut succ = t.shown_from(t.first_child_of(c, self.paint), self.paint, self.all);
        if succ == NO_NODE {
            let mut n = c;
            loop {
                if n == self.root {
                    break;
                }
                let s = t.shown_from(t.sibling_after(n, self.paint), self.paint, self.all);
                if s != NO_NODE {
                    succ = s;
                    break;
                }
                n = t.parent(n);
                if n == NO_NODE {
                    break;
                }
            }
        }
        self.cur = succ;
        Some(c)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::compile;
    use crate::runtime::{Runtime, resolve};

    fn ok(src: &str) -> Page {
        let c = compile(src, &|_| None);
        assert!(!crate::diag::has_errors(&c.diags), "{:#?}", c.diags);
        c.page
    }

    fn names(page: &Page, ids: impl Iterator<Item = NodeId>) -> Vec<String> {
        ids.map(|n| page.str(page.nodes[n].name).to_string()).collect()
    }

    /// Cloned rows hang under their repeat in display order; a later
    /// sibling's body is not in the sheet's subtree; the walk climbs out.
    #[test]
    fn rows_and_later_siblings_walk_as_the_placed_tree() {
        let mut page = ok("View v {\n Column c {\n  Panel sheet { Column inner { repeat rows capacity=2 { Label r text=\"row\" } } }\n  Panel later { Label deep text=\"d\" }\n }\n}\n");
        let mut rt = Runtime::new(&page);
        let rows = page.find("rows").unwrap();
        rt.set_count(&mut page, rows, 2);
        let res = resolve(&page, &rt);
        let t = &res.tree;
        let sheet = page.find("sheet").unwrap();
        assert_eq!(names(&page, t.subtree(sheet)), vec!["sheet", "inner", "rows", "rows/item/r", "rows/0/r", "rows/1/r"]);
        assert_eq!(names(&page, t.children(rows)), vec!["rows/item/r", "rows/0/r", "rows/1/r"]);
        assert!(t.is_under(page.find("rows/1/r").unwrap(), sheet));
        assert!(!t.is_under(page.find("deep").unwrap(), sheet));
        assert_eq!(names(&page, t.subtree(NodeId::new(0))), vec!["v", "c", "sheet", "inner", "rows", "rows/item/r", "rows/0/r", "rows/1/r", "later", "deep"]);
        // A row that leaves is off: out of the shown walks, still in the raw one.
        rt.set_count(&mut page, rows, 1);
        let res = resolve(&page, &rt);
        let t = &res.tree;
        assert_eq!(names(&page, t.children(rows)), vec!["rows/item/r", "rows/0/r"]);
        assert_eq!(names(&page, t.all_children(rows)), vec!["rows/item/r", "rows/0/r", "rows/1/r"]);
        assert!(t.is_off(page.find("rows/1/r").unwrap()));
        assert_eq!(names(&page, t.subtree(sheet)), vec!["sheet", "inner", "rows", "rows/item/r", "rows/0/r"]);
        assert_eq!(names(&page, t.all_subtree(sheet)), vec!["sheet", "inner", "rows", "rows/item/r", "rows/0/r", "rows/1/r"]);
    }

    /// Siblings paint by their raise, stable; the placed chain keeps the
    /// file's order.
    #[test]
    fn paint_order_follows_raises_and_keeps_ties_in_file_order() {
        let page = ok("View v {\n Layers l {\n  Panel a\n  Panel b\n  Panel c\n }\n}\n");
        let mut rt = Runtime::new(&page);
        let (a, b, c) = (page.find("a").unwrap(), page.find("b").unwrap(), page.find("c").unwrap());
        rt.set_raise(b, -1);
        rt.set_raise(a, 1);
        let res = resolve(&page, &rt);
        let l = page.find("l").unwrap();
        assert_eq!(res.tree.children(l).collect::<Vec<NodeId>>(), vec![a, b, c]);
        assert_eq!(res.tree.paint_children(l).collect::<Vec<NodeId>>(), vec![b, c, a]);
        assert_eq!(res.tree.paint_subtree(l).collect::<Vec<NodeId>>(), vec![l, b, c, a]);
    }
}
