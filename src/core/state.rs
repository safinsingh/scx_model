use std::collections::{HashMap, VecDeque};

pub type TaskId = u64;
pub type CpuId = usize;
pub type DsqId = u64;
pub type Ticks = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    Runnable,
    Running,
    Blocked,
    Completed,
}

#[derive(Debug)]
pub struct Task {
    pub id: TaskId,
    pub state: TaskState,
    pub current_cpu: Option<CpuId>,
    pub required_service: Ticks,
    pub consumed_service: Ticks,
    pub allocated_timeslice: Option<Ticks>,
    pub consumed_timeslice: Ticks,
    pub completion_time: Option<Ticks>,
}

#[derive(Debug)]
pub struct CpuState {
    pub id: CpuId,
    pub current: Option<TaskId>,
}

#[derive(Debug)]
pub struct Dsq {
    pub tasks: VecDeque<TaskId>,
}

impl Dsq {
    pub fn new() -> Self {
        Self {
            tasks: VecDeque::new(),
        }
    }
}

#[derive(Debug)]
pub struct KernelCtx {
    pub now: Ticks,
    pub cpus: Vec<CpuState>,
    pub tasks: HashMap<TaskId, Task>,
    pub dsqs: HashMap<DsqId, Dsq>,
    pub task_to_dsq: HashMap<TaskId, DsqId>,
    pub global_dsq_id: DsqId,
    pub per_cpu_dsq_ids: Vec<DsqId>,

    // Increment upon task/DSQ creation
    next_task_id: TaskId,
    next_dsq_id: DsqId,
}

impl KernelCtx {
    pub fn new(num_cpus: usize) -> Self {
        let mut state = Self {
            now: 0,
            cpus: (0..num_cpus)
                .map(|id| CpuState { id, current: None })
                .collect(),
            tasks: HashMap::new(),
            dsqs: HashMap::new(),
            task_to_dsq: HashMap::new(),
            global_dsq_id: 0,
            per_cpu_dsq_ids: Vec::with_capacity(num_cpus),
            next_task_id: 0,
            next_dsq_id: 0,
        };

        let global_dsq_id = state.create_dsq();
        state.global_dsq_id = global_dsq_id;

        for _ in 0..num_cpus {
            let dsq_id = state.create_dsq();
            state.per_cpu_dsq_ids.push(dsq_id);
        }

        state
    }

    pub fn create_task(&mut self, required_service: Ticks) -> TaskId {
        let id = self.next_task_id;
        self.next_task_id += 1;

        let task = Task {
            id,
            state: TaskState::Blocked,
            current_cpu: None,
            required_service,
            consumed_service: 0,
            allocated_timeslice: None,
            consumed_timeslice: 0,
            completion_time: None,
        };

        let old = self.tasks.insert(id, task);
        debug_assert!(old.is_none(), "TaskId collision");

        id
    }

    pub fn advance_time(&mut self, delta: Ticks) {
        self.now = self.now.saturating_add(delta);
    }

    pub fn create_dsq(&mut self) -> DsqId {
        let id = self.next_dsq_id;
        self.next_dsq_id += 1;
        let previous = self.dsqs.insert(id, Dsq::new());
        debug_assert!(previous.is_none(), "DSQ ID collision");
        id
    }

    pub fn dsq_push_back(&mut self, dsq_id: DsqId, task_id: TaskId, slice: Ticks) {
        assert!(
            !self.task_to_dsq.contains_key(&task_id),
            "Task {task_id} already present in some DSQ"
        );

        let task = self
            .tasks
            .get_mut(&task_id)
            .expect(&format!("Attempting to enqueue unknown task {task_id}"));
        debug_assert!(
            task.state != TaskState::Completed && task.state != TaskState::Running,
            "Task {task_id} must not be Running or Completed when enqueued"
        );

        task.allocated_timeslice = Some(slice);
        let dsq = self.dsqs.get_mut(&dsq_id).expect("Unknown DSQ");
        dsq.tasks.push_back(task_id);

        self.task_to_dsq.insert(task_id, dsq_id);
    }

    pub fn dsq_pop_front(&mut self, dsq_id: DsqId) -> Option<TaskId> {
        let dsq = self.dsqs.get_mut(&dsq_id)?;
        let task = dsq.tasks.pop_front()?;

        let removed = self.task_to_dsq.remove(&task);
        debug_assert!(removed.is_some(), "Task {task} missing DSQ membership");

        Some(task)
    }

    pub fn task_in_any_dsq(&self, task: TaskId) -> bool {
        self.task_to_dsq.contains_key(&task)
    }

    pub fn task_state(&self, task: TaskId) -> TaskState {
        self.tasks
            .get(&task)
            .expect("Queried state of invalid task")
            .state
    }

    pub fn global_dsq(&self) -> DsqId {
        self.global_dsq_id
    }

    pub fn per_cpu_dsq(&self, cpu: CpuId) -> DsqId {
        self.per_cpu_dsq_ids[cpu]
    }

    pub fn task_mut(&mut self, task: TaskId) -> &mut Task {
        self.tasks.get_mut(&task).expect("Queried invalid task")
    }

    pub fn cpu_is_idle(&self, cpu: CpuId) -> bool {
        self.cpus[cpu].current.is_none()
    }

    pub fn pick_idle_cpu(&self) -> Option<CpuId> {
        self.cpus
            .iter()
            .find(|cpu| cpu.current.is_none())
            .map(|cpu| cpu.id)
    }

    pub fn mark_runnable(&mut self, task_id: TaskId) {
        let task = self.tasks.get_mut(&task_id).expect("Unknown task");
        debug_assert!(
            task.state != TaskState::Completed,
            "Completed task {} cannot be runnable",
            task.id
        );
        task.state = TaskState::Runnable;
        task.current_cpu = None;
    }

    pub fn mark_blocked(&mut self, task_id: TaskId) {
        debug_assert!(
            !self.task_to_dsq.contains_key(&task_id),
            "Blocking task {} that is still enqueued",
            task_id
        );
        let task = self.tasks.get_mut(&task_id).expect("Unknown task");
        task.state = TaskState::Blocked;
        task.current_cpu = None;
    }

    pub fn mark_completed(&mut self, task_id: TaskId, completion_time: Ticks) {
        debug_assert!(
            !self.task_to_dsq.contains_key(&task_id),
            "Completing task {} that is still enqueued",
            task_id
        );

        let task = self.tasks.get_mut(&task_id).expect("Unknown task");
        debug_assert!(
            task.state == TaskState::Running,
            "Task {task_id} must have been running before marked complete"
        );

        task.state = TaskState::Completed;
        task.current_cpu = None;
        task.consumed_service = task.required_service;
        task.completion_time = Some(completion_time);
    }

    pub fn set_running(&mut self, cpu: CpuId, task: TaskId) {
        debug_assert!(
            !self.task_to_dsq.contains_key(&task),
            "Running task {task} must not be enqueued"
        );
        debug_assert!(
            self.cpus[cpu].current.is_none(),
            "CPU {cpu} already running a task"
        );

        self.cpus[cpu].current = Some(task);
        let task_state = self.tasks.get_mut(&task).expect("Unknown task");
        task_state.state = TaskState::Running;
        task_state.current_cpu = Some(cpu);
    }

    pub fn clear_cpu(&mut self, cpu: CpuId) {
        self.cpus[cpu].current = None;
    }
}
