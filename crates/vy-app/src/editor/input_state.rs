//! Pending operator input and the Insert session.
//!
//! An operator command (`d`, `y`) spans more than one key press. [`PendingInput`]
//! holds the only state for that, so an operator cannot be half-set and the
//! register named before it survives until the operator completes.
//!
//! See `docs/proposals/0005-logic-state-refactor.md` (Phase B).

use super::motion::combined_count;
use super::operator::{Operator, TextObjectKind};
use super::repeat::InsertKind;

/// The operator command that is waiting for the rest of its input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum PendingInput {
    #[default]
    Idle,
    /// `d` or `y` waiting for a motion, a text object, or a repeat of itself.
    Operator {
        operator: Operator,
        count: Option<u32>,
        register: Option<usize>,
    },
    /// `di` or `da` waiting for the object key.
    TextObject {
        operator: Operator,
        count: Option<u32>,
        register: Option<usize>,
        kind: TextObjectKind,
    },
}

/// An operator command waiting for its motion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PendingOperator {
    pub(super) operator: Operator,
    pub(super) count: Option<u32>,
    pub(super) register: Option<usize>,
}

/// A text object waiting for its object key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PendingTextObject {
    pub(super) operator: Operator,
    pub(super) count: Option<u32>,
    pub(super) register: Option<usize>,
    pub(super) kind: TextObjectKind,
}

impl PendingInput {
    pub(super) fn operator_pending(&self) -> bool {
        self.operator().is_some()
    }

    pub(super) fn text_object_pending(&self) -> bool {
        matches!(self, Self::TextObject { .. })
    }

    /// The waiting operator, whichever kind of input it waits for.
    pub(super) fn operator(&self) -> Option<PendingOperator> {
        match *self {
            Self::Idle => None,
            Self::Operator {
                operator,
                count,
                register,
            }
            | Self::TextObject {
                operator,
                count,
                register,
                ..
            } => Some(PendingOperator {
                operator,
                count,
                register,
            }),
        }
    }

    /// Starts `operator`.
    ///
    /// Returns the combined count and the register of a repeated operator
    /// (`dd`, `yy`) when the same operator was already waiting.
    pub(super) fn start_operator(
        &mut self,
        operator: Operator,
        count: Option<u32>,
        register: Option<usize>,
    ) -> Option<(u32, Option<usize>)> {
        if let Self::Operator {
            operator: pending,
            count: pending_count,
            register: pending_register,
        } = *self
            && pending == operator
        {
            self.clear();
            return Some((
                combined_count(pending_count, count).unwrap_or(1),
                pending_register.or(register),
            ));
        }

        *self = Self::Operator {
            operator,
            count,
            register,
        };
        None
    }

    /// Moves to a text object when an operator is waiting.
    pub(super) fn begin_text_object(&mut self, kind: TextObjectKind) {
        if let Self::Operator {
            operator,
            count,
            register,
        } = *self
        {
            *self = Self::TextObject {
                operator,
                count,
                register,
                kind,
            };
        }
    }

    /// Takes the waiting text object and clears the state.
    pub(super) fn take_text_object(&mut self) -> Option<PendingTextObject> {
        match std::mem::take(self) {
            Self::TextObject {
                operator,
                count,
                register,
                kind,
            } => Some(PendingTextObject {
                operator,
                count,
                register,
                kind,
            }),
            other => {
                *self = other;
                None
            }
        }
    }

    pub(super) fn clear(&mut self) {
        *self = Self::Idle;
    }
}

/// Text typed since an Insert session began, and how the session started.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct InsertSession {
    pub(super) kind: InsertKind,
    pub(super) buffer: String,
}

impl InsertSession {
    pub(super) fn begin(&mut self, kind: InsertKind) {
        self.kind = kind;
        self.buffer.clear();
    }

    pub(super) fn clear(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pending_operator_keeps_its_register() {
        let mut pending = PendingInput::Idle;
        assert_eq!(
            pending.start_operator(Operator::Delete, None, Some(3)),
            None
        );

        let waiting = pending.operator().expect("operator");
        assert_eq!(waiting.operator, Operator::Delete);
        assert_eq!(waiting.register, Some(3));
        assert!(pending.operator_pending());
        assert!(!pending.text_object_pending());
    }

    #[test]
    fn a_repeated_operator_merges_counts_and_clears() {
        let mut pending = PendingInput::Idle;
        pending.start_operator(Operator::Delete, Some(2), Some(1));

        let (times, register) = pending
            .start_operator(Operator::Delete, Some(3), None)
            .expect("a repeated operator");
        assert_eq!(times, 6);
        assert_eq!(register, Some(1));
        assert_eq!(pending, PendingInput::Idle);
    }

    #[test]
    fn text_objects_keep_the_operator_and_register() {
        let mut pending = PendingInput::Idle;
        pending.start_operator(Operator::Yank, Some(2), Some(5));
        pending.begin_text_object(TextObjectKind::Inner);

        assert!(pending.text_object_pending());
        let object = pending.take_text_object().expect("text object");
        assert_eq!(object.operator, Operator::Yank);
        assert_eq!(object.count, Some(2));
        assert_eq!(object.register, Some(5));
        assert_eq!(object.kind, TextObjectKind::Inner);
        assert_eq!(pending, PendingInput::Idle);
    }

    #[test]
    fn text_objects_need_an_operator() {
        let mut pending = PendingInput::Idle;
        pending.begin_text_object(TextObjectKind::Around);
        assert_eq!(pending, PendingInput::Idle);
        assert!(pending.take_text_object().is_none());
    }

    #[test]
    fn the_insert_session_tracks_how_it_started() {
        let mut session = InsertSession::default();
        session.begin(InsertKind::OpenBelow);
        session.buffer.push_str("text");

        assert_eq!(session.kind, InsertKind::OpenBelow);
        assert_eq!(session.buffer, "text");

        session.clear();
        assert_eq!(session, InsertSession::default());
    }
}
