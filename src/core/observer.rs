use super::state::{KernelCtx, TaskState};

#[derive(Debug)]
pub struct Observer {
    step: u64,
}

impl Observer {
    pub fn new() -> Self {
        Self { step: 0 }
    }

    pub fn observe(&mut self, core: &KernelCtx) {
        self.step += 1;

        for cpu in &core.cpus {
            if let Some(task_id) = cpu.current {
                let task = core.task(task_id);
                debug_assert_eq!(
                    task.state,
                    TaskState::Running,
                    "cpu.current task {task_id} must be Running"
                );
                debug_assert_eq!(
                    task.current_cpu,
                    Some(cpu.id),
                    "Task {task_id} metadata current_cpu mismatch"
                );
            }
        }

        for (&task_id, &dsq_id) in &core.task_to_dsq {
            let task = core.task(task_id);
            debug_assert_ne!(
                task.state,
                TaskState::Completed,
                "Completed task {task_id} still present in DSQ {dsq_id:?}"
            );
            debug_assert_ne!(
                task.state,
                TaskState::Running,
                "Running task {task_id} must not appear in any DSQ"
            );
            if let Some(dsq) = core.dsqs.get(dsq_id) {
                debug_assert!(
                    dsq.contains(task_id),
                    "task_to_dsq claims task {task_id} in DSQ {dsq_id:?}, but queue does not contain it"
                );
            } else {
                debug_assert!(false, "task_to_dsq references unknown DSQ {dsq_id:?}");
            }
        }
    }
}
