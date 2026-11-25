//! How long a keystroke takes to reach the application, read back out of the
//! engine's own log: a press line, then the first flush or insert line after
//! it. Nothing here touches the engine — the numbers are only as good as the
//! log, and a press still waiting for its echo is left uncounted rather than
//! given a made-up delay.

use std::collections::VecDeque;

use super::log_decode::{Body, Kind, Line};

/// Presses kept for the average: what a person can still feel, not the whole
/// session.
pub(super) const WINDOW: usize = 200;

/// A reply further away than this is not this key's echo: the engine logs a
/// flush per keystroke, so a line a second later belongs to a later key (or to
/// a session that went idle), and counting it would report seconds of delay
/// where the log holds milliseconds.
pub(super) const MAX_PAIR_MS: u64 = 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Stats {
    pub avg_ms: u64,
    pub max_ms: u64,
    /// How many keystrokes the two numbers come from.
    pub keys: usize,
}

/// Pair each press with the first flush or insert after it. A press with no
/// such line yet is not counted: the keystroke is still in flight.
pub(super) fn measure(lines: &[Line]) -> Option<Stats> {
    let mut pending: Option<u64> = None;
    let mut samples: VecDeque<u64> = VecDeque::new();
    for line in lines {
        match (&line.body, line.kind) {
            (Body::Key { release: false, .. }, _) => pending = line.abs_ms,
            (_, Kind::Flush | Kind::Insert) => {
                if let (Some(from), Some(to)) = (pending.take(), line.abs_ms) {
                    if to >= from && to - from <= MAX_PAIR_MS {
                        if samples.len() == WINDOW {
                            samples.pop_front();
                        }
                        samples.push_back(to - from);
                    }
                }
            }
            _ => {}
        }
    }
    let keys = samples.len();
    if keys == 0 {
        return None;
    }
    let max_ms = samples.iter().copied().max().unwrap_or(0);
    let avg_ms = samples.iter().sum::<u64>() / keys as u64;
    Some(Stats {
        avg_ms,
        max_ms,
        keys,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::log_decode::parse;

    fn lines(raw: &[&str]) -> Vec<Line> {
        let mut out: Vec<Line> = Vec::new();
        for line in raw {
            let prev = out.last().and_then(|l: &Line| l.abs_ms);
            out.push(parse(line, prev).expect("a parsed line"));
        }
        out
    }

    const PRESS: &str = "key keyval=0x69 state=0x0";
    const PRESS_UP: &str = "key keyval=0x69 state=0x40000000";
    const FLUSH: &str = "timed commit flush (per-key)";
    const INSERT: &str = "surrounding cursor=3 tail=\"Gi\" text=\"Gi\"";

    #[test]
    fn a_press_is_measured_until_the_flush_after_it() {
        let log = lines(&[
            "1795619563000 key keyval=0x69 state=0x0",
            "1795619563012 timed commit flush (per-key)",
        ]);
        assert_eq!(
            measure(&log),
            Some(Stats {
                avg_ms: 12,
                max_ms: 12,
                keys: 1
            })
        );
    }

    #[test]
    fn an_insert_line_closes_a_press_too() {
        let log = lines(&[
            "1795619563000 key keyval=0x69 state=0x0",
            "1795619563040 surrounding cursor=3 tail=\"Gi\" text=\"Gi\"",
            "1795619563050 timed commit flush (per-key)",
        ]);
        // The second line closes the sample, so the flush has nothing to pair.
        assert_eq!(measure(&log).map(|s| (s.avg_ms, s.keys)), Some((40, 1)));
    }

    #[test]
    fn a_release_never_starts_a_sample() {
        let log = lines(&[
            "1795619563000 key keyval=0x69 state=0x0",
            "1795619563005 key keyval=0x69 state=0x40000000",
            "1795619563010 timed commit flush (per-key)",
        ]);
        assert_eq!(measure(&log).map(|s| (s.avg_ms, s.keys)), Some((10, 1)));
    }

    #[test]
    fn a_log_without_presses_has_no_numbers() {
        assert_eq!(measure(&[]), None);
        let log = lines(&["1795619563000 timed commit flush (per-key)"]);
        assert_eq!(measure(&log), None);
    }

    #[test]
    fn a_press_still_waiting_for_its_echo_is_not_counted() {
        let log = lines(&["1795619563000 key keyval=0x69 state=0x0"]);
        assert_eq!(measure(&log), None);
    }

    #[test]
    fn a_reply_seconds_later_is_not_this_key_s_echo() {
        let log = lines(&[
            "1795619563000 key keyval=0x69 state=0x0",
            "1795619569000 timed commit flush (per-key)",
        ]);
        assert_eq!(measure(&log), None);
    }

    #[test]
    fn only_the_last_two_hundred_presses_count() {
        let mut raw: Vec<String> = Vec::new();
        for i in 0..300u64 {
            // The engine writes millisecond epochs, thirteen digits.
            raw.push(format!(
                "{} key keyval=0x69 state=0x0",
                1_700_000_000_000 + i * 1000
            ));
            // Delays grow with i, so the average proves which window was used.
            raw.push(format!(
                "{} timed commit flush (per-key)",
                1_700_000_000_000 + i * 1000 + i
            ));
        }
        let refs: Vec<&str> = raw.iter().map(String::as_str).collect();
        let stats = measure(&lines(&refs)).expect("samples");
        assert_eq!(stats.keys, WINDOW);
        assert_eq!(stats.max_ms, 299);
        let expected: u64 = (100..300u64).sum::<u64>() / WINDOW as u64;
        assert_eq!(stats.avg_ms, expected);
    }

    #[test]
    fn an_unparsed_line_between_press_and_flush_does_not_break_the_pairing() {
        let log = lines(&[
            "1795619563000 key keyval=0x69 state=0x0",
            "1795619563001 something the page has never seen",
            "1795619563015 timed commit flush (per-key)",
        ]);
        assert_eq!(measure(&log).map(|s| s.avg_ms), Some(15));
    }

    #[test]
    fn the_press_constant_is_the_one_the_engine_writes() {
        assert!(PRESS.starts_with("key keyval="));
        assert!(PRESS_UP.contains("0x40000000"));
        assert!(FLUSH.starts_with("timed commit flush"));
        assert!(INSERT.starts_with("surrounding"));
    }
}
