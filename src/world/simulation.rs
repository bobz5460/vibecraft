//! Fixed-rate singleplayer simulation timing independent from rendering.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

pub const SIMULATION_TPS: u32 = 20;
pub const SIMULATION_DT: f32 = 1.0 / SIMULATION_TPS as f32;
const MAX_CATCH_UP_STEPS: usize = 5;
pub const MAX_SCHEDULED_TICKS_PER_STEP: usize = 256;

/// Persistent simulation work. The variants are deliberately broad until the
/// block-state and entity platforms grow their own tick implementations.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum ScheduledTickKind {
    Water,
    Lava,
    Random,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct ScheduledTick {
    pub due_tick: u64,
    pub chunk: [i32; 2],
    pub kind: ScheduledTickKind,
}

/// A bounded, deterministic event queue. Events remain queued while their
/// chunk is unloaded, so streaming cannot discard pending world behavior.
#[derive(Clone, Debug, Default)]
pub struct TickScheduler {
    events: BTreeMap<u64, Vec<ScheduledTick>>,
    keys: HashSet<(u64, [i32; 2], ScheduledTickKind)>,
}

impl TickScheduler {
    pub fn from_events(events: impl IntoIterator<Item = ScheduledTick>) -> Self {
        let mut scheduler = Self::default();
        for event in events {
            scheduler.schedule(event);
        }
        scheduler
    }

    pub fn schedule(&mut self, event: ScheduledTick) {
        let key = (event.due_tick, event.chunk, event.kind);
        if self.keys.insert(key) {
            self.events.entry(event.due_tick).or_default().push(event);
        }
    }

    pub fn drain_due(&mut self, tick: u64) -> Vec<ScheduledTick> {
        let due_times: Vec<_> = self.events.range(..=tick).map(|(&due, _)| due).collect();
        let mut due = Vec::new();
        for due_tick in due_times {
            let Some(events) = self.events.remove(&due_tick) else {
                continue;
            };
            for event in events {
                self.keys.remove(&(event.due_tick, event.chunk, event.kind));
                if due.len() == MAX_SCHEDULED_TICKS_PER_STEP {
                    // Preserve unprocessed work at the next tick rather than
                    // allowing a single loaded world to stall a simulation step.
                    self.schedule(ScheduledTick { due_tick: tick + 1, ..event });
                } else {
                    due.push(event);
                }
            }
        }
        due
    }

    pub fn events(&self) -> Vec<ScheduledTick> {
        self.events
            .values()
            .flat_map(|events| events.iter().copied())
            .collect()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct FixedStepClock {
    accumulator: f32,
}

impl FixedStepClock {
    pub fn new() -> Self {
        Self { accumulator: 0.0 }
    }

    /// Returns the number of 20 TPS simulation steps due this render frame.
    /// Excess elapsed time is bounded so an unresponsive window cannot cause
    /// an unbounded catch-up loop.
    pub fn advance(&mut self, elapsed: f32) -> usize {
        if !elapsed.is_finite() || elapsed <= 0.0 {
            return 0;
        }
        self.accumulator = (self.accumulator + elapsed)
            .min(SIMULATION_DT * MAX_CATCH_UP_STEPS as f32);
        let steps = (self.accumulator / SIMULATION_DT).floor() as usize;
        self.accumulator -= steps as f32 * SIMULATION_DT;
        steps
    }

    /// Fraction of the next simulation step that elapsed for render-only
    /// interpolation. Simulation state itself must only change in `advance`.
    pub fn interpolation_alpha(&self) -> f32 {
        (self.accumulator / SIMULATION_DT).clamp(0.0, 1.0)
    }
}

impl Default for FixedStepClock {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advances_at_twenty_ticks_per_second_independent_of_frame_size() {
        let mut clock = FixedStepClock::new();
        let mut ticks = 0;
        for _ in 0..60 {
            ticks += clock.advance(1.0 / 60.0);
        }
        assert_eq!(ticks, 20);
        assert!(clock.interpolation_alpha() < 0.001);
    }

    #[test]
    fn bounds_catch_up_after_a_stall() {
        let mut clock = FixedStepClock::new();
        assert_eq!(clock.advance(10.0), MAX_CATCH_UP_STEPS);
        assert!(clock.interpolation_alpha() < 0.001);
    }

    #[test]
    fn scheduled_ticks_are_deduplicated_and_survive_deferred_processing() {
        let water = ScheduledTick {
            due_tick: 4,
            chunk: [-2, 7],
            kind: ScheduledTickKind::Water,
        };
        let mut scheduler = TickScheduler::default();
        scheduler.schedule(water);
        scheduler.schedule(water);
        assert!(scheduler.drain_due(3).is_empty());
        assert_eq!(scheduler.drain_due(4), vec![water]);
        assert!(scheduler.events().is_empty());
    }
}
