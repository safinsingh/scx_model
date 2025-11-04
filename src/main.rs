use scx_model::{Job, Sim, scheduler::FifoScheduler};

fn main() {
    let jobs = sample_jobs();
    let num_cpus = 2;
    let mut sim = Sim::<FifoScheduler>::new(jobs, num_cpus);

    while !sim.all_jobs_completed() {
        sim.step();
    }
}

fn sample_jobs() -> Vec<Job> {
    vec![
        Job {
            id: 0,
            start_time: 0,
            run_time: 3,
        },
        Job {
            id: 1,
            start_time: 1,
            run_time: 2,
        },
        Job {
            id: 2,
            start_time: 2,
            run_time: 4,
        },
    ]
}
