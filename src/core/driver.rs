use super::{
    observer::Observer,
    state::{CpuId, KernelCtx, TaskId, Ticks},
};
use crate::{
    core::{TaskState, event::SchedCoreEvent},
    scheduler::{
        EnqueueFlags, SCX_ENQ_CPU_SELECTED, SCX_ENQ_REENQ, SCX_ENQ_WAKEUP, Scheduler,
        SelectCpuDecision,
    },
};

pub struct SchedCore<S: Scheduler> {
    pub ctx: KernelCtx,
    pub scheduler: S,
    observer: Observer,
    events: Vec<SchedCoreEvent>,
}

impl<S: Scheduler> SchedCore<S> {
    pub fn new(num_cpus: usize) -> Self {
        let mut ctx = KernelCtx::new(num_cpus);
        let scheduler = S::init(&mut ctx);
        let observer = Observer::new();
        Self {
            ctx,
            scheduler,
            observer,
            events: Vec::new(),
        }
    }

    pub fn tick(&mut self) -> Vec<SchedCoreEvent> {
        for cpu in 0..self.ctx.cpus.len() {
            self.schedule_cpu(cpu);
        }
        for cpu in 0..self.ctx.cpus.len() {
            self.tick_cpu(cpu);
        }
        self.observer.observe(&self.ctx);

        self.ctx.advance_time(1);
        std::mem::take(&mut self.events)
    }

    fn tick_cpu(&mut self, cpu: CpuId) {
        let Some(current_task_id) = self.ctx.cpus[cpu].current else {
            self.events.push(SchedCoreEvent::CpuIdle { cpu });
            return;
        };

        // Increment service/decrement slice for current task
        // THIS IS THE ACTUAL "TIME BETWEEN TICKS"
        // In its own block to avoid double-mutable-borrow
        {
            let task = self.ctx.task_mut(current_task_id);
            task.consumed_service = task.consumed_service.saturating_add(1);
            task.consumed_timeslice = task.consumed_timeslice.saturating_add(1);
        }

        // The following occurs "at the end" of the tick

        // Invoke BPF ops->tick()
        self.scheduler.tick(&mut self.ctx, current_task_id);

        let task = self.ctx.task_mut(current_task_id);
        let completed = task.consumed_service >= task.required_service;
        let slice_expired = task.consumed_timeslice
            == task
                .allocated_timeslice
                .expect("Task has an unset timeslice")
            && !completed;

        if !completed && !slice_expired {
            return;
        }

        // Common "deschedule" path
        self.scheduler
            .stopping(&mut self.ctx, current_task_id, !completed);
        self.ctx.clear_cpu(cpu);
        self.events.push(SchedCoreEvent::CpuCurrentChange {
            cpu,
            from: Some(current_task_id),
            to: None,
        });

        if completed {
            self.ctx.mark_completed(current_task_id, self.ctx.now);
            self.events.push(SchedCoreEvent::TaskStateChange {
                task: current_task_id,
                from: TaskState::Running,
                to: TaskState::Completed,
            });
        } else {
            // Slice expired
            self.ctx.mark_runnable(current_task_id);
            self.events.push(SchedCoreEvent::TaskStateChange {
                task: current_task_id,
                from: TaskState::Running,
                to: TaskState::Runnable,
            });
            self.scheduler
                .enqueue(&mut self.ctx, current_task_id, SCX_ENQ_REENQ, cpu);
        }
    }

    // Attempt to give `cpu` work if it's idle:
    // 1. Pull from local, per-CPU DSQ
    // 2. Pull from global DSQ
    // 3. Call ops->dispatch() to fill local, per-CPU DSQ
    fn schedule_cpu(&mut self, cpu: CpuId) {
        if !self.ctx.cpu_is_idle(cpu) {
            return;
        }

        let maybe_next_task = self
            .ctx
            .dsq_pop(self.ctx.per_cpu_dsq(cpu))
            .or_else(|| self.ctx.dsq_pop(self.ctx.global_dsq()))
            .or_else(|| {
                self.scheduler.dispatch(&mut self.ctx, cpu);
                self.ctx.dsq_pop(self.ctx.per_cpu_dsq(cpu))
            });

        if let Some(task) = maybe_next_task {
            let prev_state = self.ctx.task(task).state;
            self.ctx.set_running(cpu, task);
            self.scheduler.running(&mut self.ctx, task);

            self.events.extend(vec![
                SchedCoreEvent::TaskStateChange {
                    task,
                    from: prev_state,
                    to: TaskState::Running,
                },
                SchedCoreEvent::CpuCurrentChange {
                    cpu,
                    from: None,
                    to: Some(task),
                },
            ]);
        }
    }

    pub fn create_task(&mut self, required_service: Ticks, weight: u64) -> TaskId {
        let task = self.ctx.create_task(required_service, weight);
        self.scheduler.enable(&mut self.ctx, task);
        task
    }

    pub fn wake_task(&mut self, task: TaskId, wakeup_cpu: CpuId) {
        self.ctx.mark_runnable(task);

        match self.scheduler.select_cpu(&mut self.ctx, task, wakeup_cpu) {
            SelectCpuDecision::DirectDispatch(cpu, slice) => {
                let dsq = self.ctx.per_cpu_dsq(cpu);
                self.ctx.dsq_push_fifo(dsq, task, slice);
            }
            SelectCpuDecision::EnqueueOn(cpu) => {
                let flags: EnqueueFlags = SCX_ENQ_WAKEUP | SCX_ENQ_CPU_SELECTED;
                self.scheduler.enqueue(&mut self.ctx, task, flags, cpu);
            }
            SelectCpuDecision::EnqueueOnDefault => {
                let flags: EnqueueFlags = SCX_ENQ_WAKEUP;
                self.scheduler
                    .enqueue(&mut self.ctx, task, flags, wakeup_cpu);
            }
        }
    }

    pub fn now(&self) -> Ticks {
        self.ctx.now
    }

    pub fn observer(&self) -> &Observer {
        &self.observer
    }
}
