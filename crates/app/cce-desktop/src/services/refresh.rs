//! Periodic refresh for panels that show live server state.
//!
//! The web console mounts three polling sources (health 15s, metrics 30s, watch
//! version 10s). The desktop had none: every panel was a manual snapshot, so a
//! panel could sit on a stale answer while the index underneath it moved.
//!
//! One loop drives every cadence instead of one timer per panel. Three reasons:
//! a single loop cannot stack overlapping requests, it keeps the tick cost flat
//! as panels are added, and it makes "which panel refreshes when" one table
//! that a test can read.
//!
//! The loop only asks panels to replay, never to reload blindly: each panel
//! reuses the inputs it last ran with, so a refresh cannot overwrite a user's
//! unsaved edits.

use std::{
    collections::BTreeMap,
    sync::Mutex,
    time::{Duration, Instant},
};

use gpui_kit::{App, AsyncApp, Global, Task};

use crate::views::PanelKind;

/// How often the loop looks for due panels.
const TICK: Duration = Duration::from_millis(500);
/// Floor for any configured cadence, so a misconfiguration cannot spam the
/// server.
pub const MIN_INTERVAL: Duration = Duration::from_secs(5);

/// How often one panel is refreshed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cadence {
    /// Only when the user asks.
    Manual,
    /// Every `interval`, clamped to [`MIN_INTERVAL`].
    Every(Duration),
}

impl Cadence {
    fn interval(self) -> Option<Duration> {
        match self {
            Self::Manual => None,
            Self::Every(interval) => Some(interval.max(MIN_INTERVAL)),
        }
    }
}

/// Cadence table plus the last time each panel was refreshed.
#[derive(Debug, Default)]
pub struct Schedule {
    cadence: BTreeMap<PanelKind, Cadence>,
    last: BTreeMap<PanelKind, Instant>,
}

impl Schedule {
    /// Install the default cadences, aligned with the web console.
    pub fn with_defaults() -> Self {
        let mut schedule = Self::default();
        schedule.set(PanelKind::Health, Cadence::Every(Duration::from_secs(15)));
        schedule.set(PanelKind::Metrics, Cadence::Every(Duration::from_secs(30)));
        schedule.set(PanelKind::Watch, Cadence::Every(Duration::from_secs(10)));
        schedule.set(PanelKind::Storage, Cadence::Every(Duration::from_secs(30)));
        schedule.set(PanelKind::Tasks, Cadence::Every(Duration::from_secs(30)));
        schedule.set(PanelKind::Logs, Cadence::Every(Duration::from_secs(15)));
        // Explicit rather than implicit: a panel missing from this table would
        // silently never refresh, which is the bug this layer exists to fix.
        for kind in [
            PanelKind::Search,
            PanelKind::Graph,
            PanelKind::Entities,
            PanelKind::Index,
            PanelKind::Projects,
            PanelKind::Tools,
            PanelKind::Config,
        ] {
            schedule.set(kind, Cadence::Manual);
        }
        schedule
    }

    pub fn set(&mut self, kind: PanelKind, cadence: Cadence) {
        self.cadence.insert(kind, cadence);
    }

    /// Record that `kind` just refreshed.
    pub fn touch(&mut self, kind: PanelKind, now: Instant) {
        self.last.insert(kind, now);
    }

    /// Panels whose cadence elapsed, in a stable order.
    ///
    /// A panel with no recorded run is due: it was just created, and its own
    /// constructor decides whether to load immediately.
    pub fn due(&self, now: Instant) -> Vec<PanelKind> {
        let mut due = Vec::new();
        for (kind, cadence) in &self.cadence {
            let Some(interval) = cadence.interval() else {
                continue;
            };
            let fresh = self
                .last
                .get(kind)
                .is_some_and(|last| now.duration_since(*last) < interval);
            if !fresh {
                due.push(*kind);
            }
        }
        due
    }
}

/// Global holder so the loop and the panels share one table.
pub struct RefreshGlobal(pub Mutex<Schedule>);

impl Global for RefreshGlobal {}

pub fn init(cx: &mut App) {
    if cx.try_global::<RefreshGlobal>().is_none() {
        cx.set_global(RefreshGlobal(Mutex::new(Schedule::with_defaults())));
    }
}

