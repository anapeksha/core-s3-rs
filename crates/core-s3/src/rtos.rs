//! Single-primary-executor ownership for esp-rtos 0.4 / Embassy 0.10.
//!
//! Start from CPU0 thread mode, then optionally start a synchronous CPU1 worker,
//! then run the primary executor. Do not combine this layer with direct scheduler
//! startup, another executor, or another Embassy time driver. Claims only detect
//! calls through this layer; upstream state is not publicly queryable.
//!
//! esp-rtos may panic internally (including its second-core scheduler timeout,
//! allocation, and context assertions). These panics cannot be converted to typed
//! errors in this no_std layer. The readiness deadline bounds our wait, not the
//! upstream call or an interrupt-disabled/stalled system. Timeout does not cancel
//! the worker: it can enter later, and its stack must remain permanently owned.
//! No arbitrary affinity, per-task stack, priority, or high-water API is provided.

#![deny(unsafe_code)]

use core::sync::atomic::{AtomicBool, Ordering};

/// Conservative wrapper policy, not a guarantee against worker stack overflow.
pub const MIN_WORKER_STACK_BYTES: usize = 4096;
/// Maximum wrapper readiness wait.
pub const MAX_STARTUP_TIMEOUT_US: u32 = 10_000_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RuntimeConfig {
    pub worker_ready_timeout_us: u32,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            worker_ready_timeout_us: 1_000_000,
        }
    }
}

