use crate::core::state::Ticks;

pub type JobId = u64;

#[derive(Debug, Clone)]
pub struct Job {
    pub id: JobId,
    pub start_time: Ticks,
    pub run_time: Ticks,
}

#[derive(Debug, Clone)]
pub struct JobInstance {
    pub job: Job,
    pub completion_time: Option<Ticks>,
}
