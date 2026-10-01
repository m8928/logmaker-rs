//! Kafka producer sender.
//!
//! `send` only enqueues the record (like the Java `KafkaProducer`); a
//! background thread batches records and produces them round-robin across the
//! topic's partitions. Records carry the same headers as the Java edition:
//! `index`, `read_time`, `module`, `pointer`, `file_name`, `parser_name`.

use std::collections::BTreeMap;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, SendTimeoutError};
use logmaker_plugin_api::{ArgSpec, ArgType, Args, PluginError, Sender, SenderFactory, arg_str};
use rskafka::BackoffConfig;
use rskafka::client::ClientBuilder;
use rskafka::client::partition::{Compression, PartitionClient, UnknownTopicHandling};
use rskafka::record::Record;
use tokio::sync::watch;

use crate::java_date::{JavaDateFormat, ZonedTime};

/// Records buffered before `send` starts waiting for the producer.
const QUEUE_CAPACITY: usize = 100_000;
/// How long `send` waits for buffer space before reporting an error.
const ENQUEUE_TIMEOUT: Duration = Duration::from_secs(1);
const MAX_BATCH: usize = 1_000;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
/// Pause between reconnection attempts while the cluster is unreachable.
const RECONNECT_DELAY: Duration = Duration::from_secs(1);
const WARN_INTERVAL: Duration = Duration::from_secs(10);
/// How long dropping the sender waits for buffered records to be flushed.
const CLOSE_TIMEOUT: Duration = Duration::from_secs(5);
/// How long an in-flight produce request may continue after the sender is closed.
const CLOSE_GRACE: Duration = Duration::from_secs(1);

pub struct KafkaFactory;

struct KafkaSender {
    name: String,
    index: String,
    index_format: JavaDateFormat,
    read_time_format: JavaDateFormat,
    queue: Option<crossbeam_channel::Sender<Record>>,
    closed: watch::Sender<bool>,
    /// Disconnects when the producer thread exits.
    finished: Receiver<()>,
    worker: Option<JoinHandle<()>>,
}

impl SenderFactory for KafkaFactory {
    fn type_name(&self) -> &str {
        "Kafka"
    }

    fn args(&self) -> Vec<ArgSpec> {
        vec![
            ArgSpec::required(
                "bootstrap",
                ArgType::String,
                "Comma-separated broker list, e.g. kafka:9092.",
            ),
            ArgSpec::required("topic", ArgType::String, "Destination topic."),
            ArgSpec::required("index", ArgType::String, "Prefix of the index header."),
            ArgSpec::required(
                "indexPattern",
                ArgType::String,
                "Date pattern appended to the index header, e.g. yyyyMMdd.",
            ),
        ]
    }

    fn create(&self, name: &str, args: &Args) -> Result<Box<dyn Sender>, PluginError> {
        let brokers: Vec<String> = arg_str(args, "bootstrap")
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|b| !b.is_empty())
            .map(str::to_owned)
            .collect();
        if brokers.is_empty() {
            return Err(PluginError::invalid("bootstrap"));
        }
        let topic = arg_str(args, "topic").unwrap_or_default().to_owned();
        if topic.is_empty() {
            return Err(PluginError::invalid("topic"));
        }
        let index_format = JavaDateFormat::parse_java_time(arg_str(args, "indexPattern").unwrap_or_default())
            .map_err(|_| PluginError::invalid("indexPattern"))?;

        let (queue, records) = crossbeam_channel::bounded(QUEUE_CAPACITY);
        let (closed, closed_rx) = watch::channel(false);
        let (finished_tx, finished) = crossbeam_channel::bounded::<()>(0);
        let mut producer = Producer::new(name, brokers, topic, closed_rx);
        let worker = std::thread::Builder::new()
            .name(format!("kafka-{name}"))
            .spawn(move || {
                let _finished = finished_tx;
                producer.run(&records);
            })
            .map_err(|e| PluginError::failed(format!("cannot start Kafka producer thread: {e}")))?;

