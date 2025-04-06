---
description: Example showing how to use the inference functionality
---

# 🍄 Model Inference

The Bubbaloop server is able to run **inference** efficiently Visual Language Models (VLM) using directly the camera streams without any latency in the same process and broadcast the results.

Supported models (via [Kornia](https://github.com/kornia/kornia-paligemma) / [Candle](https://github.com/huggingface/candle))

* PaliGemma: [https://ai.google.dev/gemma/docs/paligemma](https://ai.google.dev/gemma/docs/paligemma/prompt-system-instructions)

## Edit the pipeline

Similar to the [Camera Recording ](examples/camera-recording.md)pipeline, we can customize the `inference.ron` pipeline to adjust to our system setup. This will require compiling every time you modify your config.

```json
(
    tasks: [
        // NOTE: Modify this block to customize
        (
            id: "cam0",
            type: "crate::cu29::tasks::VideoCapture",
            config: {
                config: {
                // URL of the RTSP camera
                "source_type": "rtsp",
                "source_uri": "rtsp://<username>:<password>@<ip>:<port>/<stream>"
            }
        ),
        (
            id: "inference",
            type: "crate::cu29::tasks::Inference",
        ),
        (
            id: "bcast_text",
            type: "crate::cu29::tasks::BroadcastChat",
        ),
        (
            id: "bcast_image",
            type: "crate::cu29::tasks::BroadcastImage",
        ),
    ],
    cnx: [
        (src: "cam0", dst: "inference", msg: "crate::cu29::msgs::ImageRgb8Msg"),
        (src: "cam0", dst: "bcast_image", msg: "crate::cu29::msgs::ImageRgb8Msg"),
        (src: "inference", dst: "bcast_text", msg: "crate::cu29::msgs::PromptResponseMsg"),
    ],
    logging: (
        slab_size_mib: 1024, // Preallocates 1GiB of memory map file at a time
        section_size_mib: 100, // Preallocates 100MiB of memory map per section for the main logger.
        enable_task_logging: false,
    ),
)
```

## Start the server

```
just serve
```

## Start the inference

```
just start-pipeline inference 0.0.0.0 3000
```

By default, this command will start the inference engine using the prompt "cap en" — to generate a short capture from each frame.

{% hint style="info" %}
Check the supported prompts: [https://ai.google.dev/gemma/docs/paligemma/prompt-system-instructions](https://ai.google.dev/gemma/docs/paligemma/prompt-system-instructions)
{% endhint %}

In your terminal you should be able to get somethin similar

```
[2025-04-06T14:20:13Z INFO  bubbaloop::api::server] 🚀 Starting the server
[2025-04-06T14:20:13Z INFO  bubbaloop::api::server] 🔥 Listening on: 0.0.0.0:3000
[2025-04-06T14:20:13Z INFO  bubbaloop::api::server] 🔧 Press Ctrl+C to stop the server
[2025-04-06T14:20:31Z DEBUG bubbaloop::cu29::tasks::inference] Received response from inference thread: PromptResponseMsg { prompt: "cap en", response: " Two people are sitting on the bed. In-front of them there is a table with some objects and other things on it. On top of them there is roof, light and we can see trees and sky in the background is sunny." }
```

## Inference settings

We expose some setting via a REST api to the following end point.

```
curl -X POST "http://localhost:3000/api/v0/inference/settings" \
  -H "Content-Type: application/json" \
  -d '{"prompt": "answer Is there any human?"}'
```

This will fix the prompt to run inference on to detect people

## Broadcast

You can access also to the image streams and prompts results via the following API including their timestamps.

**Jpeg encoded images**

```html
http://localhost:3000/api/v0/inference/image
```

**Model inference results**

```
http://localhost:3000/api/v0/inference/results
```

### Visualize streams

We provide a small Python script that calls the above end points and visualize the results with [Rerun](https://rerun.io/)

{% hint style="info" %}
[https://github.com/kornia/bubbaloop/blob/main/examples/python-inference/client.py](../examples/python-inference/client.py)
{% endhint %}

<figure><img src="https://github.com/kornia/data/blob/main/bubbaloop_inference.png?raw=true" alt=""><figcaption></figcaption></figure>

## Stop recording

To stop the pipeline, use the `stop-pipeline` command:

```
just stop-pipeline inference 0.0.0.0 3000
```
