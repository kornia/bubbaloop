---
description: Example showing how to stream data from cameras and log into disk
---

# 📷 Camera Recording

The Bubbaloop platform includes a `cameras` pipeline functionality which allows to stream and record data from multi camera streams and serialize in disk including the video frames metadata such as the timestamps.

## Edit the pipeline file

In order to customize the recording pipeline we need to follow the steps below, eg to adjust our RTSP streams configuration:

{% stepper %}
{% step %}
#### Update the pipeline in[ cameras.rs](../../src/cu29/pipelines/cameras.rs)

Go to [`cameras.rs`](../../src/cu29/pipelines/cameras.rs) an update the `config` parameter by specifying the path to the pipeline `ron` file that you want to use for the recording task.

We provide as an example a couple of pipelines to record from one and multiple cameras. See: `cameras_1.ron` , `cameras_2.ron` , etc.

```rust
#[copper_runtime(config = "src/cu29/pipelines/cmeras_1.ron")]
struct CamerasApp {}
```
{% endstep %}

{% step %}
#### Customize the pipeline file

You can definitely customize the `ron` file e.g to update the camera parameters like the `source_uri` to point to your RTSP camera; or enable disable the broadcasting.

{% hint style="info" %}
The RTSP url it's expected to be as in the following format

<pre><code><strong>"rtsp://&#x3C;username>:&#x3C;password>@&#x3C;ip>:&#x3C;port>/&#x3C;stream>
</strong></code></pre>
{% endhint %}

{% hint style="danger" %}
The `channel_id` must be a valid `usize` number and must be not repeated.
{% endhint %}
{% endstep %}
{% endstepper %}

These are `ron` files examples to use with single and multicam with broadcasting included

{% tabs %}
{% tab title="RTSP (single)" %}
```json
(
    tasks: [
        (
            id: "cam0",
            type: "crate::cu29::tasks::VideoCapture",
            config: {
                "source_type": "rtsp",
                // URL of the RTSP camera
                // rtsp://<username>:<password>@<ip>:<port>/<stream>
                "source_uri": "rtsp://tapo_entrance:123456789@192.168.1.141:554/stream2",
                "channel_id": 0,
            }
        ),
        (
            id: "enc0",
            type: "crate::cu29::tasks::ImageEncoder",
        ),
        (
            id: "logger",
            type: "crate::cu29::tasks::RerunLoggerOne",
            config: {
                // Path to the directory where the recordings will be stored
                "path": "/tmp/",
            }
        ),
        (
            id: "bcast0",
            type: "crate::cu29::tasks::ImageBroadcast",
        ),
    ],
    cnx: [
        (src: "cam0", dst: "enc0", msg: "crate::cu29::msgs::ImageRgb8Msg"),
        (src: "enc0", dst: "logger", msg: "crate::cu29::msgs::EncodedImage"),
        (src: "enc0", dst: "bcast0", msg: "crate::cu29::msgs::EncodedImage"),
    ]
    ,
    logging: (
        slab_size_mib: 1024, // Preallocates 1GiB of memory map file at a time
        section_size_mib: 100, // Preallocates 100MiB of memory map per section for the main logger.
        enable_task_logging: false,
    ),
)

```
{% endtab %}

{% tab title="RTSP (multi)" %}
```json
(
    tasks: [
        (
            id: "cam0",
            type: "crate::cu29::tasks::VideoCapture",
            config: {
                "source_type": "rtsp",
                // URL of the RTSP camera
                // rtsp://<username>:<password>@<ip>:<port>/<stream>
                "source_uri": "rtsp://tapo_entrance:123456789@192.168.1.141:554/stream2",
                "channel_id": 0,
            }
        ),
        (
            id: "cam1",
            type: "crate::cu29::tasks::VideoCapture",
            config: {
                "source_type": "rtsp",
                // URL of the RTSP camera
                // rtsp://<username>:<password>@<ip>:<port>/<stream>
                "source_uri": "rtsp://tapo_terrace:123456789@192.168.1.151:554/stream2",
                "channel_id": 1,
            }
        ),
        (
            id: "enc0",
            type: "crate::cu29::tasks::ImageEncoder",
        ),
        (
            id: "enc1",
            type: "crate::cu29::tasks::ImageEncoder",
        ),
        (
            id: "bcast0",
            type: "crate::cu29::tasks::ImageBroadcast",
        ),
        (
            id: "bcast1",
            type: "crate::cu29::tasks::ImageBroadcast",
        ),
        (
            id: "logger",
            type: "crate::cu29::tasks::RerunLoggerTwo",
            config: {
                // Path to the directory where the logs will be stored
                "path": "/tmp/",
            }
        ),
    ],
    cnx: [
        (src: "cam0", dst: "enc0", msg: "crate::cu29::msgs::ImageRgb8Msg"),
        (src: "cam1", dst: "enc1", msg: "crate::cu29::msgs::ImageRgb8Msg"),
        (src: "enc0", dst: "logger", msg: "crate::cu29::msgs::EncodedImage"),
        (src: "enc1", dst: "logger", msg: "crate::cu29::msgs::EncodedImage"),
        (src: "enc0", dst: "bcast0", msg: "crate::cu29::msgs::EncodedImage"),
        (src: "enc1", dst: "bcast1", msg: "crate::cu29::msgs::EncodedImage"),
    ]
    ,
    logging: (
        slab_size_mib: 1024, // Preallocates 1GiB of memory map file at a time
        section_size_mib: 100, // Preallocates 100MiB of memory map per section for the main logger.
        enable_task_logging: false,
    ),
)

```
{% endtab %}

