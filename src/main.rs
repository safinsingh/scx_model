use average::Estimate;
use rand::prelude::*;
use scx_model::{Job, SchedCoreEvent, Sim, scheduler::FifoScheduler};

fn main() {
    let jobs = bernoulli_jobs(500, 0.3, 0.3, 2, 6, 0);
    let num_cpus = 2;
    let mut sim = Sim::<FifoScheduler>::new(jobs, num_cpus);

    let mut current_idle = vec![0; num_cpus];
    let mut max_idle = 0;

    while !sim.all_jobs_completed() {
        let now = sim.core.now();
        let events = sim.step();

        let mut got_idle = vec![false; num_cpus];
        for event in events {
            println!("t={} {:?}", now, event);

            match event {
                SchedCoreEvent::CpuIdle { cpu } => got_idle[cpu] = true,
                _ => {}
            };
        }

        for cpu in 0..num_cpus {
            if got_idle[cpu] {
                current_idle[cpu] += 1;
                max_idle = max_idle.max(current_idle[cpu]);
            } else {
                current_idle[cpu] = 0;
            }
        }
    }

    let offcpu_times = sim.jobs_map(|j| j.completion_time.unwrap() - j.start_time.unwrap());
    let response_times = sim.jobs_map(|j| j.start_time.unwrap() - j.job.arrival_time);

    // Time to first run
    println!("Average response time: {:.2} ticks", avg(response_times));
    println!("Average offcpu time: {:.2} ticks", avg(offcpu_times));
    println!("Longest starvation period: {:?} ticks", max_idle);
}

fn bernoulli_jobs(
    ticks: u64,
    p_arrival: f64,
    p_short: f64,
    short_ticks: u64,
    long_ticks: u64,
    seed: u64,
) -> Vec<Job> {
    let mut rng = StdRng::seed_from_u64(seed);
    let mut jobs = Vec::new();

    for t in 0..ticks {
        if rng.random::<f64>() < p_arrival {
            let run_time = if rng.random::<f64>() < p_short {
                short_ticks
            } else {
                long_ticks
            };

            jobs.push(Job {
                id: jobs.len() as u64,
                arrival_time: t,
                run_time,
            });
        }
    }

    jobs
}

fn avg(iter: impl Iterator<Item = f64>) -> f64 {
    iter.collect::<average::Mean>().estimate()
}