        Ok(Box::new(KafkaSender {
            name: name.to_owned(),
            index: arg_str(args, "index").unwrap_or_default().to_owned(),
            index_format,
            read_time_format: JavaDateFormat::parse_java_time("yyyyMMddHHmmssSSS").expect("valid pattern"),
            queue: Some(queue),
            closed,
            finished,
            worker: Some(worker),
        }))
    }
}

impl Sender for KafkaSender {
    fn send(&self, data: &str) -> Result<(), PluginError> {
        let now = ZonedTime::now();
        let headers = BTreeMap::from([
            (
                "index".to_owned(),
                format!("{}{}", self.index, self.index_format.format(&now)).into_bytes(),
            ),
            ("read_time".to_owned(), self.read_time_format.format(&now).into_bytes()),
            ("module".to_owned(), b"LogMaker".to_vec()),
            ("pointer".to_owned(), b"0".to_vec()),
            ("file_name".to_owned(), self.name.clone().into_bytes()),
            ("parser_name".to_owned(), self.name.clone().into_bytes()),
        ]);
        let record = Record {
            key: None,
            value: Some(data.as_bytes().to_vec()),
            headers,
            timestamp: rskafka::chrono::Utc::now(),
        };
        let queue = self.queue.as_ref().expect("queue lives until drop");
        queue.send_timeout(record, ENQUEUE_TIMEOUT).map_err(|e| match e {
            SendTimeoutError::Timeout(_) => PluginError::failed("Kafka producer buffer is full"),
            SendTimeoutError::Disconnected(_) => PluginError::failed("Kafka producer has stopped"),
        })
    }
}

impl Drop for KafkaSender {
    /// Closes the queue and waits (bounded) for the producer to flush records
    /// it can still deliver; pending connection attempts are abandoned.
    fn drop(&mut self) {
        let _ = self.closed.send(true);
        drop(self.queue.take());
        match self.finished.recv_timeout(CLOSE_TIMEOUT) {
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                tracing::warn!(sender = %self.name, "Kafka producer did not flush within {CLOSE_TIMEOUT:?}");
            }
            _ => {
                if let Some(worker) = self.worker.take() {
                    let _ = worker.join();
                }
            }
        }
    }
}

struct Producer {
    name: String,
    brokers: Vec<String>,
    topic: String,
    partitions: Vec<PartitionClient>,
    next_partition: usize,
    reconnect_at: Option<Instant>,
    last_warning: Option<Instant>,
    closed: watch::Receiver<bool>,
}

impl Producer {
    fn new(name: &str, brokers: Vec<String>, topic: String, closed: watch::Receiver<bool>) -> Self {
        Self {
            name: name.to_owned(),
            brokers,
            topic,
            partitions: Vec::new(),
            next_partition: 0,
            reconnect_at: None,
            last_warning: None,
            closed,
        }
    }

