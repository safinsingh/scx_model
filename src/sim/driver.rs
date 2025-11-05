use num_traits::AsPrimitive;
use rustc_hash::FxHashMap;

use super::job::{Job, JobInstance};
use crate::{
    SchedCoreEvent,
    core::{TaskId, TaskState, driver::SchedCore, state::CpuId},
    scheduler::Scheduler,
};

pub struct Sim<S: Scheduler> {
    pub core: SchedCore<S>,
    pub jobs: Vec<JobInstance>,
    job_cursor: usize,
    num_cpus: usize,

    // TaskId --> job[index] map; used to propagate task completion to Job object
    tasks_to_jobs: FxHashMap<TaskId, usize>,
    // For O(1) completion check
    num_jobs_complete: usize,
}

impl<S: Scheduler> Sim<S> {
    pub fn new(mut jobs: Vec<Job>, num_cpus: usize) -> Self {
        debug_assert!(num_cpus > 0, "Simulation requires at least one CPU");

        jobs.sort_by(|a, b| {
            a.arrival_time
                .cmp(&b.arrival_time)
                .then_with(|| a.id.cmp(&b.id))
        });
        let jobs = jobs
            .into_iter()
            .map(|job| {
                debug_assert!(job.run_time > 0, "Job runtime must be nonzero");
                JobInstance {
                    job,
                    start_time: None,
                    completion_time: None,
                }
            })
            .collect();

        Self {
            core: SchedCore::<S>::new(num_cpus),
            jobs,
            job_cursor: 0,
            num_cpus,
            tasks_to_jobs: FxHashMap::default(),
            num_jobs_complete: 0,
        }
    }

    pub fn step(&mut self) -> Vec<SchedCoreEvent> {
        self.handle_arrivals();

        let now = self.core.now();
        let events = self.core.tick();

        for event in events.iter() {
            match event {
                SchedCoreEvent::TaskStateChange {
                    task,
                    to: TaskState::Completed,
                    ..
                } => {
                    let job_index = self
                        .tasks_to_jobs
                        .remove(&task)
                        .expect("Completed job missing associated task");

                    // The job was serviced during timestep "now"
                    // We report the first full timestep during which job is complete.
                    self.jobs[job_index].completion_time = Some(now + 1);
                    self.num_jobs_complete += 1;
                }
                SchedCoreEvent::TaskStateChange {
                    task,
                    to: TaskState::Running,
                    ..
                } => {
                    let job_index = *self
                        .tasks_to_jobs
                        .get(&task)
                        .expect("Running job missing associated task");

                    self.jobs[job_index].start_time.get_or_insert(now);
                }
                _ => {}
            }
        }

        events
    }

    fn handle_arrivals(&mut self) {
        let now = self.core.now();
        let arriving_jobs = self.jobs[self.job_cursor..]
            .iter_mut()
            .take_while(|job| job.job.arrival_time == now); // This will be contiguous, since jobs are sorted

        for job in arriving_jobs {
            let task_id = self.core.create_task(job.job.run_time, job.job.weight);
            self.tasks_to_jobs.insert(task_id, self.job_cursor);

            let wakeup_cpu = (job.job.id % self.num_cpus as u64) as CpuId;
            self.core.wake_task(task_id, wakeup_cpu);

            self.job_cursor += 1;
        }
    }

    pub fn all_jobs_completed(&self) -> bool {
        self.num_jobs_complete == self.jobs.len()
    }

    pub fn jobs_map<T>(&self, f: impl FnMut(&JobInstance) -> T) -> impl Iterator<Item = f64>
    where
        T: AsPrimitive<f64>,
    {
        self.jobs.iter().map(f).map(|s| s.as_())
    }

    pub fn jobs_filter_map<T>(
        &self,
        f: impl FnMut(&&JobInstance) -> bool,
        m: impl FnMut(&JobInstance) -> T,
    ) -> impl Iterator<Item = f64>
    where
        T: AsPrimitive<f64>,
    {
        self.jobs.iter().filter(f).map(m).map(|s| s.as_())
    }
}