{% tab title="Webcam" %}
```json
(
    tasks: [
        (
            id: "cam0",
            type: "crate::cu29::tasks::VideoCapture",
            config: {
                "source_type": "v4l2",
                "source_uri": "/dev/video0",
                "source_fps": 30,
                "image_cols": 640,
                "image_rows": 480,
            }
        ),
        (
            id: "rerun",
            type: "crate::cu29::tasks::RerunLogger",
            config: {
                // Path to the directory where the logs will be stored
                "path": "/tmp/",
                // IP address of the rerun server
                "ip": "192.168.1.144",
                // Port of the rerun server
                "port": 9876,
            }
        )
    ],
    cnx: [
        (src: "cam0", dst: "rerun", msg: "crate::cu29::msgs::ImageRgb8Msg"),
    ]
    ,
    logging: (
        slab_size_mib: 1024, // Preallocates 1GiB of memory map file at a time
        section_size_mib: 100, // Preallocates 100MiB of memory map per section for the main logger.
    ),
)
```
{% endtab %}
{% endtabs %}

## Start the server

```
just serve
```

```bash
[2025-04-13T12:22:53Z INFO  bubbaloop::api::server] 🚀 Starting the server
[2025-04-13T12:22:53Z INFO  bubbaloop::api::server] 🔥 Listening on: 0.0.0.0:3000
[2025-04-13T12:22:53Z INFO  bubbaloop::api::server] 🔧 Press Ctrl+C to stop the server
```

## Start streaming

Start the camera pipeline and log using [rerun.io](https://www.rerun.io).

```
just start-pipeline cameras 0.0.0.0 3000
```

```bash
Result: {
  "message": "Pipeline recording started"
}
```

## Visualize the streaming

You can use the example [`python-streaming`](https://github.com/kornia/bubbaloop/tree/main/examples/python-streaming) to visualize the streams in real-time using Rerun.

```bash
python examples/python-streaming/client.py \
   --host 0.0.0.0 --port 3000 --cameras 0 # 1 (for multi cam)
```

{% tabs %}
{% tab title="Single Camera" %}
<figure><img src="https://github.com/kornia/data/blob/main/bubbaloop/bubbaloop_stream.png?raw=true" alt=""><figcaption></figcaption></figure>
{% endtab %}

{% tab title="Multi Camera" %}
<figure><img src="https://github.com/kornia/data/blob/main/bubbaloop/bubbaloop_stream_two_cams.png?raw=true" alt=""><figcaption></figcaption></figure>
{% endtab %}
{% endtabs %}

## Start Recording

Send a request to server to start recording from the cameras

```bash
just start-recording 0.0.0.0 30000
```

#### Client terminal

```
Result: {
  "message": "Pipeline recording started"
}
```

## Stop recording

To stop the pipeline, use the `stop-pipeline` command:

```bash
just stop-pipeline recording 0.0.0.0 3000
```

#### **Client terminal**

```
Result: {
  "message": "Pipeline recording stopped"
}
```

#### **Server terminal**

```bash
[2025-04-13T12:10:45Z DEBUG bubbaloop::api::handles::pipeline] Request to stop pipeline: recording
[2025-04-13T12:10:45Z DEBUG bubbaloop::cu29::pipelines::recording] Recording pipeline stopped
[2025-04-13T12:10:45Z DEBUG re_log_encoding::file_sink] Log stream written to /tmp/1744545975.rrd
```

## Get the recorded data and Visualize

You can copy to your home directory (or via ssh) the recorded files into your computer.

```bash
scp bubbaloop777:/home/nvidia/1735941642.rrd ~/data
```

Open the file directly wth rerun to introspect the recording

```bash
rerun 1735941642.rrd
```