    fn run(&mut self, records: &Receiver<Record>) {
        let runtime = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
            Ok(runtime) => runtime,
            Err(e) => {
                tracing::error!(sender = %self.name, "cannot start Kafka runtime: {e}");
                return;
            }
        };
        // Ends when the sender is dropped and the queue is drained.
        while let Ok(first) = records.recv() {
            let mut batch = vec![first];
            batch.extend(records.try_iter().take(MAX_BATCH - 1));
            let count = batch.len();
            if let Err(e) = runtime.block_on(self.produce(batch)) {
                self.warn(&format!("dropped {count} record(s): {e}"));
            }
        }
    }

    fn warn(&mut self, message: &str) {
        if self.last_warning.is_none_or(|at| at.elapsed() >= WARN_INTERVAL) {
            self.last_warning = Some(Instant::now());
            tracing::warn!(sender = %self.name, topic = %self.topic, "Kafka: {message}");
        }
    }

    async fn produce(&mut self, batch: Vec<Record>) -> Result<(), String> {
        if self.partitions.is_empty() {
            if *self.closed.borrow() {
                return Err("sender closed before the cluster was reachable".into());
            }
            if self.reconnect_at.is_some_and(|at| Instant::now() < at) {
                return Err("cluster unavailable".into());
            }
            let mut closed = self.closed.clone();
            let connected = tokio::select! {
                result = self.connect() => result,
                _ = closed.wait_for(|closed| *closed) => Err("sender closed while connecting".into()),
            };
            if let Err(e) = connected {
                self.reconnect_at = Some(Instant::now() + RECONNECT_DELAY);
                return Err(e);
            }
        }
        let partition = &self.partitions[self.next_partition % self.partitions.len()];
        self.next_partition = self.next_partition.wrapping_add(1);
        let mut closed = self.closed.clone();
        let error = tokio::select! {
            result = tokio::time::timeout(REQUEST_TIMEOUT, partition.produce(batch, Compression::NoCompression)) => {
                match result {
                    Ok(Ok(_)) => return Ok(()),
                    Ok(Err(e)) => e.to_string(),
                    Err(_) => "produce request timed out".to_owned(),
                }
            }
            _ = async {
                let _ = closed.wait_for(|closed| *closed).await;
                tokio::time::sleep(CLOSE_GRACE).await;
            } => "sender closed while producing".to_owned(),
        };
        // Reconnect on the next batch (or drop batches quickly once closed).
        self.partitions.clear();
        Err(error)
    }

    async fn connect(&mut self) -> Result<(), String> {
        let backoff = BackoffConfig {
            deadline: Some(REQUEST_TIMEOUT),
            ..BackoffConfig::default()
        };
        let build = ClientBuilder::new(self.brokers.clone())
            .client_id("logmaker")
            .backoff_config(backoff)
            .build();
        let client = tokio::time::timeout(REQUEST_TIMEOUT, build)
            .await
            .map_err(|_| "connection timed out".to_owned())?
            .map_err(|e| format!("cannot connect to {}: {e}", self.brokers.join(",")))?;

        let topics = client.list_topics().await.map_err(|e| e.to_string())?;
        let partition_ids: Vec<i32> = match topics.into_iter().find(|t| t.name == self.topic) {
            Some(topic) => topic.partitions.into_iter().collect(),
            None => {
                // Mirrors broker-side topic auto-creation for a fresh cluster.
                let controller = client.controller_client().map_err(|e| e.to_string())?;
                controller
                    .create_topic(self.topic.clone(), 1, 1, 5_000)
                    .await
                    .map_err(|e| format!("topic {} does not exist and cannot be created: {e}", self.topic))?;
                vec![0]
            }
        };
        let mut partitions = Vec::with_capacity(partition_ids.len());
        for id in partition_ids {
            let partition = client
                .partition_client(self.topic.clone(), id, UnknownTopicHandling::Retry)
                .await
                .map_err(|e| e.to_string())?;
            partitions.push(partition);
        }
        if partitions.is_empty() {
            return Err(format!("topic {} has no partitions", self.topic));
        }
        self.partitions = partitions;
        self.reconnect_at = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use logmaker_plugin_api::check_args;
    use serde_json::json;

    use super::*;

    #[test]
    fn validates_arguments() {
        let args = json!({"bootstrap": " , ", "topic": "t", "index": "i", "indexPattern": "yyyy"});
        let args = args.as_object().cloned().unwrap();
        check_args(&KafkaFactory.args(), &args).unwrap();
        assert_eq!(
            KafkaFactory.create("k", &args).err(),
            Some(PluginError::invalid("bootstrap"))
        );

        let args = json!({"bootstrap": "x:9092", "topic": "t", "index": "i", "indexPattern": "yyyy T"});
        let args = args.as_object().cloned().unwrap();
        assert_eq!(
            KafkaFactory.create("k", &args).err(),
            Some(PluginError::invalid("indexPattern"))
        );
    }

    #[test]
    fn send_enqueues_without_a_reachable_cluster_and_drop_returns() {
        // Port 9 (discard) refuses connections, so the producer drops batches.
        let args = json!({"bootstrap": "127.0.0.1:9", "topic": "t", "index": "idx_", "indexPattern": "yyyyMMdd"});
        let sender = KafkaFactory.create("k", args.as_object().unwrap()).unwrap();
        sender.send("line").unwrap();
        drop(sender);
    }
}
