# FlowForge

**FlowForge** is a blazing-fast, asynchronous, stage-based pipeline framework crafted in Rust, designed to orchestrate complex task workflows with unparalleled efficiency. Built for scalability and resilience, it empowers developers to process tasks through modular stages, seamlessly integrating concurrency control, robust error handling, retries, and persistent database storage. FlowForge is the backbone for high-throughput, mission-critical applications, from data pipelines to task orchestration.

## Proven in Production

A forked version of FlowForge has been successfully deployed in a leading technology company, integrated with the Actix framework. In a high-demand business scenario, it reliably processes **hundreds of gigabytes of files daily**, demonstrating its stability and performance. This deployment, while based on FlowForge's prototype, showcases its potential as a foundation for production-grade systems. The fully engineered version, evolved from this prototype, boasts **exceptional extensibility**, making it a battle-tested framework capable of meeting diverse and evolving requirements.

## Features

* **Asynchronous Excellence**: Harnesses Rust's **tokio** runtime for non-blocking, high-performance task processing.
* **Modular Stage Architecture**: Enables flexible workflows with customizable stages, each with fine-tuned concurrency, timeout, and retry configurations.
* **Seamless Database Integration**: Offers a generic interface for persistent state management, compatible with various storage backends.
* **Robust Error Handling**: Ensures reliability with configurable retries and timeouts for fault-tolerant execution.
* **Scalable Concurrency**: Optimizes resource usage through per-stage concurrency limits, ideal for large-scale workloads.
* **Graceful Termination**: Supports clean pipeline shutdown with comprehensive completion tracking.
* **Extensible Design**: Facilitates custom stage implementations and database integrations for tailored solutions.

## Advantages

* **Unmatched Performance**: Leverages Rust’s zero-cost abstractions for lightning-fast execution, proven in high-throughput environments.
* **Unbounded Flexibility**: Modular design empowers developers to craft bespoke workflows for any domain.
* **Rock-Solid Reliability**: Retries, timeouts, and persistent storage ensure fault tolerance under extreme conditions.
* **Scalability at Core**: Concurrency controls make it ideal for scaling from prototypes to enterprise-grade systems.
* **Type-Safe Assurance**: Rust’s type system minimizes runtime errors, ensuring dependable operation.

## Disadvantages

* **Rust Learning Curve**: Requires familiarity with Rust’s async ecosystem for advanced customizations.
* **Initial Setup Overhead**: Complex workflows may demand additional configuration for stages and databases.
* **Prototype Nature**: While robust, the core framework is a prototype, requiring engineering for specific production needs.

## Future Vision

FlowForge is poised to redefine task orchestration in modern systems. Its extensible architecture paves the way for integrations with distributed systems like Kubernetes, advanced monitoring tools, and AI-driven workflow optimization. As it evolves, FlowForge aims to become the go-to framework for building resilient, high-performance pipelines in industries ranging from fintech to big data analytics. With a vibrant community and ongoing enhancements, FlowForge is set to power the next generation of scalable, fault-tolerant applications.

## Quick Start

### Simple Example

This example demonstrates a pipeline with a single stage for task processing.

```rust
use flowforge::{agent::{Agent, AgentStage, StageStatus}, pipeline::Pipeline, stage::Stage};
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Duration;

// Define a custom stage
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CustomStage { TransformData }

impl std::fmt::Display for CustomStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{:?}", self) }
}

pub struct TransformDataStage;

#[async_trait]
impl Stage for TransformDataStage {
    fn concurrency_limit(&self) -> usize { 5 }
    fn timeout(&self) -> Duration { Duration::from_secs(3) }
    fn retry_times(&self) -> u32 { 2 }
    fn stage(&self) -> AgentStage { AgentStage::TransformData }
    fn clone_box(&self) -> Box<dyn Stage> { Box::new(self.clone()) }

    async fn async_process(&self, agent: &mut Agent, _is_terminated: Arc<AtomicBool>) -> anyhow::Result<()> {
        println!("Transforming agent: {}", agent.id);
        Ok(())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize a mock database manager
    let db_manager = MockDatabaseManager::new("sqlite::memory:".to_string(), 10).await?;

    // Create and configure pipeline
    let mut pipeline = Pipeline::new(db_manager).await?;
    pipeline.register_stages(vec![Box::new(TransformDataStage)]).await?;

    // Submit an agent
    let agent = Agent::new();
    pipeline.submit_agent(agent).await?;

    // Wait for completion
    pipeline.wait_for_completion().await?;
    println!("Pipeline completed successfully!");
    Ok(())
}
```

### Database Integration

Implement the **DatabaseManager** and **DbStore** traits to enable persistent storage. Use **insert\_data** and **update\_data** for agent state management. Refer to the documentation for advanced setups.

## Architecture Overview

* **Agent**: Represents tasks with unique IDs, statuses, and stage parameters, flowing through the pipeline.
* **Stage**: Defines processing units with customizable concurrency, timeout, and retry settings.
* **Pipeline**: Orchestrates agent routing across stages, ensuring efficient task execution.
* **DatabaseManager**: Provides a flexible interface for persistent storage operations.

## Use Cases

* **Big Data Pipelines**: Process terabytes of data through transformation and validation stages.
* **Task Orchestration**: Coordinate complex workflows with dependencies in distributed systems.
* **Real-Time Processing**: Handle streaming data with low-latency, fault-tolerant pipelines.

## Contributing

Join us in shaping the future of FlowForge! Submit pull requests or open issues on the GitHub repository.

## License

FlowForge is licensed under the MIT License. See the LICENSE file for details.
