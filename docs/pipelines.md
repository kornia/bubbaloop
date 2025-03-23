# 🍰 Pipelines

Bubbaloop can serve local pipeline orchestrated by copper-rs[^1] which are defined by ron[^2] files.

## Start pipeline

Create and register a pipeline given its name. This will spawn a background task.

```
just start-pipeline HOST IP PIPE_NAME
```

```bash
Result: {
  "message": "Pipeline 'PIPE_NAME' started"
}
```

## Stop pipeline

To stop the pipeline, use the `stop-pipeline` command:

```
just stop-pipeline HOST IP PIPE_NAME
```

```bash
Result: {
  "message": "Pipeline 'PIPE_NAME' stopped"
}
```

## List pipelines

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

[^1]: Visit the project: [https://github.com/copper-project/copper-rs](https://github.com/copper-project/copper-rs)

[^2]: Rusty Object Notation [https://github.com/ron-rs/ron](https://github.com/ron-rs/ron)
