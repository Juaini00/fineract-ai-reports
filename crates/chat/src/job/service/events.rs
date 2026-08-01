use super::*;

impl JobService {
    /// Emit a chat-job event: durable PG insert + best-effort Redis publish
    /// (`chat_job:{id}:latest_event`). SSE handlers poll the Redis key.
    /// ponytail: best-effort — Redis failures are warned but not propagated, since PG is the source of truth.
    pub(super) async fn emit_event(
        &self,
        job_id: Uuid,
        kind: &str,
        step: Option<&str>,
        payload: Value,
    ) -> Result<()> {
        self.jobs
            .insert_event(job_id, kind, step, payload.clone())
            .await?;
        if let Some(client) = &self.redis {
            let body = json!({
                "kind": kind,
                "step": step,
                "payload": payload,
                "at": Utc::now(),
            })
            .to_string();
            let key = format!("chat_job:{job_id}:latest_event");
            match client.get_multiplexed_async_connection().await {
                Ok(mut conn) => {
                    let result: redis::RedisResult<()> =
                        redis::AsyncCommands::set_ex(&mut conn, key, body.clone(), 3600).await;
                    if let Err(error) = result {
                        warn!(job_id = %job_id, redis_url = %redis_url_log_value(&self.redis_url), error = %error, "redis publish event failed");
                    }
                    // Live fan-out. The Postgres insert above is the durable record; this is
                    // best-effort and must never fail the job.
                    let channel = format!("chat_job:{job_id}:events");
                    let published: redis::RedisResult<()> =
                        redis::AsyncCommands::publish(&mut conn, channel, body.clone()).await;
                    if let Err(error) = published {
                        warn!(
                            job_id = %job_id,
                            redis_url = %redis_url_log_value(&self.redis_url),
                            error = %error,
                            "redis publish to job channel failed",
                        );
                    }
                    if matches!(kind, "final" | "error") {
                        let state_key = format!("chat_job:{job_id}:live_state");
                        let state = if kind == "final" {
                            "completed"
                        } else {
                            "failed"
                        };
                        let _: redis::RedisResult<()> = redis::AsyncCommands::set_ex(
                            &mut conn,
                            state_key,
                            state.to_string(),
                            3600,
                        )
                        .await;
                    }
                }
                Err(error) => {
                    warn!(job_id = %job_id, redis_url = %redis_url_log_value(&self.redis_url), error = %error, "redis connect failed")
                }
            }
        }
        Ok(())
    }
}
