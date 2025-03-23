---
description: Your first Bubbaloop service experience
---

# 🌈 Hello World

## Start the server

```
just serve
```

## Request to start the task

Send a HTTP request to the server to start the background task

```
just start-pipeline bubaloop 0.0.0.0 3000
```

From the server side you will see the following

```bash
[2025-01-05T15:51:33Z DEBUG bubbaloop::pipeline] | Hello !! This is a Bubbaloop !!! 🎮
[2025-01-05T15:51:34Z DEBUG bubbaloop::pipeline] / Hello !! This is a Bubbaloop !!! 🌈
[2025-01-05T15:51:35Z DEBUG bubbaloop::pipeline] - Hello !! This is a Bubbaloop !!! 😊
[2025-01-05T15:51:36Z DEBUG bubbaloop::pipeline] \ Hello !! This is a Bubbaloop !!! 🚀
[2025-01-05T15:51:37Z DEBUG bubbaloop::pipeline] | Hello !! This is a Bubbaloop !!! 🦀
[2025-01-05T15:51:38Z DEBUG bubbaloop::pipeline] / Hello !! This is a Bubbaloop !!! 🎉
```

## Stop the task

To stop the pipeline, use the `stop-pipeline` command:

```
just stop-pipeline bubbaloop 0.0.0.0 3000
```

From client

```
Result: {
  "message": "Pipeline bubbaloop stopped"
}
```

From server

```bash
[2025-01-05T15:51:39Z DEBUG bubbaloop::pipeline] Request to stop pipeline: bubbaloop
[2025-01-05T15:51:40Z DEBUG bubbaloop::pipeline] Pipeline bubbaloop stopped after 155 iterations
```

