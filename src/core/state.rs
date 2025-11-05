use keyed_priority_queue::KeyedPriorityQueue;
use rustc_hash::FxHashMap;
use slotmap::{SlotMap, new_key_type};
use std::collections::VecDeque;

// Index into Task Vec
pub type TaskId = usize;
pub type CpuId = usize;
pub type Ticks = u64;
new_key_type! {
    pub struct DsqId;
}

#[derive(PartialEq, Eq, Hash, Debug, Copy, Clone)]
pub struct Vtime(pub u64);

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
    pub vtime: u64,
    pub weight: u64,
}

#[derive(Debug)]
pub struct CpuState {
    pub id: CpuId,
    pub current: Option<TaskId>,
}

// KeyedPriorityQueue is a max-heap, so we need to flip-flop Vtime's Ord
impl PartialOrd for Vtime {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        other.0.partial_cmp(&self.0)
    }
}

impl Ord for Vtime {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other.0.cmp(&self.0)
    }
}

#[derive(Debug)]
pub enum Dsq {
    Fifo {
        tasks: VecDeque<TaskId>,
    },
    Priq {
        tasks: KeyedPriorityQueue<TaskId, Vtime>,
    },
}

impl Dsq {
    pub fn new_fifo() -> Self {
        Self::Fifo {
            tasks: VecDeque::new(),
        }
    }

    pub fn new_priq() -> Self {
        Self::Priq {
            tasks: KeyedPriorityQueue::new(),
        }
    }

    pub fn contains(&self, task_id: TaskId) -> bool {
        match self {
            Self::Fifo { tasks } => tasks.contains(&task_id),
            Self::Priq { tasks } => tasks.iter().any(|t| *t.0 == task_id),
        }
    }
}

#[derive(Debug)]
pub struct KernelCtx {
    pub now: Ticks,
    pub cpus: Vec<CpuState>,
    pub tasks: Vec<Task>,
    pub dsqs: SlotMap<DsqId, Dsq>,
    pub task_to_dsq: FxHashMap<TaskId, DsqId>,
    pub global_dsq_id: DsqId,
    pub per_cpu_dsq_ids: Vec<DsqId>,

    // Increment upon task creation
    next_task_id: TaskId,
}

impl KernelCtx {
    pub fn new(num_cpus: usize) -> Self {
        let mut dsqs = SlotMap::with_capacity_and_key(1 + num_cpus);

        // Create global DSQ
        let global_dsq_id = dsqs.insert(Dsq::new_fifo());

        // Create per-CPU DSQs
        let mut per_cpu_dsq_ids = Vec::with_capacity(num_cpus);
        for _ in 0..num_cpus {
            let dsq_id = dsqs.insert(Dsq::new_fifo());
            per_cpu_dsq_ids.push(dsq_id);
        }

        Self {
            now: 0,
            cpus: (0..num_cpus)
                .map(|id| CpuState { id, current: None })
                .collect(),
            tasks: Vec::new(),
            dsqs,
            task_to_dsq: FxHashMap::default(),
            global_dsq_id,
            per_cpu_dsq_ids,
            next_task_id: 0,
        }
    }

    pub fn create_task(&mut self, required_service: Ticks, weight: u64) -> TaskId {
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
            vtime: 0,
            weight,
        };

        debug_assert_eq!(self.tasks.len(), id, "TaskId must match Vec index");
        self.tasks.push(task);

