//! The concurrency rules: the Rust twins of `cpp/src/rules/concurrency/`, one file per rule,
//! tested by `testdata/rules/<area>/<ID>/`.

use crate::{Check, Rule};

mod blocking_assignment_to_interface;
mod blocking_event_trigger_in_clocked_block;
mod circular_thread_wait_deadlock;
mod clocking_block_output_read_skew;
mod constant_wait_condition;
mod disable_fork_without_scope;
mod disable_named_block_in_class;
mod driving_during_reset;
mod event_trigger_in_always_comb;
mod file_handle_not_closed;
mod fork_inside_function;
mod fork_loop_variable_capture;
mod fork_loop_variable_shared;
mod lock_not_released;
mod mutex_try_get_without_check;
mod polling_loop;
mod repeat_loop_without_timing;
mod semaphore_zero_key;
mod shared_write_from_forks;
mod static_routine_variable_lifetime;
mod try_put_ignored_return;
mod unbounded_mailbox_instantiation;
mod unbounded_thread_spawn;
mod unguarded_blocking_wait;
mod wait_fork_without_fork;
mod wait_modified_in_loop;
mod zero_delay_event_trigger;
mod zero_delay_event_wait_loop;
mod zero_time_forever_loop;

/// Every rule of this category.
pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule {
            id: "SV-CB-001",
            name: "ClockingBlockOutputReadSkew",
            check: Check::Syntax(clocking_block_output_read_skew::clocking_output_read),
        },
        Rule {
            id: "SV-CNC-002",
            name: "CircularThreadWaitDeadlock",
            check: Check::Syntax(circular_thread_wait_deadlock::circular_wait),
        },
        Rule { id: "SV-CNC-003", name: "PollingLoop", check: Check::Syntax(polling_loop::polling_loop) },
        Rule {
            id: "SV-CNC-004",
            name: "SharedWriteFromForks",
            check: Check::Syntax(shared_write_from_forks::shared_write),
        },
        Rule {
            id: "SV-CON-004",
            name: "ConstantWaitCondition",
            check: Check::Syntax(constant_wait_condition::constant_wait),
        },
        Rule {
            id: "SV-CON-005",
            name: "SemaphoreZeroKey",
            check: Check::Syntax(semaphore_zero_key::semaphore_zero_key),
        },
        Rule {
            id: "SV-CON-006",
            name: "EventTriggerInAlwaysComb",
            check: Check::Syntax(event_trigger_in_always_comb::trigger_in_comb),
        },
        Rule {
            id: "SV-CON-007",
            name: "UnboundedMailboxInstantiation",
            check: Check::Syntax(unbounded_mailbox_instantiation::unbounded_mailbox),
        },
        Rule {
            id: "SV-CON-008",
            name: "MutexTryGetWithoutCheck",
            check: Check::Syntax(mutex_try_get_without_check::try_get_unchecked),
        },
        Rule {
            id: "SV-CON-009",
            name: "WaitForkWithoutFork",
            check: Check::Syntax(wait_fork_without_fork::wait_fork_without_fork),
        },
        Rule {
            id: "SV-CON-010",
            name: "DisableForkWithoutScope",
            check: Check::Syntax(disable_fork_without_scope::disable_fork),
        },
        Rule {
            id: "SV-CON-011",
            name: "WaitModifiedInLoop",
            check: Check::Syntax(wait_modified_in_loop::wait_in_loop),
        },
        Rule {
            id: "SV-CON-012",
            name: "ForkLoopVariableCapture",
            check: Check::Syntax(fork_loop_variable_capture::fork_loop_capture),
        },
        Rule {
            id: "SV-CON-013",
            name: "BlockingEventTriggerInClockedBlock",
            check: Check::Syntax(blocking_event_trigger_in_clocked_block::blocking_trigger),
        },
        Rule {
            id: "SV-CON-014",
            name: "DisableNamedBlockInClass",
            check: Check::Syntax(disable_named_block_in_class::disable_in_class),
        },
        Rule {
            id: "SV-CON-015",
            name: "UnguardedBlockingWait",
            check: Check::Syntax(unguarded_blocking_wait::unguarded_wait),
        },
        Rule {
            id: "SV-DLY-003",
            name: "RepeatLoopWithoutTiming",
            check: Check::Syntax(repeat_loop_without_timing::repeat_without_timing),
        },
        Rule {
            id: "SV-DLY-004",
            name: "ZeroTimeForeverLoop",
            check: Check::Syntax(zero_time_forever_loop::zero_time_forever),
        },
        Rule {
            id: "SV-DRV-001",
            name: "DrivingDuringReset",
            check: Check::Syntax(driving_during_reset::driving_during_reset),
        },
        Rule {
            id: "SV-EVT-001",
            name: "ZeroDelayEventWaitLoop",
            check: Check::Syntax(zero_delay_event_wait_loop::triggered_wait_loop),
        },
        Rule {
            id: "SV-FOR-001",
            name: "ForkInsideFunction",
            check: Check::Syntax(fork_inside_function::fork_in_function),
        },
        Rule {
            id: "SV-INT-001",
            name: "BlockingAssignmentToInterface",
            check: Check::Syntax(blocking_assignment_to_interface::blocking_to_interface),
        },
        Rule {
            id: "SV-IPC-001",
            name: "TryPutIgnoredReturn",
            check: Check::Syntax(try_put_ignored_return::try_put_ignored),
        },
        Rule {
            id: "SV-RCE-001",
            name: "ZeroDelayEventTrigger",
            check: Check::Syntax(zero_delay_event_trigger::zero_delay_event_trigger),
        },
        Rule { id: "SV-RSC-001", name: "LockNotReleased", check: Check::Syntax(lock_not_released::lock_not_released) },
        Rule {
            id: "SV-RSC-002",
            name: "FileHandleNotClosed",
            check: Check::Syntax(file_handle_not_closed::file_not_closed),
        },
        Rule {
            id: "SV-THR-001",
            name: "ForkLoopVariableShared",
            check: Check::Syntax(fork_loop_variable_shared::fork_loop_variable),
        },
        Rule {
            id: "SV-THR-002",
            name: "StaticRoutineVariableLifetime",
            check: Check::Syntax(static_routine_variable_lifetime::static_routine_variable),
        },
        Rule {
            id: "SV-THR-003",
            name: "UnboundedThreadSpawn",
            check: Check::Syntax(unbounded_thread_spawn::unbounded_spawn),
        },
    ]
}
