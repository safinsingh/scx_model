use crate::core::state::Ticks;

pub type JobId = u64;

#[derive(Debug, Clone)]
pub struct Job {
    pub id: JobId,
    pub arrival_time: Ticks,
    pub run_time: Ticks,
}

#[derive(Debug, Clone)]
pub struct JobInstance {
    pub job: Job,
    pub start_time: Option<Ticks>,
    pub completion_time: Option<Ticks>,
}
