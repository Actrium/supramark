/// Timing diagram IR
/// Participant type: robust (thick state band) or concise (thin state line)
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub enum TimingParticipantKind {
    Robust,
    Concise,
}

/// A participant (signal/entity) in the timing diagram
#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct TimingParticipant {
    /// Display name (from quoted string)
    pub name: String,
    /// Alias used in references
    pub alias: Option<String>,
    /// Robust or concise
    pub kind: TimingParticipantKind,
}

impl TimingParticipant {
    /// Return the identifier used for referencing (alias if set, else name)
    pub fn id(&self) -> &str {
        self.alias.as_deref().unwrap_or(&self.name)
    }
}

/// State change: participant enters a new state at a given (absolute) time
#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct TimingStateChange {
    /// Participant alias/name
    pub participant: String,
    /// Absolute time of the change
    pub time: i64,
    /// New state name
    pub state: String,
}

/// Message arrow between participants
#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct TimingMessage {
    /// Source participant
    pub from: String,
    /// Target participant
    pub to: String,
    /// Label on the arrow
    pub label: String,
    /// Absolute time at source
    pub from_time: i64,
    /// Absolute time at target (may differ for `TO@+offset` syntax)
    pub to_time: i64,
}

/// Constraint / measurement annotation between two time points on a participant
#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct TimingConstraint {
    /// Participant this constraint belongs to
    pub participant: String,
    /// Start time (absolute)
    pub start_time: i64,
    /// End time (absolute)
    pub end_time: i64,
    /// Label text
    pub label: String,
}

/// A note annotation on the timing diagram.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct TimingNote {
    pub text: String,
    pub position: String,
    pub target: Option<String>,
}

/// Complete timing diagram intermediate representation.
///
/// All times are stored as resolved absolute values. The parser resolves
/// relative offsets (`@+100`) during parsing so downstream consumers
/// (layout, render) only deal with absolute timestamps.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct TimingDiagram {
    pub participants: Vec<TimingParticipant>,
    pub state_changes: Vec<TimingStateChange>,
    pub messages: Vec<TimingMessage>,
    pub constraints: Vec<TimingConstraint>,
    pub notes: Vec<TimingNote>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn participant_id_uses_alias_when_set() {
        let p = TimingParticipant {
            name: "DNS Resolver".to_string(),
            alias: Some("DNS".to_string()),
            kind: TimingParticipantKind::Robust,
        };
        assert_eq!(p.id(), "DNS");
    }

    #[test]
    fn participant_id_uses_name_when_no_alias() {
        let p = TimingParticipant {
            name: "DNS Resolver".to_string(),
            alias: None,
            kind: TimingParticipantKind::Robust,
        };
        assert_eq!(p.id(), "DNS Resolver");
    }

    #[test]
    fn timing_diagram_default_construction() {
        let td = TimingDiagram {
            participants: vec![],
            state_changes: vec![],
            messages: vec![],
            constraints: vec![],

            notes: vec![],
        };
        assert!(td.participants.is_empty());
        assert!(td.state_changes.is_empty());
        assert!(td.messages.is_empty());
        assert!(td.constraints.is_empty());
    }

    #[test]
    fn state_change_fields() {
        let sc = TimingStateChange {
            participant: "WB".to_string(),
            time: 100,
            state: "Processing".to_string(),
        };
        assert_eq!(sc.participant, "WB");
        assert_eq!(sc.time, 100);
        assert_eq!(sc.state, "Processing");
    }

    #[test]
    fn timing_message_fields() {
        let msg = TimingMessage {
            from: "WU".to_string(),
            to: "WB".to_string(),
            label: "URL".to_string(),
            from_time: 100,
            to_time: 100,
        };
        assert_eq!(msg.from, "WU");
        assert_eq!(msg.to, "WB");
        assert_eq!(msg.label, "URL");
        assert_eq!(msg.from_time, 100);
        assert_eq!(msg.to_time, 100);
    }

    #[test]
    fn timing_message_with_offset() {
        let msg = TimingMessage {
            from: "WB".to_string(),
            to: "DNS".to_string(),
            label: "Resolve URL".to_string(),
            from_time: 300,
            to_time: 350,
        };
        assert_ne!(msg.from_time, msg.to_time);
    }

    #[test]
    fn timing_constraint_fields() {
        let c = TimingConstraint {
            participant: "WU".to_string(),
            start_time: 200,
            end_time: 350,
            label: "{150 ms}".to_string(),
        };
        assert_eq!(c.participant, "WU");
        assert_eq!(c.end_time - c.start_time, 150);
    }

    #[test]
    fn participant_kind_equality() {
        assert_eq!(TimingParticipantKind::Robust, TimingParticipantKind::Robust);
        assert_ne!(
            TimingParticipantKind::Robust,
            TimingParticipantKind::Concise
        );
    }

    #[test]
    fn clone_timing_diagram() {
        let td = TimingDiagram {
            participants: vec![TimingParticipant {
                name: "A".to_string(),
                alias: Some("a".to_string()),
                kind: TimingParticipantKind::Concise,
            }],
            state_changes: vec![TimingStateChange {
                participant: "a".to_string(),
                time: 0,
                state: "Idle".to_string(),
            }],
            messages: vec![],
            constraints: vec![],
            notes: vec![],
        };
        let cloned = td.clone();
        assert_eq!(cloned.participants.len(), 1);
        assert_eq!(cloned.state_changes.len(), 1);
    }

    #[test]
    fn full_diagram_construction() {
        let td = TimingDiagram {
            participants: vec![
                TimingParticipant {
                    name: "DNS Resolver".to_string(),
                    alias: Some("DNS".to_string()),
                    kind: TimingParticipantKind::Robust,
                },
                TimingParticipant {
                    name: "Web Browser".to_string(),
                    alias: Some("WB".to_string()),
                    kind: TimingParticipantKind::Robust,
                },
                TimingParticipant {
                    name: "Web User".to_string(),
                    alias: Some("WU".to_string()),
                    kind: TimingParticipantKind::Concise,
                },
            ],
            state_changes: vec![
                TimingStateChange {
                    participant: "WU".into(),
                    time: 0,
                    state: "Idle".into(),
                },
                TimingStateChange {
                    participant: "WB".into(),
                    time: 0,
                    state: "Idle".into(),
                },
                TimingStateChange {
                    participant: "DNS".into(),
                    time: 0,
                    state: "Idle".into(),
                },
                TimingStateChange {
                    participant: "WU".into(),
                    time: 100,
                    state: "Waiting".into(),
                },
                TimingStateChange {
                    participant: "WB".into(),
                    time: 100,
                    state: "Processing".into(),
                },
            ],
            messages: vec![TimingMessage {
                from: "WU".into(),
                to: "WB".into(),
                label: "URL".into(),
                from_time: 100,
                to_time: 100,
            }],
            constraints: vec![TimingConstraint {
                participant: "WU".into(),
                start_time: 200,
                end_time: 350,
                label: "{150 ms}".into(),
            }],
            notes: vec![],
        };
        assert_eq!(td.participants.len(), 3);
        assert_eq!(td.state_changes.len(), 5);
        assert_eq!(td.messages.len(), 1);
        assert_eq!(td.constraints.len(), 1);
    }

    #[test]
    fn state_change_clone() {
        let sc = TimingStateChange {
            participant: "A".into(),
            time: 42,
            state: "Running".into(),
        };
        let sc2 = sc.clone();
        assert_eq!(sc2.time, 42);
        assert_eq!(sc2.state, "Running");
    }
}