        id
    }

    pub fn advance_time(&mut self, delta: Ticks) {
        self.now = self.now.saturating_add(delta);
    }

    pub fn create_dsq_fifo(&mut self) -> DsqId {
        self.dsqs.insert(Dsq::new_fifo())
    }

    pub fn create_dsq_priq(&mut self) -> DsqId {
        self.dsqs.insert(Dsq::new_priq())
    }

    fn dsq_push(&mut self, dsq_id: DsqId, task_id: TaskId, slice: Ticks, vtime: Option<Vtime>) {
        assert!(
            !self.task_to_dsq.contains_key(&task_id),
            "Task {task_id} already present in some DSQ"
        );

        let task = self.task_mut(task_id);
        debug_assert!(
            task.state != TaskState::Completed && task.state != TaskState::Running,
            "Task {task_id} must not be Running or Completed when enqueued"
        );

        task.allocated_timeslice = Some(slice);
        let dsq = self.dsqs.get_mut(dsq_id).expect("Unknown DSQ");

        match dsq {
            Dsq::Fifo { tasks } => tasks.push_back(task_id),
            Dsq::Priq { tasks } => {
                tasks.push(
                    task_id,
                    vtime.expect("Attempted to push to a PrioDsq with no vtime"),
                );
            }
        };

        self.task_to_dsq.insert(task_id, dsq_id);
    }

    pub fn dsq_push_fifo(&mut self, dsq_id: DsqId, task_id: TaskId, slice: Ticks) {
        self.dsq_push(dsq_id, task_id, slice, None);
    }

    pub fn dsq_push_priq(&mut self, dsq_id: DsqId, task_id: TaskId, slice: Ticks, vtime: Vtime) {
        self.dsq_push(dsq_id, task_id, slice, Some(vtime));
    }

    pub fn dsq_pop(&mut self, dsq_id: DsqId) -> Option<TaskId> {
        let dsq = self.dsqs.get_mut(dsq_id)?;
        let task = match dsq {
            Dsq::Fifo { tasks } => tasks.pop_front(),
            Dsq::Priq { tasks } => tasks.pop().map(|t| t.0),
        }?;

        let removed = self.task_to_dsq.remove(&task);
        debug_assert!(removed.is_some(), "Task {task} missing DSQ membership");

        Some(task)
    }

    pub fn dsq_move_to_local(&mut self, dsq_id: DsqId, cpu: CpuId) {
        if let Some(task) = self.dsq_pop(dsq_id) {
            self.dsq_push_fifo(
                self.per_cpu_dsq(cpu),
                task,
                self.task(task)
                    .allocated_timeslice
                    .expect("Task on DSQ must have slice"),
            );
        }
    }

    pub fn task_in_any_dsq(&self, task_id: TaskId) -> bool {
        self.task_to_dsq.contains_key(&task_id)
    }

    pub fn task(&self, task_id: TaskId) -> &Task {
        &self.tasks[task_id]
    }

    pub fn task_mut(&mut self, task_id: TaskId) -> &mut Task {
        &mut self.tasks[task_id]
    }

    pub fn global_dsq(&self) -> DsqId {
        self.global_dsq_id
    }

    pub fn per_cpu_dsq(&self, cpu: CpuId) -> DsqId {
        self.per_cpu_dsq_ids[cpu]
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
        let task = self.task_mut(task_id);
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
        let task = self.task_mut(task_id);
        task.state = TaskState::Blocked;
        task.current_cpu = None;
    }

    pub fn mark_completed(&mut self, task_id: TaskId, completion_time: Ticks) {
        debug_assert!(
            !self.task_to_dsq.contains_key(&task_id),
            "Completing task {} that is still enqueued",
            task_id
        );

        let task = &mut self.tasks[task_id];
        debug_assert!(
            task.state == TaskState::Running,
            "Task {task_id} must have been running before marked complete"
        );

        task.state = TaskState::Completed;
        task.current_cpu = None;
        task.consumed_service = task.required_service;
        task.completion_time = Some(completion_time);
    }

    // Return previous state (runnable, but possibly blocked if ddsp'd)
    pub fn set_running(&mut self, cpu: CpuId, task_id: TaskId) -> TaskState {
        debug_assert!(
            !self.task_to_dsq.contains_key(&task_id),
            "Running task {task_id} must not be enqueued"
        );
        debug_assert!(
            self.cpus[cpu].current.is_none(),
            "CPU {cpu} already running a task"
        );

        self.cpus[cpu].current = Some(task_id);
        let task_state = self.task_mut(task_id);
        let prev_state = task_state.state;
        task_state.state = TaskState::Running;
        task_state.current_cpu = Some(cpu);
        prev_state
    }

    pub fn clear_cpu(&mut self, cpu: CpuId) {
        self.cpus[cpu].current = None;
    }
}
