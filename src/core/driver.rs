use super::{
    observer::Observer,
    state::{CpuId, KernelCtx, TaskId, Ticks},
};
use crate::scheduler::{
    DispatchError, EnqueueFlags, SCX_ENQ_CPU_SELECTED, SCX_ENQ_PREEMPT, SCX_ENQ_REENQ,
    SCX_ENQ_WAKEUP, Scheduler, SelectCpuDecision,
};

pub struct SchedCore<S: Scheduler> {
    pub ctx: KernelCtx,
    pub scheduler: S,
    observer: Observer,
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
        }
    }

    pub fn tick(&mut self) -> Vec<TaskId> {
        self.ctx.advance_time(1);
        let mut completed = Vec::new();
        let cpu_count = self.ctx.cpus.len();
        for cpu in 0..cpu_count {
            if let Some(task) = self.tick_cpu(cpu) {
                completed.push(task);
            }
        }
        self.observer.observe(&self.ctx);
        completed
    }

    // Return TaskId if the current task has completed upon tick()
    fn tick_cpu(&mut self, cpu: CpuId) -> Option<TaskId> {
        // Attempt to give `cpu` work if it's idle:
        // 1. Pull from local, per-CPU DSQ
        // 2. Pull from global DSQ
        // 3. Call ops->dispatch() to fill local, per-CPU DSQ
        if self.ctx.cpu_is_idle(cpu) {
            self.try_schedule_cpu(cpu);
        }

        let current_task_id = match self.ctx.cpus[cpu].current {
            Some(task) => task,
            None => return None,
        };

        // Increment service/decrement slice for current task
        // In its own block to avoid double-mutable-borrow
        {
            let task = self
                .ctx
                .tasks
                .get_mut(&current_task_id)
                .expect("Running task missing from task table");
            task.consumed_service = task.consumed_service.saturating_add(1);
            task.consumed_timeslice = task.consumed_service.saturating_add(1);
        }

        // Invoke BPF ops->tick()
        self.scheduler.tick(&mut self.ctx, current_task_id);

        let task = self
            .ctx
            .tasks
            .get(&current_task_id)
            .expect("Running task missing from task table after tick()");
        let completed = task.consumed_service >= task.required_service;
        let slice_expired = task.consumed_timeslice
            == task
                .allocated_timeslice
                .expect("Task must has an unset timeslice")
            && !completed;

        if completed {
            self.ctx.clear_cpu(cpu);
            self.ctx.mark_completed(current_task_id, self.ctx.now);
            return Some(current_task_id);
        }

        if slice_expired {
            self.ctx.clear_cpu(cpu);
            self.ctx.mark_runnable(current_task_id);
            let flags: EnqueueFlags = SCX_ENQ_PREEMPT | SCX_ENQ_REENQ;
            self.scheduler
                .enqueue(&mut self.ctx, current_task_id, flags, cpu);
        }

        None
    }

    fn try_schedule_cpu(&mut self, cpu: CpuId) {
        if let Some(task) = self.ctx.dsq_pop_front(self.ctx.per_cpu_dsq(cpu)) {
            self.ctx.set_running(cpu, task);
            return;
        }

        if let Some(task) = self.ctx.dsq_pop_front(self.ctx.global_dsq()) {
            self.ctx.set_running(cpu, task);
            return;
        }

        if let Err(DispatchError::NoRunnableTask) = self.scheduler.dispatch(&mut self.ctx, cpu) {
            // Scheduler left CPU idle.
        }

        if let Some(task) = self.ctx.dsq_pop_front(self.ctx.per_cpu_dsq(cpu)) {
            self.ctx.set_running(cpu, task);
            return;
        }
    }

    pub fn wake_task(&mut self, task: TaskId, wakeup_cpu: CpuId) {
        self.ctx.mark_runnable(task);
        match self.scheduler.select_cpu(&mut self.ctx, task, wakeup_cpu) {
            SelectCpuDecision::DirectDispatch(cpu, slice) => {
                let dsq = self.ctx.per_cpu_dsq(cpu);
                self.ctx.dsq_push_back(dsq, task, slice);
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
