//! Cross-panel navigation.
//!
//! Every panel used to be a terminal destination except for one hand-off slot
//! that carried a search hit into the graph panel. That slot is generalised
//! here: a [`Target`] names what a panel should show, and [`NavRequest`] pairs
//! it with a monotonic generation so a receiver can drop a request it already
//! applied while a repeated click on the same row still counts as a delivery.
//!
//! The mapping from [`Target`] to [`PanelKind`] lives here and nowhere else, so
//! a new link is one enum variant plus one `on_navigate` arm in the receiving
//! view. Dispatch stays a `match`: there is no trait object in this path.

use gpui_kit::SharedString;

use crate::views::PanelKind;

/// Where a cross-panel link points.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// Open one stable symbol id in the entity panel.
    Entity { id: SharedString },
    /// Open the graph panel on the neighbourhood of one entity.
    GraphSeed {
        entity_id: SharedString,
        label: SharedString,
    },
    /// Open the graph panel on the neighbourhood of one file.
    ///
    /// File-level hits carry no stable symbol id, so a graph seed cannot
    /// express them; this is the link those hits get instead.
    GraphFile { file: SharedString },
}

impl Target {
    /// Panel that answers this target.
    pub fn panel(&self) -> PanelKind {
        match self {
            Self::Entity { .. } => PanelKind::Entities,
            Self::GraphSeed { .. } | Self::GraphFile { .. } => PanelKind::Graph,
        }
    }

    /// Short label used in notices and tests, never rendered raw.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Entity { .. } => "entity",
            Self::GraphSeed { .. } => "graph_seed",
            Self::GraphFile { .. } => "graph_file",
        }
    }
}

/// One pending navigation, consumed by the window once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavRequest {
    pub target: Target,
    pub generation: u64,
}

/// Small helper that answers "is this delivery new?" for a receiver.
///
/// Receivers keep the last generation they applied; the window keeps its own
/// counter so it can open the right panel exactly once per request.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Applied(u64);

impl Applied {
    /// Start from an already-consumed generation, used when a window opens
    /// after a request was delivered.
    pub fn accepting(generation: u64) -> Self {
        Self(generation)
    }

    /// Record `generation`, returning `true` only for a new delivery.
    pub fn accept(&mut self, generation: u64) -> bool {
        if generation <= self.0 {
            return false;
        }
        self.0 = generation;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn targets() -> [Target; 3] {
        [
            Target::Entity {
                id: SharedString::from("sym_1"),
            },
            Target::GraphSeed {
                entity_id: SharedString::from("sym_1"),
                label: SharedString::from("load"),
            },
            Target::GraphFile {
                file: SharedString::from("src/lib.rs"),
            },
        ]
    }

    #[test]
    fn targets_map_to_the_panel_that_answers_them() {
        let targets = targets();
        assert_eq!(targets[0].panel(), PanelKind::Entities);
        assert_eq!(targets[1].panel(), PanelKind::Graph);
        assert_eq!(targets[2].panel(), PanelKind::Graph);
    }

    #[test]
    fn a_generation_is_applied_once() {
        let mut applied = Applied::default();
        assert!(applied.accept(1));
        assert!(!applied.accept(1));
        assert!(!applied.accept(0));
        assert!(applied.accept(2));
        assert!(!applied.accept(2));
    }

    #[test]
    fn a_window_opened_after_delivery_starts_consumed() {
        let mut applied = Applied::accepting(7);
        assert!(!applied.accept(7), "an old request is not replayed");
        assert!(applied.accept(8));
    }

    #[test]
    fn target_names_are_unique() {
        let mut names: Vec<&str> = targets().iter().map(|target| target.name()).collect();
        names.sort_unstable();
        let count = names.len();
        names.dedup();
        assert_eq!(names.len(), count);
    }
}
