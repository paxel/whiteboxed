//! One cost over a whole drawing, and one search over the choices the routing
//! still has once the boxes stand: which candidate route a line takes, the order of
//! the lanes in a channel, which lines go under, and which branches of a fanned-out
//! relation get a frame point of their own. A rule about the picture is a cost
//! term here, never a special case in the pipeline.

use std::collections::{BTreeMap, BTreeSet};

use super::{Chan, Lanes, Route, SideOrder};
use crate::model::End;

/// What a drawing costs, worst first: the readability measures (crossings,
/// overlaps, holes, lines through the title or a band, at their exchange rates),
/// then looks (bends, short steps, length). A lower cost is a better drawing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Cost {
    pub tangle: i64,
    pub cosmetic: i64,
}

/// One setting of every choice.
#[derive(Debug, Clone, PartialEq)]
pub struct Choice {
    /// Per line, the candidate route it takes; `None` for the first one offered.
    pub route: Vec<Option<Route>>,
    /// The order of the lines along each channel; lines missing here join at the end.
    pub lanes: Lanes,
    /// The order of the ports along each side of a box or the frame; ports missing
    /// here join where their wish puts them.
    pub order: SideOrder,
    /// Ports moved half a lane along their side, off the spot the spacing gave them.
    pub shift: BTreeMap<(usize, End), f32>,
    /// Lines through mouseholes.
    pub hidden: BTreeSet<usize>,
    /// Frame ends that left the frame point shared by the branches of their relation.
    pub unshared: BTreeSet<(usize, End)>,
}

/// How many drawings the search may look at for one layout.
pub const BUDGET: usize = 4000;

/// Deterministic local search: from `start`, the first move that lowers the cost
/// is taken, until no move does or the budget is spent. `evaluate` draws a choice
/// and prices it; `moves` lists the neighbours of a choice as drawn, cheapest
/// remedies first. Same input, same drawing.
pub fn local_search<D>(
    start: Choice,
    evaluate: &mut dyn FnMut(&Choice) -> (Cost, D),
    moves: &dyn Fn(&Choice, &D) -> Vec<Choice>,
    budget: usize,
) -> (Choice, D) {
    let mut current = start;
    let (mut best, mut drawing) = evaluate(&current);
    let mut left = budget;
    loop {
        let mut improved = false;
        for candidate in moves(&current, &drawing) {
            if left == 0 {
                return (current, drawing);
            }
            left -= 1;
            let (cost, d) = evaluate(&candidate);
            if cost < best {
                best = cost;
                current = candidate;
                drawing = d;
                improved = true;
                break;
            }
        }
        if !improved {
            return (current, drawing);
        }
    }
}

/// The lanes of `wanted` (which lines run along which channel now) in the order
/// `kept` had them, lines new to a channel after the rest.
pub fn reconcile(kept: &Lanes, wanted: &Lanes) -> Lanes {
    let mut out: Lanes = BTreeMap::new();
    for (chan, users) in wanted {
        let mut order: Vec<usize> = kept
            .get(chan)
            .map(|k| k.iter().copied().filter(|li| users.contains(li)).collect())
            .unwrap_or_default();
        for li in users {
            if !order.contains(li) {
                order.push(*li);
            }
        }
        out.insert(*chan, order);
    }
    out
}

/// Every order that moving one item of `list` to another place gives, each once.
pub fn insertion_moves<T: Clone + PartialEq>(list: &[T]) -> Vec<Vec<T>> {
    let mut out: Vec<Vec<T>> = Vec::new();
    let k = list.len();
    for from in 0..k {
        for to in 0..k {
            if from == to {
                continue;
            }
            let mut order = list.to_vec();
            let moved = order.remove(from);
            order.insert(to, moved);
            if order != list && !out.contains(&order) {
                out.push(order);
            }
        }
    }
    out
}

/// The lane orders one move away from `lanes`, per channel.
pub fn lane_moves(lanes: &Lanes) -> Vec<(Chan, Vec<usize>)> {
    lanes
        .iter()
        .flat_map(|(chan, list)| insertion_moves(list).into_iter().map(|o| (*chan, o)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_search_takes_improving_moves_until_none_is_left() {
        // Cost is the route index summed; moves lower one index at a time.
        // Cost is the number of lines under; moves bring one line back up.
        let start = Choice {
            route: Vec::new(),
            lanes: Lanes::new(),
            order: SideOrder::new(),
            shift: BTreeMap::new(),
            hidden: [0, 1, 2].into_iter().collect(),
            unshared: BTreeSet::new(),
        };
        let mut evaluate = |c: &Choice| {
            (
                Cost {
                    tangle: c.hidden.len() as i64,
                    cosmetic: 0,
                },
                (),
            )
        };
        let moves = |c: &Choice, _: &()| {
            c.hidden
                .iter()
                .map(|li| {
                    let mut n = c.clone();
                    n.hidden.remove(li);
                    n
                })
                .collect()
        };
        let (found, _) = local_search(start.clone(), &mut evaluate, &moves, 100);
        assert!(found.hidden.is_empty());
        // A spent budget stops the search where it is.
        let (found, _) = local_search(start, &mut evaluate, &moves, 2);
        assert_eq!(found.hidden.len(), 1);
    }

    #[test]
    fn reconciled_lanes_keep_the_old_order_and_append_newcomers() {
        let mut kept = Lanes::new();
        kept.insert(Chan::V(1), vec![5, 3, 9]);
        let mut wanted = Lanes::new();
        wanted.insert(Chan::V(1), vec![3, 7, 5]);
        wanted.insert(Chan::H(0), vec![1]);
        let out = reconcile(&kept, &wanted);
        assert_eq!(out[&Chan::V(1)], vec![5, 3, 7]);
        assert_eq!(out[&Chan::H(0)], vec![1]);
        // Three lines: four orders are one move away (two moves are swaps that
        // another move gives as well).
        assert_eq!(lane_moves(&out).len(), 4);
        assert_eq!(insertion_moves(&[1, 2]).len(), 1);
    }
}
