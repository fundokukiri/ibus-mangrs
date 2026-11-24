//! The state-B next-word ranking: the evaluator's stupid-backoff contract,
//! ported as a pure function over tier rows so the measured numbers
//! (DEVLOG, task 5) transfer to the engine unchanged.
//!
//! Three tiers, best first: the context's own rows (the pair's trigram, or
//! the last word's bigram when only one context word exists) scored as
//! count / row-total; the bigram rows of the last word at `BACKOFF` times
//! their row share; and the global unigram stream at `BACKOFF²`. Row-total
//! denominators, the surface tie-break, and the A-member rule (a candidate
//! the context rows already carry keeps its context score, never its
//! backoff score) all mirror `scripts/eval-predict-model.py`.
//!
//! The pick-driven lift is multiplicative — `1 + LIFT_PAIR*p + LIFT_FREQ*f`
//! on a candidate's final score (see `learn`) — because the tiers live in
//! probability space, where an additive count-space bonus would dwarf every
//! term.

// Stupid backoff per tier (design-pinned; the task-5 measurement put the
// fused scorer +1.53pp top-5 over bigram-only on 24.5M held-out positions).
pub(crate) const BACKOFF: f64 = 0.4;
/// Two hops of backoff land on the unigram tier (0.4 * 0.4).
pub(crate) const BACKOFF2: f64 = BACKOFF * BACKOFF;
/// One scored tier: the rows (id, count), their sum as the probability
/// denominator, and the stupid-backoff multiplier (1.0 for the context
/// tier itself, 0.0 when the tier is absent).
pub(crate) struct Tier<'a> {
    pub(crate) rows: &'a [(u32, u32)],
    pub(crate) total: f64,
    pub(crate) factor: f64,
}

/// The global unigram stream: row ids ordered count-desc then surface-asc
/// (positive counts only), their counts, the stream total as denominator,
/// and the tier's backoff multiplier.
pub(crate) struct UniStream<'a, C>
where
    C: Fn(u32) -> u32,
{
    pub(crate) order: &'a [u32],
    pub(crate) count: C,
    pub(crate) total: f64,
    pub(crate) factor: f64,
}