/// Record a refresh so the next one waits a full interval.
pub fn touch(kind: PanelKind, cx: &App) {
    let Some(global) = cx.try_global::<RefreshGlobal>() else {
        return;
    };
    let mut schedule = global.0.lock().unwrap_or_else(|err| err.into_inner());
    schedule.touch(kind, Instant::now());
}

/// Spawn the single refresh loop.
///
/// Panels are asked to replay only while a client exists: replaying against a
/// dropped connection would replace a good result with a transport failure.
pub fn start(cx: &mut App) -> Task<()> {
    cx.spawn(async move |cx: &mut AsyncApp| {
        loop {
            cx.background_executor().timer(TICK).await;
            let due = cx.update(|cx| {
                if crate::state::AppState::global(cx).read(cx).status
                    != crate::state::ConnectionStatus::Connected
                {
                    return Vec::new();
                }
                let Some(global) = cx.try_global::<RefreshGlobal>() else {
                    return Vec::new();
                };
                let now = Instant::now();
                let mut schedule = global.0.lock().unwrap_or_else(|err| err.into_inner());
                let due = schedule.due(now);
                for kind in &due {
                    schedule.touch(*kind, now);
                }
                due
            });
            for kind in due {
                cx.update(|cx| crate::dock_layout::replay(kind, cx));
                // Yield between panels so a burst of due cadences becomes a
                // sequence of requests instead of a spike.
                cx.background_executor()
                    .timer(Duration::from_millis(1))
                    .await;
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_panels_are_never_due() {
        let mut schedule = Schedule::default();
        schedule.set(PanelKind::Projects, Cadence::Manual);
        assert!(schedule.due(Instant::now()).is_empty());
    }

    #[test]
    fn an_unrun_panel_is_due_then_waits_for_its_interval() {
        let mut schedule = Schedule::default();
        schedule.set(PanelKind::Health, Cadence::Every(Duration::from_secs(15)));
        let start = Instant::now();
        assert_eq!(schedule.due(start), vec![PanelKind::Health]);

        schedule.touch(PanelKind::Health, start);
        assert!(schedule.due(start + Duration::from_secs(14)).is_empty());
        assert_eq!(
            schedule.due(start + Duration::from_secs(15)),
            vec![PanelKind::Health]
        );
    }

    #[test]
    fn cadences_below_the_floor_are_clamped() {
        let mut schedule = Schedule::default();
        schedule.set(
            PanelKind::Metrics,
            Cadence::Every(Duration::from_millis(100)),
        );
        let start = Instant::now();
        schedule.touch(PanelKind::Metrics, start);
        assert!(schedule.due(start + Duration::from_secs(1)).is_empty());
        assert_eq!(schedule.due(start + MIN_INTERVAL), vec![PanelKind::Metrics]);
    }

    #[test]
    fn the_default_table_matches_the_web_console() {
        let mut schedule = Schedule::with_defaults();
        let start = Instant::now();
        for kind in [
            PanelKind::Health,
            PanelKind::Metrics,
            PanelKind::Watch,
            PanelKind::Storage,
            PanelKind::Tasks,
            PanelKind::Logs,
        ] {
            schedule.touch(kind, start);
        }
        // Nothing is due at once; each cadence becomes due on its own tick
        // and stays due until it is touched again.
        assert!(schedule.due(start).is_empty());
        assert_eq!(
            schedule.due(start + Duration::from_secs(10)),
            vec![PanelKind::Watch]
        );
        assert_eq!(
            schedule.due(start + Duration::from_secs(15)),
            vec![PanelKind::Watch, PanelKind::Health, PanelKind::Logs]
        );
        assert_eq!(
            schedule.due(start + Duration::from_secs(30)),
            vec![
                PanelKind::Storage,
                PanelKind::Watch,
                PanelKind::Health,
                PanelKind::Metrics,
                PanelKind::Tasks,
                PanelKind::Logs,
            ]
        );
        assert!(
            schedule
                .due(start + Duration::from_secs(30))
                .contains(&PanelKind::Health)
        );
        assert!(
            !schedule
                .due(start + Duration::from_secs(30))
                .contains(&PanelKind::Search)
        );
    }
}
