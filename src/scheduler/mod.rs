pub mod fifo;

use crate::core::{
    Ticks,
    state::{CpuId, KernelCtx, TaskId},
};
pub use fifo::FifoScheduler;

pub type EnqueueFlags = u64;

pub const SCX_ENQ_WAKEUP: EnqueueFlags = 1 << 0;
pub const SCX_ENQ_HEAD: EnqueueFlags = 1 << 4;
pub const SCX_ENQ_CPU_SELECTED: EnqueueFlags = 1 << 10;
pub const SCX_ENQ_PREEMPT: EnqueueFlags = 1 << 32;
pub const SCX_ENQ_REENQ: EnqueueFlags = 1 << 40;
pub const SCX_ENQ_LAST: EnqueueFlags = 1 << 41;
pub const SCX_ENQ_CLEAR_OPSS: EnqueueFlags = 1 << 56;
pub const SCX_ENQ_DSQ_PRIQ: EnqueueFlags = 1 << 57;

pub const SCX_SLICE_DFL: u64 = 3;

#[derive(Debug)]
pub enum DispatchError {
    NoRunnableTask,
}

#[derive(Debug)]
pub enum SelectCpuDecision {
    DirectDispatch(CpuId, Ticks),
    EnqueueOn(CpuId),
    EnqueueOnDefault,
}

pub trait Scheduler {
    fn init(ctx: &mut KernelCtx) -> Self;

    fn exit(&mut self, _ctx: &mut KernelCtx) {}

    fn select_cpu(
        &mut self,
        ctx: &mut KernelCtx,
        task: TaskId,
        wakeup_cpu: CpuId,
    ) -> SelectCpuDecision;

    fn enqueue(&mut self, ctx: &mut KernelCtx, task: TaskId, flags: EnqueueFlags, prev_cpu: CpuId);

    fn dispatch(&mut self, ctx: &mut KernelCtx, cpu: CpuId);

    fn tick(&mut self, _ctx: &mut KernelCtx, _task: TaskId) {}
}