/// Merge the three tiers into the best `cap` candidates, best id first.
///
/// The A-member rule mirrors the evaluator: a candidate the context tier
/// already carries keeps its context score, never its backoff score. The
/// lift closure turns the pick counts into a multiplicative bonus; it is
/// deliberately not applied to the unigram tier, whose candidates carry no
/// context meaning. `surface_cmp` breaks score ties by name.
pub(crate) fn fused_state_b(
    ctx: Tier<'_>,
    backoff: Tier<'_>,
    uni: UniStream<'_, impl Fn(u32) -> u32>,
    surface_cmp: impl Fn(u32, u32) -> std::cmp::Ordering,
    lift: impl Fn(u32) -> f64,
    cap: usize,
) -> Vec<u32> {
    let ctx_total = ctx.total.max(1.0);
    let backoff_total = backoff.total.max(1.0);
    let mut in_ctx = std::collections::HashSet::new();
    let mut in_backoff = std::collections::HashSet::new();
    // Kept sorted best-first by (score desc, surface asc); never longer
    // than `cap`. `consider` is the single entry point for every tier.
    let mut pool: Vec<(f64, u32)> = Vec::new();
    let consider = |pool: &mut Vec<(f64, u32)>, score: f64, id: u32| {
        if pool.len() == cap {
            let (worst_score, worst_id) = pool[cap - 1];
            let worse = worst_score
                .total_cmp(&score)
                .then_with(|| surface_cmp(worst_id, id));
            if worse != std::cmp::Ordering::Less {
                return; // the new candidate cannot displace the worst kept
            }
            pool.pop();
        } else if pool.len() > cap {
            return;
        }
        // An existing entry sorts before the new one when it is strictly
        // better: a higher score, or an equal score with a smaller surface.
        let pos = pool.partition_point(|&(s, i)| {
            s.total_cmp(&score) == std::cmp::Ordering::Greater
                || (s.total_cmp(&score) == std::cmp::Ordering::Equal
                    && surface_cmp(i, id) == std::cmp::Ordering::Less)
        });
        pool.insert(pos, (score, id));
        if pool.len() > cap {
            pool.pop();
        }
    };

    // The context tier competes whole — the engine serves `cap` slots, so
    // unlike the evaluator's top-5 stream no row may be dropped for
    // ranking past the seed (it would vanish from ranks 6..cap).
    for &(id, count) in ctx.rows {
        in_ctx.insert(id);
        consider(&mut pool, count as f64 / ctx_total * lift(id), id);
    }
    if backoff.factor > 0.0 && !backoff.rows.is_empty() {
        for &(id, _) in backoff.rows {
            in_backoff.insert(id);
        }
        for &(id, count) in backoff.rows {
            if in_ctx.contains(&id) {
                continue; // a context member keeps its context score
            }
            consider(
                &mut pool,
                backoff.factor * count as f64 / backoff_total * lift(id),
                id,
            );
        }
    }
    // The unigram stream only descends (count order), so once it falls
    // below the kept worst, every later row loses too. Ties at the worst
    // keep walking: within one count the stream orders surfaces ascending,
    // but the kept worst may sit in a different count group.
    let u_scale = uni.factor / uni.total.max(1.0);
    for &id in uni.order {
        if in_ctx.contains(&id) || in_backoff.contains(&id) {
            continue;
        }
        let score = u_scale * (uni.count)(id) as f64;
        if pool.len() == cap {
            let (worst_score, _) = pool[cap - 1];
            if score < worst_score {
                break;
            }
        }
        consider(&mut pool, score, id);
    }
    pool.into_iter().map(|(_, id)| id).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Synthetic vocabulary: stable ids with stable names, so the tie-break
    // and the expected orders read directly.
    const X: u32 = 0;
    const Y: u32 = 1;
    const Z: u32 = 2;
    const U: u32 = 3;
    const V: u32 = 4;
    const W: u32 = 5;
    fn name(id: u32) -> &'static str {
        match id {
            X => "x",
            Y => "y",
            Z => "z",
            U => "u",
            V => "v",
            W => "w",
            _ => "?",
        }
    }
    fn cmp(x: u32, y: u32) -> std::cmp::Ordering {
        name(x).cmp(name(y))
    }
    fn tier(rows: &[(u32, u32)], total: f64, factor: f64) -> Tier<'_> {
        Tier {
            rows,
            total,
            factor,
        }
    }

    #[test]
    fn fused_state_b_ranks_the_trigram_over_backoff() {
        // Context (trigram) total 10: y=6, x=4. Backoff (bigram) total 10:
        // z=9, x=9. Scores: y 0.6, x 0.4 (an A member keeps its A score),
        // z 0.36; the unigram tier (0.16*count/1000) fills behind them.
        let ctx_rows = [(Y, 6), (X, 4)];
        let backoff_rows = [(Z, 9), (X, 9)];
        let out = fused_state_b(
            tier(&ctx_rows, 10.0, 1.0),
            tier(&backoff_rows, 10.0, BACKOFF),
            UniStream {
                order: &[U, V, W],
                count: |id| match id {
                    U => 100,
                    V => 50,
                    W => 10,
                    _ => 0,
                },
                total: 1000.0,
                factor: BACKOFF2,
            },
            cmp,
            |_| 1.0,
            25,
        );
        assert_eq!(out, vec![Y, X, Z, U, V, W]);
    }

    #[test]
    fn fused_state_b_an_a_member_keeps_its_a_score() {
        // x sits in both tiers. Its context score is 0.1 (lifted 3.0); its
        // backoff score would be 0.9 (lifted 2.7) — enough to overtake y
        // (0.6). The A-member rule must keep the context score: y first.
        let ctx_rows = [(Y, 6), (X, 1)];
        let backoff_rows = [(X, 9)];
        let out = fused_state_b(
            tier(&ctx_rows, 10.0, 1.0),
            tier(&backoff_rows, 10.0, BACKOFF),
            UniStream {
                order: &[],
                count: |_| 0,
                total: 1000.0,
                factor: BACKOFF2,
            },
            cmp,
            |id| if id == X { 3.0 } else { 1.0 },
            25,
        );
        assert_eq!(out, vec![Y, X], "an A member keeps its A score");
    }

    #[test]
    fn fused_state_b_lifts_a_picked_candidate() {
        // Lifted x (0.4 * 3 = 1.2) overtakes untouched y (0.6).
        let ctx_rows = [(Y, 6), (X, 4)];
        let out = fused_state_b(
            tier(&ctx_rows, 10.0, 1.0),
            tier(&[], 1.0, 0.0),
            UniStream {
                order: &[],
                count: |_| 0,
                total: 1000.0,
                factor: BACKOFF2,
            },
            cmp,
            |id| if id == X { 3.0 } else { 1.0 },
            25,
        );
        assert_eq!(out, vec![X, Y]);
    }

    #[test]
    fn fused_state_b_caps_and_keeps_the_best() {
        // Eight same-shape candidates over a cap of 3: the top three by
        // count survive, nothing else leaks in.
        let rows: Vec<(u32, u32)> = (0..8).map(|i| (i, 8 - i)).collect();
        let out = fused_state_b(
            tier(&rows, 36.0, 1.0),
            tier(&[], 1.0, 0.0),
            UniStream {
                order: &[],
                count: |_| 0,
                total: 1000.0,
                factor: BACKOFF2,
            },
            cmp,
            |_| 1.0,
            3,
        );
        assert_eq!(out, vec![0, 1, 2]);
    }

    #[test]
    fn fused_state_b_ties_break_by_surface() {
        // Equal counts across two candidates: the smaller surface leads.
        let rows = [(W, 5), (V, 5)];
        let out = fused_state_b(
            tier(&rows, 10.0, 1.0),
            tier(&[], 1.0, 0.0),
            UniStream {
                order: &[],
                count: |_| 0,
                total: 1000.0,
                factor: BACKOFF2,
            },
            cmp,
            |_| 1.0,
            25,
        );
        assert_eq!(out, vec![V, W]);
    }
}
