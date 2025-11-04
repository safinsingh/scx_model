# scx_model

`scx_model` operates on 3 layers of abstraction:

1. `trait Scheduler` defines the methods any BPF scheduler must implement. Each method is given a reference to `KernelCtx`, which exposes a subset of kernel resources and APIs, such as DSQ creation and idle CPU identification.
2. `SchedCore` holds a concrete `Scheduler` implementation and its corresponding `KernelCtx`. `SchedCore` receives ticks, performing scheduling action as needed by consulting its `Scheduler` and mutating CPU state accordingly. Each `SchedCore::tick` returns a list of completed tasks. 
3.  `Sim` (`src/sim/driver.rs`) drives forward progress in `SchedCore` and injects a list of high-level `Job`s with a `start_time` and `run_time` into `SchedCore` as new task wakeups when appropriate. `Sim` is informed of `Job` completions by `SchedCore`.