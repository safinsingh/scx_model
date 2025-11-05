use std::cmp;

use super::{CpuId, EnqueueFlags, KernelCtx, Scheduler, SelectCpuDecision, TaskId};
use crate::{
    core::{DsqId, Vtime},
    scheduler::SCX_SLICE_DFL,
};

pub struct PriqScheduler {
    global_priq: DsqId,
    vtime_now: u64,
}

impl Scheduler for PriqScheduler {
    fn init(ctx: &mut KernelCtx) -> Self {
        Self {
            global_priq: ctx.create_dsq_priq(),
            vtime_now: SCX_SLICE_DFL,
        }
    }

    fn select_cpu(
        &mut self,
        ctx: &mut KernelCtx,
        _task: TaskId,
        _wakeup_cpu: CpuId,
    ) -> SelectCpuDecision {
        if let Some(cpu) = ctx.pick_idle_cpu() {
            SelectCpuDecision::DirectDispatch(cpu, SCX_SLICE_DFL)
        } else {
            SelectCpuDecision::EnqueueOnDefault
        }
    }

    fn enqueue(
        &mut self,
        ctx: &mut KernelCtx,
        task: TaskId,
        _flags: EnqueueFlags,
        _prev_cpu: CpuId,
    ) {
        let vtime = cmp::max(
            ctx.task(task).vtime,
            self.vtime_now.saturating_sub(SCX_SLICE_DFL),
        );
        ctx.dsq_push_priq(self.global_priq, task, SCX_SLICE_DFL, Vtime(vtime));
    }

    fn dispatch(&mut self, ctx: &mut KernelCtx, cpu: CpuId) {
        ctx.dsq_move_to_local(self.global_priq, cpu);
    }

    // Progress global vtime
    fn running(&mut self, ctx: &mut KernelCtx, task: TaskId) {
        self.vtime_now = cmp::max(self.vtime_now, ctx.task(task).vtime);
    }

    fn stopping(&mut self, ctx: &mut KernelCtx, task: TaskId, _runnable: bool) {
        let task = ctx.task_mut(task);
        task.vtime += (task.consumed_timeslice * 100) / task.weight;
    }

    fn enable(&mut self, ctx: &mut KernelCtx, task: TaskId) {
        ctx.task_mut(task).vtime = self.vtime_now;
    }
}
