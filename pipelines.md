# 🍰 Pipelines

With Bubbaloop you can serve local pipeline orchestrated by copper-rs[^1] which are defined by ron[^2] files.

## Start pipeline

Start and register a pipeline given its name. This will spawn a background task.

```
just pipeline-start HOST IP NAME
```

## Stop pipeline

To stop the pipeline, use the `pipeline-stop` command:

```
just pipeline-stop HOST IP NAME
```

## List pipelines

To list all the registered pipelines, use the `pipeline-list` command:

```
just pipeline-list HOST IP
```

[^1]: Visit the project: [https://github.com/copper-project/copper-rs](https://github.com/copper-project/copper-rs)

[^2]: Rusty Object Notation [https://github.com/ron-rs/ron](https://github.com/ron-rs/ron)
