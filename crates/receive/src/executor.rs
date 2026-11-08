//! Fallible execution of validated plans, with terminal failure/cancellation.
//!
//! The initial A33 check and repeated caller interlock are NEW integration
//! policy, not a verbatim stock transition. They cannot prove RF inactivity.
//! Cancellation only prevents later configuration requests: it does not undo
//! partial writes, stop a receiver, switch gates off, or restore prior state.

use crate::{Chip, FrequencyPlan, Operation, Plan};

/// Bounded synchronous access, exclusively owned for the execution lifetime.
///
/// Every method must return in a separately established finite bound without
/// panic. An error may follow a partially applied operation. The caller checks
/// its actual board/PTT/ownership/preparation interlock here; register A33 alone
/// cannot establish that the PA, supplies, or other transmitter paths are off.
/// No IRQ, DMA or other owner may alter these registers/pins during an operation.
/// The precondition check is observational/interlock-only: it MUST NOT change
/// RF/relay registers or pin ownership, including after a fresh route read.
pub trait Io {
    type Error;
    fn check_receive_preconditions(&mut self) -> Result<(), Self::Error>;
    fn read(&mut self, chip: Chip, register: u8) -> Result<u16, Self::Error>;
    fn write(&mut self, chip: Chip, register: u8, value: u16) -> Result<(), Self::Error>;
    fn set_antenna_pf0(&mut self, high: bool) -> Result<(), Self::Error>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Status {
    Idle,
    Running,
    /// All requested calls succeeded. This is NOT settled/locked/RF-ready.
    Configured,
    /// Terminal; no later call through this executor is permitted.
    Cancelled,
    /// Terminal; the original error and progress remain available.
    Faulted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Progress {
    pub status: Status,
    /// Fully successful plan operations. A failed operation may be partial.
    pub completed_operations: usize,
    pub total_operations: usize,
    pub initial_tx_check_completed: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StartError {
    AlreadyRunning,
    Terminal(Status),
}

/// Exact action that failed or detected a rejected TX-gate state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Attempt {
    InitialGuard,
    InitialTxRead,
    OperationGuard {
        index: usize,
    },
    Write {
        index: usize,
        chip: Chip,
        register: u8,
    },
    RouteRead {
        index: usize,
    },
    RouteWriteGuard {
        index: usize,
    },
    RouteWrite {
        index: usize,
    },
    Antenna {
        index: usize,
    },
}

#[derive(Debug, Eq, PartialEq)]
pub enum Failure<E> {
    Io(E),
    TxGatesEnabled {
        observed_reg33: u16,
    },
    /// A fixed dependency's route layout differed from its reviewed contract.
    UnexpectedRouteRequest,
}

#[derive(Debug, Eq, PartialEq)]
pub struct Fault<E> {
    pub attempt: Attempt,
    pub reason: Failure<E>,
    pub progress: Progress,
}

enum Program {
    Primary(Plan),
    Frequency(FrequencyPlan),
}
impl Program {
    fn operations(&self) -> &[Operation] {
        match self {
            Self::Primary(plan) => plan.operations(),
            Self::Frequency(plan) => plan.operations(),
        }
    }
}

pub struct Executor<I: Io> {
    io: I,
    program: Option<Program>,
    status: Status,
    completed: usize,
    initial_checked: bool,
    fault: Option<Fault<I::Error>>,
}

impl<I: Io> Executor<I> {
    /// Inert construction. Does not qualify, inspect, or configure hardware.
    pub const fn new(io: I) -> Self {
        Self {
            io,
            program: None,
            status: Status::Idle,
            completed: 0,
            initial_checked: false,
            fault: None,
        }
    }

    pub const fn status(&self) -> Status {
        self.status
    }

    pub fn progress(&self) -> Progress {
        Progress {
            status: self.status,
            completed_operations: self.completed,
            total_operations: self.program.as_ref().map_or(0, |p| p.operations().len()),
            initial_tx_check_completed: self.initial_checked,
        }
    }

    pub fn fault(&self) -> Option<&Fault<I::Error>> {
        self.fault.as_ref()
    }

    // Only the owning runtime may compose further bounded RX requests or a
    // terminal external-mute cleanup. No public escape/recovery API is exposed.
    pub(crate) fn runtime_io(&mut self) -> &mut I {
        &mut self.io
    }

    fn start_program(&mut self, program: Program) -> Result<(), StartError> {
        match self.status {
            Status::Running => return Err(StartError::AlreadyRunning),
            Status::Cancelled | Status::Faulted => return Err(StartError::Terminal(self.status)),
            Status::Idle | Status::Configured => {}
        }
        self.program = Some(program);
        self.completed = 0;
        self.initial_checked = false;
        self.status = Status::Running;
        Ok(())
    }

    /// Explicit retuning is allowed after Configured, with a NEW initial check.
    /// Fault/Cancelled never restart. Accepts no caller-created operation slice.
    pub fn start(&mut self, plan: Plan) -> Result<(), StartError> {
        self.start_program(Program::Primary(plan))
    }

    /// Both A and B frequency-only plans require the same initial A33 guard.
    /// This is conservative integration policy, not stock secondary behavior.
    pub fn start_frequency(&mut self, plan: FrequencyPlan) -> Result<(), StartError> {
        self.start_program(Program::Frequency(plan))
    }

    /// Stop later requests without any cleanup I/O or claim of physical off.
    /// A pre-existing fault remains the terminal state and is not overwritten.
    pub fn cancel(&mut self) -> Progress {
        if self.status != Status::Faulted {
            self.status = Status::Cancelled;
        }
        self.progress()
    }

    /// First step: caller interlock and fresh A33 TX-bit check, with no writes.
    /// Later steps: at most ONE plan operation. A route operation performs its
    /// own fresh read and a second caller interlock immediately before writing.
    /// No retries or guessed shutdown writes follow any failure.
    pub fn step(&mut self) -> Result<Progress, &Fault<I::Error>> {
        if self.status == Status::Running {
            if let Err((attempt, reason)) = self.run_step() {
                self.status = Status::Faulted;
                self.fault = Some(Fault {
                    attempt,
                    reason,
                    progress: self.progress(),
                });
            }
        }
        match self.fault.as_ref() {
            Some(fault) => Err(fault),
            None => Ok(self.progress()),
        }
    }

    fn run_step(&mut self) -> Result<(), (Attempt, Failure<I::Error>)> {
        if !self.initial_checked {
            self.io
                .check_receive_preconditions()
                .map_err(|e| (Attempt::InitialGuard, Failure::Io(e)))?;
            let word = self
                .io
                .read(Chip::A, 0x33)
                .map_err(|e| (Attempt::InitialTxRead, Failure::Io(e)))?;
            if word & 0x000c != 0 {
                return Err((
                    Attempt::InitialTxRead,
                    Failure::TxGatesEnabled {
                        observed_reg33: word,
                    },
                ));
            }
            self.initial_checked = true;
            return Ok(());
        }

        let index = self.completed;
        let operation = self
            .program
            .as_ref()
            .and_then(|p| p.operations().get(index))
            .copied();
        let Some(operation) = operation else {
            self.status = Status::Configured;
            return Ok(());
        };
        self.io
            .check_receive_preconditions()
            .map_err(|e| (Attempt::OperationGuard { index }, Failure::Io(e)))?;
        match operation {
            Operation::Write {
                chip,
                register,
                value,
            } => self.io.write(chip, register, value).map_err(|e| {
                (
                    Attempt::Write {
                        index,
                        chip,
                        register,
                    },
                    Failure::Io(e),
                )
            })?,
            Operation::PrimaryRoute { route } => {
                let previous = self
                    .io
                    .read(Chip::A, 0x33)
                    .map_err(|e| (Attempt::RouteRead { index }, Failure::Io(e)))?;
                if previous & 0x000c != 0 {
                    return Err((
                        Attempt::RouteRead { index },
                        Failure::TxGatesEnabled {
                            observed_reg33: previous,
                        },
                    ));
                }
                let requests = uv98_band_plan::stock_receive_requests(route, previous);
                let uv98_band_plan::Request::PrimaryGpioRegister { value } = requests[0] else {
                    return Err((
                        Attempt::RouteRead { index },
                        Failure::UnexpectedRouteRequest,
                    ));
                };
                self.io
                    .check_receive_preconditions()
                    .map_err(|e| (Attempt::RouteWriteGuard { index }, Failure::Io(e)))?;
                self.io
                    .write(Chip::A, 0x33, value)
                    .map_err(|e| (Attempt::RouteWrite { index }, Failure::Io(e)))?;
            }
            Operation::AntennaPf0 { high } => self
                .io
                .set_antenna_pf0(high)
                .map_err(|e| (Attempt::Antenna { index }, Failure::Io(e)))?,
        }
        self.completed += 1;
        if self
            .program
            .as_ref()
            .is_some_and(|p| self.completed == p.operations().len())
        {
            self.status = Status::Configured;
        }
        Ok(())
    }
}
