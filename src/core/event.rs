use crate::core::{CpuId, TaskId, TaskState};

#[derive(Debug)]
pub enum SchedCoreEvent {
    TaskStateChange {
        task: TaskId,
        from: TaskState,
        to: TaskState,
    },
    CpuCurrentChange {
        cpu: CpuId,
        from: Option<TaskId>,
        to: Option<TaskId>,
    },
    // CPU idle even after dispatch()
    CpuIdle {
        cpu: CpuId,
    },
}
