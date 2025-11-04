use super::job::{Job, JobInstance};
use crate::{
    core::{
        driver::SchedCore,
        state::{CpuId, TaskId},
    },
    scheduler::Scheduler,
};
use std::collections::HashMap;

pub struct Sim<S: Scheduler> {
    pub core: SchedCore<S>,
    pub jobs: Vec<JobInstance>,
    job_cursor: usize,
    // TaskId --> job[index] map; used to propagate task completion to Job object
    tasks_to_jobs: HashMap<TaskId, usize>,
    num_cpus: usize,
}

impl<S: Scheduler> Sim<S> {
    pub fn new(mut jobs: Vec<Job>, num_cpus: usize) -> Self {
        assert!(num_cpus > 0, "Simulation requires at least one CPU");
        jobs.sort_by(|a, b| {
            a.start_time
                .cmp(&b.start_time)
                .then_with(|| a.id.cmp(&b.id))
        });
        let jobs = jobs
            .into_iter()
            .map(|job| JobInstance {
                job,
                completion_time: None,
            })
            .collect();

        Self {
            core: SchedCore::<S>::new(num_cpus),
            jobs,
            job_cursor: 0,
            tasks_to_jobs: HashMap::new(),
            num_cpus,
        }
    }

    pub fn step(&mut self) {
        self.handle_arrivals();
        let completed_tasks = self.core.tick();

        let completion_time = self.core.now();
        for task in completed_tasks {
            let job_index = *self
                .tasks_to_jobs
                .get(&task)
                .expect("Completed job missing associated task");

            self.jobs[job_index].completion_time = Some(completion_time);
        }
    }

    fn handle_arrivals(&mut self) {
        let now = self.core.now();
        let arriving_jobs = self.jobs[self.job_cursor..]
            .iter_mut()
            .take_while(|job| job.job.start_time == now); // This will be contiguous, since jobs are sorted

        for job in arriving_jobs {
            let task_id = self.core.ctx.create_task(job.job.run_time);
            self.tasks_to_jobs.insert(task_id, self.job_cursor);

            let wakeup_cpu = (job.job.id % self.num_cpus as u64) as CpuId;
            self.core.wake_task(task_id, wakeup_cpu);

            self.job_cursor += 1;
        }
    }

    pub fn all_jobs_completed(&self) -> bool {
        self.jobs.iter().all(|job| job.completion_time.is_some())
    }
}
