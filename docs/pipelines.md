---
description: Basic usage and pipeline management with Bubbaloop
---

# 🍰 Pipeline API

**Bubbaloop** is a Rust-based server application that orchestrates computational pipelines using the Cu29 ([copper-rs](https://github.com/copper-project/copper-rs)) framework. It provides both an HTTP API and CLI for managing these pipelines.

## Core Concepts

* Pipeline Management: The system dynamically manages multiple pipeline types (bubbaloop, inference, recording, streaming) that process data through connected tasks.
* Cu29/Copper Framework: Pipelines are built using the Cu29 framework ([copper-rs](https://github.com/copper-project/copper-rs)), which provides a task-based computation model with message passing between components.
* RON Configuration: Pipelines are defined in [RON](https://github.com/ron-rs/ron) (Rusty Object Notation) files that specify:
  * Tasks: Individual processing components with unique IDs and configurations
  * Connections: Message flows between tasks with specific message types

## Architecture

* API Server: An Axum-based HTTP server that exposes endpoints for pipeline management
* Pipeline Store: Central registry tracking all running pipelines with their statuses
* Result Store: Maintains processing results and enables streaming of data between components

## Pipeline Types

* `bubbaloop` — Our hello-world simple demo pipeline
* `cameras` — Captures and records video streams form single or multiple camera
* `inference` — Processes video streams for inference using computer vision models

## Available API

* `POST /api/v0/pipeline/start` Start a pipeline with specified ID
* `POST /api/v0/pipeline/stop` Stop a running pipeline
* `GET /api/v0/pipeline/list` List all available pipelines with their statuses

## Usage

### Start pipeline

Create and register a pipeline given its name. This will spawn a background task.

```
just start-pipeline HOST IP PIPE_NAME
```

```bash
Result: {
  "message": "Pipeline 'PIPE_NAME' started"
}
```

### Stop pipeline

To stop the pipeline, use the `stop-pipeline` command:

```
just stop-pipeline HOST IP PIPE_NAME
```

```bash
Result: {
  "message": "Pipeline 'PIPE_NAME' stopped"
}
```

### List pipelines

To list all the registered pipelines and their status, use the `list-pipeline` command:

```
just pipeline-list HOST IP
```

```bash
Result: [
  {
    "id": "bubbaloop",
    "status": "Running"
  }
]
```