impl RuntimeConfig {
    pub const fn validate(self) -> Result<(), ConfigError> {
        if self.worker_ready_timeout_us == 0
            || self.worker_ready_timeout_us > MAX_STARTUP_TIMEOUT_US
        {
            Err(ConfigError::InvalidStartupTimeout)
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigError {
    InvalidStartupTimeout,
    EmptyStack,
    StackTooSmall,
    StackNotMultipleOf16,
}

/// Validate before constructing an esp-hal `Stack`.
pub const fn validate_worker_stack(bytes: usize) -> Result<(), ConfigError> {
    if bytes == 0 {
        Err(ConfigError::EmptyStack)
    } else if bytes < MIN_WORKER_STACK_BYTES {
        Err(ConfigError::StackTooSmall)
    } else if !bytes.is_multiple_of(16) {
        Err(ConfigError::StackNotMultipleOf16)
    } else {
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeError {
    InvalidConfig(ConfigError),
    AlreadyStarted,
    WrongCore,
    ExecutorAlreadyConsumed,
    ExecutorStorageOccupied,
    WorkerAlreadyStarted,
    WorkerStackStorageOccupied,
    WorkerReadinessTimeout,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskError {
    AlreadyConsumed,
    Busy,
}

struct Claim(AtomicBool);
impl Claim {
    const fn new() -> Self {
        Self(AtomicBool::new(false))
    }

    fn take(&self) -> bool {
        self.0
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }
}

/// One-shot task submission owning its permanent future storage.
/// Declare this in static storage; neither the storage nor its claim is exposed,
/// so downstream code cannot re-wrap it to bypass the one-shot invariant.
/// A failed busy attempt also consumes the task (busy is mapped defensively).
///
/// `F` must be a nameable future type for a conventional static declaration.
/// Anonymous async-function/block futures cannot generally be named on stable
/// Rust. Use a concrete future type, or caller-static `StaticCell` storage with
/// type inference where supported; this API does not erase future types or
/// replace Embassy's task macros.
/// The factory is only called when Embassy can claim the storage. Construction
/// in-place uses Embassy's API; elimination of temporary stack copies depends on
/// compiler optimization, as documented by Embassy.
pub struct StaticTask<F: core::future::Future + 'static> {
    storage: embassy_executor::raw::TaskStorage<F>,
    consumed: Claim,
}

impl<F: core::future::Future + 'static> StaticTask<F> {
    pub const fn new() -> Self {
        Self {
            storage: embassy_executor::raw::TaskStorage::new(),
            consumed: Claim::new(),
        }
    }

    pub fn spawn(
        &'static self,
        spawner: embassy_executor::Spawner,
        factory: impl FnOnce() -> F,
    ) -> Result<(), TaskError> {
        if !self.consumed.take() {
            return Err(TaskError::AlreadyConsumed);
        }
        let token = self.storage.spawn(factory).map_err(|_| TaskError::Busy)?;
        spawner.spawn(token);
        Ok(())
    }
}

impl<F: core::future::Future + 'static> Default for StaticTask<F> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(target_arch = "xtensa")]
mod target {
    use super::*;
    use esp_hal::peripherals::{CPU_CTRL, FROM_CPU_INTR0, FROM_CPU_INTR1};
    use static_cell::StaticCell;

    static STARTED: Claim = Claim::new();
    static WORKER: Claim = Claim::new();
    static EXECUTOR: Claim = Claim::new();

    /// Permanent storage for the single primary executor. The inner cell is
    /// private and cannot be borrowed, extracted, or wrapped a second time.
    pub struct PrimaryExecutorStorage {
        cell: StaticCell<esp_rtos::embassy::Executor>,
    }

    impl PrimaryExecutorStorage {
        pub const fn new() -> Self {
            Self {
                cell: StaticCell::new(),
            }
        }
    }

    impl Default for PrimaryExecutorStorage {
        fn default() -> Self {
            Self::new()
        }
    }

    /// Permanent CPU1 worker stack and its one-shot entry state.
    /// `N` is the stack capacity in bytes, not words; wrapper metadata adds
    /// overhead to the total object size. Startup validates `N` before creating
    /// the HAL stack. The inner storage cannot be extracted or re-wrapped.
    pub struct Cpu1WorkerStack<const N: usize> {
        cell: StaticCell<esp_hal::system::Stack<N>>,
        consumed: Claim,
        ready: AtomicBool,
    }

    impl<const N: usize> Cpu1WorkerStack<N> {
        pub const fn new() -> Self {
            Self {
                cell: StaticCell::new(),
                consumed: Claim::new(),
                ready: AtomicBool::new(false),
            }
        }

        /// Whether the worker wrapper has entered, even if startup timed out.
        /// This does not indicate application initialization or completion.
        pub fn is_ready(&self) -> bool {
            self.ready.load(Ordering::Acquire)
        }
    }

    impl<const N: usize> Default for Cpu1WorkerStack<N> {
        fn default() -> Self {
            Self::new()
        }
    }

    /// Unique scheduler ownership token; not constructible without startup.
    pub struct CoreS3Runtime {
        config: RuntimeConfig,
    }

    impl CoreS3Runtime {
        /// Validate and claim before calling `esp_rtos::start` on CPU0.
        /// Configure the upstream allocator beforehand when required. Direct
        /// upstream startup cannot be detected. Panic leaves the claim consumed.
        pub fn start(
            config: RuntimeConfig,
            timer: impl esp_rtos::TimerSource,
            interrupt: FROM_CPU_INTR0<'static>,
        ) -> Result<Self, RuntimeError> {
            config.validate().map_err(RuntimeError::InvalidConfig)?;
            if esp_hal::system::Cpu::current() != esp_hal::system::Cpu::ProCpu {
                return Err(RuntimeError::WrongCore);
            }
            if !STARTED.take() {
                return Err(RuntimeError::AlreadyStarted);
            }
            esp_rtos::start(timer, interrupt);
            Ok(Self { config })
        }

        /// Claim caller-static storage and run the only primary executor forever.
        /// No executor reference is exposed and no additional time driver is made.
        pub fn run(
            self,
            storage: &'static PrimaryExecutorStorage,
            init: impl FnOnce(embassy_executor::Spawner),
        ) -> Result<core::convert::Infallible, RuntimeError> {
            if esp_hal::system::Cpu::current() != esp_hal::system::Cpu::ProCpu {
                return Err(RuntimeError::WrongCore);
            }
            if !EXECUTOR.take() {
                return Err(RuntimeError::ExecutorAlreadyConsumed);
            }
            let executor = storage
                .cell
                .try_init_with(esp_rtos::embassy::Executor::new)
                .ok_or(RuntimeError::ExecutorStorageOccupied)?;
            executor.run(init)
        }

        /// Start a synchronous CPU1 worker, not an executor. Readiness means the
        /// wrapper entered, not that application initialization has completed.
        /// Errors after claiming do not release ownership or permit retries.
        /// Upstream can panic internally; see module-level timeout limitations.
        pub fn start_worker<const N: usize>(
            &self,
            cpu: CPU_CTRL<'static>,
            interrupt: FROM_CPU_INTR1<'static>,
            storage: &'static Cpu1WorkerStack<N>,
            worker: impl FnOnce() + Send + 'static,
        ) -> Result<(), RuntimeError> {
            validate_worker_stack(N).map_err(RuntimeError::InvalidConfig)?;
            if esp_hal::system::Cpu::current() != esp_hal::system::Cpu::ProCpu {
                return Err(RuntimeError::WrongCore);
            }
            if !WORKER.take() {
                return Err(RuntimeError::WorkerAlreadyStarted);
            }
            if !storage.consumed.take() {
                return Err(RuntimeError::WorkerAlreadyStarted);
            }
            let stack = storage
                .cell
                .try_init_with(esp_hal::system::Stack::new)
                .ok_or(RuntimeError::WorkerStackStorageOccupied)?;
            let start = esp_hal::time::Instant::now();
            esp_rtos::start_second_core(cpu, interrupt, stack, move || {
                storage.ready.store(true, Ordering::Release);
                worker();
            });
            while !storage.is_ready() {
                if start.elapsed().as_micros() >= u64::from(self.config.worker_ready_timeout_us) {
                    return Err(RuntimeError::WorkerReadinessTimeout);
                }
                esp_hal::rom::ets_delay_us(1);
            }
            Ok(())
        }
    }
}

#[cfg(target_arch = "xtensa")]
pub use target::{CoreS3Runtime, Cpu1WorkerStack, PrimaryExecutorStorage};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stack_validation() {
        assert_eq!(validate_worker_stack(0), Err(ConfigError::EmptyStack));
        assert_eq!(validate_worker_stack(16), Err(ConfigError::StackTooSmall));
        assert_eq!(
            validate_worker_stack(4097),
            Err(ConfigError::StackNotMultipleOf16)
        );
        assert_eq!(validate_worker_stack(4096), Ok(()));
        assert_eq!(validate_worker_stack(8192), Ok(()));
    }

    #[test]
    fn timeout_validation() {
        for timeout in [0, MAX_STARTUP_TIMEOUT_US + 1] {
            assert_eq!(
                RuntimeConfig {
                    worker_ready_timeout_us: timeout
                }
                .validate(),
                Err(ConfigError::InvalidStartupTimeout)
            );
        }
        assert_eq!(RuntimeConfig::default().validate(), Ok(()));
        assert_eq!(
            RuntimeConfig {
                worker_ready_timeout_us: MAX_STARTUP_TIMEOUT_US
            }
            .validate(),
            Ok(())
        );
    }

    #[test]
    fn task_owns_its_one_shot_claim() {
        static TASK: StaticTask<core::future::Ready<()>> = StaticTask::new();
        assert!(TASK.consumed.take());
        assert!(!TASK.consumed.take());
        let other = StaticTask::<core::future::Ready<()>>::default();
        assert!(other.consumed.take());
    }

    #[test]
    fn claims_are_permanent() {
        let claim = Claim::new();
        assert!(claim.take());
        assert!(!claim.take());
        assert!(!claim.take());
    }
}
