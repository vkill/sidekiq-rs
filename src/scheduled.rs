use crate::{periodic::PeriodicJob, RedisPool, UnitOfWork};
use slog::debug;

pub struct Scheduled {
    redis: RedisPool,
    logger: slog::Logger,
}

impl Scheduled {
    pub fn new(redis: RedisPool, logger: slog::Logger) -> Self {
        Self { redis, logger }
    }

    pub async fn enqueue_jobs(
        &self,
        now: chrono::DateTime<chrono::Utc>,
        sorted_sets: &Vec<String>,
    ) -> Result<usize, Box<dyn std::error::Error>> {
        let mut n = 0;
        for sorted_set in sorted_sets {
            let mut redis = self.redis.get().await?;

            let jobs: Vec<String> = redis
                .zrangebyscore_limit(sorted_set.clone(), "-inf", now.timestamp(), 0, 100)
                .await?;

            n += jobs.len();

            for job in jobs {
                if redis.zrem(sorted_set.clone(), job.clone()).await? {
                    let work = UnitOfWork::from_job_string(job)?;

                    debug!(self.logger, "Enqueueing job";
                        "class" => &work.job.class,
                        "queue" => &work.queue
                    );

                    work.enqueue_direct(&mut *redis).await?;
                }
            }
        }

        Ok(n)
    }

    pub async fn enqueue_periodic_jobs(
        &self,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<usize, Box<dyn std::error::Error>> {
        let mut conn = self.redis.get().await?;

        let periodic_jobs: Vec<String> = conn
            .zrangebyscore_limit("periodic".to_string(), "-inf", now.timestamp(), 0, 100)
            .await?;

        for periodic_job in &periodic_jobs {
            let pj = PeriodicJob::from_periodic_job_string(periodic_job.clone())?;

            if pj.update(&mut *conn, periodic_job).await? {
                let job = pj.into_job();
                let work = UnitOfWork::from_job(job);

                debug!(self.logger, "Enqueueing periodic job";
                    "args" => &pj.args,
                    "class" => &work.job.class,
                    "queue" => &work.queue,
                    "name" => &pj.name,
                    "cron" => &pj.cron,
                );

                work.enqueue_direct(&mut conn).await?;
            }
        }

        Ok(periodic_jobs.len())
    }
}
