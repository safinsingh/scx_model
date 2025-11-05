use average::Estimate;
use rand::prelude::*;
use rand_distr::Poisson;
use scx_model::{
    Job, SchedCoreEvent, Sim,
    core::Ticks,
    scheduler::{FifoScheduler, PriqScheduler},
    sim::JobId,
};
use std::{cmp, ops::Range};

fn main() {
    let jobs = JobGenerator {
        seed: 0,
        // Latest job arrival
        horizon: 1000,
        // E[jobs/timestep]
        lambda: 1.0,
        // Proportion of heavy-weight jobs
        p_weighted: 0.3,
        // Simulate bimodal job length
        p_hit: 0.2,
        cache_hit_range: 1..3,
        cache_miss_range: 6..10,
    }
    .generate();
    let num_cpus = 8;
    let mut sim = Sim::<PriqScheduler>::new(jobs, num_cpus);

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
                max_idle = cmp::max(max_idle, current_idle[cpu]);
            } else {
                current_idle[cpu] = 0;
            }
        }
    }

    let heavy_slowdowns = sim.jobs_filter_map(
        |j| j.job.weight == 10000,
        |j| (j.completion_time.unwrap() as f64 - j.job.arrival_time as f64) / j.job.run_time as f64,
    );
    let normal_slowdowns = sim.jobs_filter_map(
        |j| j.job.weight == 100,
        |j| (j.completion_time.unwrap() as f64 - j.job.arrival_time as f64) / j.job.run_time as f64,
    );

    println!("Average heavy slowdown: {:.3}", avg(heavy_slowdowns));
    println!("Average normal slowdown: {:.3}", avg(normal_slowdowns));
    println!("Longest starvation period: {:?} ticks", max_idle);
}

/// HELPERS ///

struct JobGenerator {
    seed: u64,
    horizon: Ticks,
    lambda: f64,
    p_weighted: f64,
    p_hit: f64,
    cache_hit_range: Range<Ticks>,
    cache_miss_range: Range<Ticks>,
}

impl JobGenerator {
    fn generate(self) -> Vec<Job> {
        let Self {
            seed,
            horizon,
            lambda,
            p_weighted,
            p_hit,
            cache_hit_range,
            cache_miss_range,
        } = self;

        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
        let poisson = Poisson::new(lambda).expect("invalid lambda for Poisson");
        let mut jobs = Vec::with_capacity((lambda * horizon as f64) as usize);
        let mut next_id: JobId = 0;

        for t in 0..horizon {
            let arrivals = poisson.sample(&mut rng) as u64;
            for _ in 0..arrivals {
                let is_hit = rng.random_bool(p_hit);
                let run_time = rng.random_range(if is_hit {
                    cache_hit_range.clone()
                } else {
                    cache_miss_range.clone()
                });
                let weight = if rng.random_bool(p_weighted) {
                    10000
                } else {
                    100
                };

                jobs.push(Job {
                    id: next_id,
                    arrival_time: t,
                    run_time,
                    weight,
                });
                next_id += 1;
            }
        }

        jobs
    }
}

fn avg(iter: impl Iterator<Item = f64>) -> f64 {
    iter.collect::<average::Mean>().estimate()
}
