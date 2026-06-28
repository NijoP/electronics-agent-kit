//! State-machine framework (mechanism) + execution engine (`docs/core/*`).
//!
//! The framework is the reusable mechanism; concrete phase machines are *instances* (P7).
//! The execution engine drives a machine to a terminal state, recording one phase event
//! per transition (P5) and bounding the step count (no silent infinite loops, P13).

use crate::protocol::AgentContext;
use eak_ports::Event;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateKind {
    Initial,
    Normal,
    Waiting,
    TerminalSuccess,
    TerminalFailure,
}

/// The result of advancing a machine one transition.
#[derive(Debug, Clone, PartialEq)]
pub enum StepResult {
    Continue(String),
    Done,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MachineError {
    Internal(String),
}
impl std::fmt::Display for MachineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MachineError::Internal(m) => write!(f, "machine error: {m}"),
        }
    }
}
impl std::error::Error for MachineError {}

/// A phase state machine instance conforming to the framework. Its `step` performs the
/// current state's work (via the context) and returns the deterministic next state.
pub trait Machine {
    fn name(&self) -> &str;
    fn initial(&self) -> String;
    fn step(&mut self, state: &str, ctx: &mut dyn AgentContext)
        -> Result<StepResult, MachineError>;
}

#[derive(Debug, Clone, PartialEq)]
pub enum PhaseOutcome {
    Success,
    Failed(String),
}

/// Drives one machine to a terminal state.
pub struct ExecutionEngine {
    pub max_steps: u32,
}
impl ExecutionEngine {
    pub fn new() -> Self {
        Self { max_steps: 64 }
    }

    pub fn run(&self, machine: &mut dyn Machine, ctx: &mut dyn AgentContext) -> PhaseOutcome {
        let phase = machine.name().to_string();
        let mut state = machine.initial();
        if let Err(e) = ctx.emit(vec![Event::PhaseEntered {
            phase: phase.clone(),
            state: state.clone(),
        }]) {
            return PhaseOutcome::Failed(format!("commit failed: {e}"));
        }

        for _ in 0..self.max_steps {
            match machine.step(&state, &mut *ctx) {
                Ok(StepResult::Continue(next)) => {
                    if let Err(e) = ctx.emit(vec![Event::PhaseStateChanged {
                        phase: phase.clone(),
                        from: state.clone(),
                        to: next.clone(),
                    }]) {
                        return PhaseOutcome::Failed(format!("commit failed: {e}"));
                    }
                    state = next;
                }
                Ok(StepResult::Done) => {
                    let _ = ctx.emit(vec![Event::PhaseCompleted {
                        phase: phase.clone(),
                        outcome: "success".into(),
                    }]);
                    return PhaseOutcome::Success;
                }
                Ok(StepResult::Failed(reason)) => {
                    let _ = ctx.emit(vec![Event::PhaseFailed {
                        phase: phase.clone(),
                        reason: reason.clone(),
                    }]);
                    return PhaseOutcome::Failed(reason);
                }
                Err(e) => {
                    let reason = e.to_string();
                    let _ = ctx.emit(vec![Event::PhaseFailed {
                        phase: phase.clone(),
                        reason: reason.clone(),
                    }]);
                    return PhaseOutcome::Failed(reason);
                }
            }
        }

        let reason = "step budget exceeded".to_string();
        let _ = ctx.emit(vec![Event::PhaseFailed {
            phase,
            reason: reason.clone(),
        }]);
        PhaseOutcome::Failed(reason)
    }
}
impl Default for ExecutionEngine {
    fn default() -> Self {
        Self::new()
    }
}
