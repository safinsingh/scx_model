use super::{CpuId, EnqueueFlags, KernelCtx, Scheduler, SelectCpuDecision, TaskId};

pub struct FifoScheduler;

impl Scheduler for FifoScheduler {
    fn init(_ctx: &mut KernelCtx) -> Self {
        Self
    }

    fn select_cpu(
        &mut self,
        _ctx: &mut KernelCtx,
        _task: TaskId,
        _wakeup_cpu: CpuId,
    ) -> SelectCpuDecision {
        SelectCpuDecision::EnqueueOnDefault
    }

    fn enqueue(
        &mut self,
        ctx: &mut KernelCtx,
        task: TaskId,
        flags: EnqueueFlags,
        _prev_cpu: CpuId,
    ) {
        let _ = flags;
        let dsq = ctx.global_dsq();
        ctx.dsq_push_back(dsq, task, super::SCX_SLICE_DFL);
    }

    fn dispatch(&mut self, _ctx: &mut KernelCtx, _cpu: CpuId) {}
}
