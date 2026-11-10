//! The concurrency rules: the Rust twins of `cpp/src/rules/concurrency/`, one file per rule,
//! tested by `testdata/rules/<area>/<ID>/`.

use crate::profile::*;
use crate::{Check, Rule, Severity};

pub(crate) mod blocking_assignment_to_interface;
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
        Rule { id: "SV-CB-001", name: "ClockingBlockOutputReadSkew", category: "Concurrency", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Disallows reading output signals of a clocking block. Clocking block outputs are drive-only; reading them samples post-skew driven values with simulation race conditions.", check: Check::Syntax(clocking_block_output_read_skew::clocking_output_read),
        },
        Rule { id: "SV-CNC-002", name: "CircularThreadWaitDeadlock", category: "Concurrency", severity: Severity::Error, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Detects circular lock acquisition orders and wait-for dependency cycles across concurrent threads in fork..join blocks, which cause guaranteed or probabilistic simulation deadlocks.", check: Check::Syntax(circular_thread_wait_deadlock::circular_wait),
        },
        Rule { id: "SV-CNC-003", name: "PollingLoop", category: "Concurrency", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Flags loops that test a flag or signal and wait a constant #delay between tests, instead of wait(), @ or an event (busy-wait smell, Sonar S2925).", check: Check::Syntax(polling_loop::polling_loop) },
        Rule { id: "SV-CNC-004", name: "SharedWriteFromForks", category: "Concurrency", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Flags class properties written in two or more branches of one fork, or in a join_none thread and in its parent, without a semaphore (SpotBugs IS2_INCONSISTENT_SYNC).", check: Check::Syntax(shared_write_from_forks::shared_write),
        },
        Rule { id: "SV-CON-004", name: "ConstantWaitCondition", category: "Concurrency", severity: Severity::Error, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Flags 'wait()' statements with compile-time constant conditions (e.g., 'wait(0);', 'wait(1);'). 'wait(0);' deadlocks the simulation thread permanently, while 'wait(1);' executes instantaneously.", check: Check::Syntax(constant_wait_condition::constant_wait),
        },
        Rule { id: "SV-CON-005", name: "SemaphoreZeroKey", category: "Concurrency", severity: Severity::Error, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Flags semaphore method calls (sem.get(0), sem.put(0), sem.try_get(0)) passing non-positive key counts (<= 0).", check: Check::Syntax(semaphore_zero_key::semaphore_zero_key),
        },
        Rule { id: "SV-CON-006", name: "EventTriggerInAlwaysComb", category: "Concurrency", severity: Severity::Error, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Disallows event triggers ('->' or '->>') inside always_comb blocks (IEEE 1800 § 9.2.2.2).", check: Check::Syntax(event_trigger_in_always_comb::trigger_in_comb),
        },
        Rule { id: "SV-CON-007", name: "UnboundedMailboxInstantiation", category: "Concurrency", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Disallows instantiating unbounded mailboxes without a capacity limit to prevent memory bloat and missed producer-consumer synchronization.", check: Check::Syntax(unbounded_mailbox_instantiation::unbounded_mailbox),
        },
        Rule { id: "SV-CON-008", name: "MutexTryGetWithoutCheck", category: "Concurrency", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Flags non-blocking semaphore or mutex try_get() calls whose return status is ignored, which risks threads executing critical sections without holding the lock.", check: Check::Syntax(mutex_try_get_without_check::try_get_unchecked),
        },
        Rule { id: "SV-CON-009", name: "WaitForkWithoutFork", category: "Concurrency", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Flags 'wait fork;' statements in tasks or routines that contain no 'fork' blocks. Calling wait fork without spawning child processes can cause unintended hangs or dead code.", check: Check::Syntax(wait_fork_without_fork::wait_fork_without_fork),
        },
        Rule { id: "SV-CON-010", name: "DisableForkWithoutScope", category: "Concurrency", severity: Severity::Error, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Disallows calling 'disable fork;' directly at task or routine scope without an enclosing isolating 'fork ... join' block.", check: Check::Syntax(disable_fork_without_scope::disable_fork),
        },
        Rule { id: "SV-CON-011", name: "WaitModifiedInLoop", category: "Concurrency", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Flags wait(...) statements inside loops where condition variables are never modified or assigned inside the loop body.", check: Check::Syntax(wait_modified_in_loop::wait_in_loop),
        },
        Rule { id: "SV-CON-012", name: "ForkLoopVariableCapture", category: "Concurrency", severity: Severity::Error, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Disallows referencing loop variables directly inside 'fork ... join_none' or 'join_any' blocks without an automatic local variable copy.", check: Check::Syntax(fork_loop_variable_capture::fork_loop_capture),
        },
        Rule { id: "SV-CON-013", name: "BlockingEventTriggerInClockedBlock", category: "Concurrency", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Disallows blocking event triggers (-> ev) inside clocked blocks; non-blocking triggers (->> ev) should be used instead.", check: Check::Syntax(blocking_event_trigger_in_clocked_block::blocking_trigger),
        },
        Rule { id: "SV-CON-014", name: "DisableNamedBlockInClass", category: "Concurrency", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Flags 'disable <block>' and 'disable <task>' inside class methods. A named-block or task disable acts on every running copy, so stopping one agent's thread also stops the same thread in all other instances. 'disable fork' is not flagged.", check: Check::Syntax(disable_named_block_in_class::disable_in_class),
        },
        Rule { id: "SV-CON-015", name: "UnguardedBlockingWait", category: "Concurrency", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: true, native: false, about: "Flags wait(vif.sig) and @(vif.sig) in drivers and sequences that are not raced against a timeout in fork ... join_any. Clock, reset and clocking-block events are not flagged, nor are tasks whose name says timeout or watchdog. Experimental: a watchdog elsewhere in the testbench may already cover the wait.", check: Check::Syntax(unguarded_blocking_wait::unguarded_wait),
        },
        Rule { id: "SV-DLY-003", name: "RepeatLoopWithoutTiming", category: "Concurrency", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Flags repeat loops inside testbench tasks that contain no time-consuming statements (#, @, wait). Zero-delay repeat loops in tasks spin instantaneously in a single delta cycle, often indicating missing clock synchronization.", check: Check::Syntax(repeat_loop_without_timing::repeat_without_timing),
        },
        Rule { id: "SV-DLY-004", name: "ZeroTimeForeverLoop", category: "Concurrency", severity: Severity::Error, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Flags forever loops whose body has no timing control, no task call and no exit, so the loop spins in zero simulation time and hangs the simulator.", check: Check::Syntax(zero_time_forever_loop::zero_time_forever),
        },
        Rule { id: "SV-DRV-001", name: "DrivingDuringReset", category: "RTL_TB_Bridge", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Disallow driving virtual interface control signals without an active reset check or synchronization guard. Asserting handshake signals during reset violates physical bus protocols and corrupts hardware state.", check: Check::Syntax(driving_during_reset::driving_during_reset),
        },
        Rule { id: "SV-EVT-001", name: "ZeroDelayEventWaitLoop", category: "Concurrency", severity: Severity::Error, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Using wait(event.triggered) inside a loop without an intervening time-advancing delay (@ or #) causes a zero-delay infinite delta loop that hangs the simulator at 100% CPU.", check: Check::Syntax(zero_delay_event_wait_loop::triggered_wait_loop),
        },
        Rule { id: "SV-FOR-001", name: "ForkInsideFunction", category: "Concurrency", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Disallows spawning concurrent threads with 'fork ... join/join_any/join_none' inside functions. Functions must remain synchronous and instantaneous per IEEE 1800 § 13.4.4.", check: Check::Syntax(fork_inside_function::fork_in_function),
        },
        Rule { id: "SV-INT-001", name: "BlockingAssignmentToInterface", category: "Concurrency", severity: Severity::Error, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Disallows blocking assignments ('=') when driving virtual interface or clocking block signals. Clocking blocks and synchronous interface signals must be driven with non-blocking drives ('<=') per IEEE 1800 § 14.16.", check: Check::Syntax(blocking_assignment_to_interface::blocking_to_interface),
        },
        Rule { id: "SV-IPC-001", name: "TryPutIgnoredReturn", category: "Concurrency", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &["MG-8.1"], experimental: false, native: false, about: "Non-blocking IPC routines (try_put, try_get, try_peek) return a boolean status. Ignoring the return value silently drops transactions when queues are full or empty.", check: Check::Syntax(try_put_ignored_return::try_put_ignored),
        },
        Rule { id: "SV-RCE-001", name: "ZeroDelayEventTrigger", category: "Concurrency", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Flags blocking event triggers ('-> event') which cause zero-delay simulation race conditions. Recommends non-blocking event triggers ('->> event') or persistent event evaluation ('wait(event.triggered)').", check: Check::Syntax(zero_delay_event_trigger::zero_delay_event_trigger),
        },
        Rule { id: "SV-RSC-001", name: "LockNotReleased", category: "Concurrency", severity: Severity::Error, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Flags semaphore get()/try_get() and sequencer lock()/grab() that some path leaves without the matching put()/unlock()/ungrab(), such as an early return (Sonar S2222).", check: Check::Syntax(lock_not_released::lock_not_released) },
        Rule { id: "SV-RSC-002", name: "FileHandleNotClosed", category: "Correctness", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Flags $fopen calls in classes that never call $fclose, which loses buffered trace data and exhausts file handles in long regressions (Sonar S2095).", check: Check::Syntax(file_handle_not_closed::file_not_closed),
        },
        Rule { id: "SV-THR-001", name: "ForkLoopVariableShared", category: "Concurrency", severity: Severity::Error, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Spawning background threads with fork..join_none inside loops without copying the loop variable to an automatic local variable causes all threads to see the final loop index.", check: Check::Syntax(fork_loop_variable_shared::fork_loop_variable),
        },
        Rule { id: "SV-THR-002", name: "StaticRoutineVariableLifetime", category: "Concurrency", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "A task outside a class has static lifetime by default: its local variables are shared by every call that is running, so calls that overlap in time overwrite each other's values. Functions return in zero time and are not reported.", check: Check::Syntax(static_routine_variable_lifetime::static_routine_variable),
        },
        Rule { id: "SV-THR-003", name: "UnboundedThreadSpawn", category: "Concurrency", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Flags fork ... join_none inside forever or while loops with no wait fork, disable fork or semaphore get in the loop to bound outstanding threads (thread-leak checks).", check: Check::Syntax(unbounded_thread_spawn::unbounded_spawn),
        },
    ]
}
