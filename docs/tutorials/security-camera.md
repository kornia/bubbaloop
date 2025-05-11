---
description: 'Bubbaloop 101: Turn Your Phone into a Smart Security Camera in 10 Minutes'
icon: house-signal
cover: >-
  https://images.unsplash.com/photo-1520697830682-bbb6e85e2b0b?crop=entropy&cs=srgb&fm=jpg&ixid=M3wxOTcwMjR8MHwxfHNlYXJjaHw4fHxzZWN1cml0eXxlbnwwfHx8fDE3NDY4OTI5ODF8MA&ixlib=rb-4.1.0&q=85
coverY: 0
---

# Security Camera App

**Why should you care?**

* **You already own the hardware.** An old iPhone or Android device on your windowsill is now your first smart security feed.
* **Privacy‑first.** Everything stays local on a $249 Jetson Orin Nano or your laptop – no cloud fees, no vendor lock‑in.
* **Instant insight.** Live multi‑camera visualization and local video recording with spatial intelligence built in.

This guide walks you through setting up **Bubbaloop**, an open-source camera pipeline built with Rust and [kornia-rs](https://github.com/kornia/kornia-rs), to:

* Ingest real-time video from your phone or IP cameras
* Do high level vision tasks like question answering, object detection etc on frames
* Visualize and interact with the results in real-time
* All with high performance on low-cost edge hardware

⏱️ You’ll go from "unopened box" to live feed + local recording in 10–15 minutes.

***

## What You'll Need

### Your Phone or Any Camera

* **iPhone** – use [RTSP Stream](https://apps.apple.com/us/app/rtsp-stream/id6474928937) or Larix Broadcaster
* **Android** – use [WebCamPro](https://play.google.com/store/apps/details?id=com.shenyaocn.android.WebCamPro\&hl=en)
* **Optional**: IP Cam (RTSP compatible) – e.g. TP-Link Tapo TC65 (\~£29)

<figure><img src="https://encrypted-tbn0.gstatic.com/images?q=tbn:ANd9GcRnFY-EslA-S-Bj-qf9mfLKLJ-LK5bKVuKdbA&#x26;s" alt=""><figcaption></figcaption></figure>

### Hardware

* **Jetson Orin Nano (8GB)** – [Buy here from Seeed Studio](https://www.seeedstudio.com/NVIDIAr-Jetson-Orintm-Nano-Developer-Kit-p-5617.html) (\~$249)
* Or your **Linux laptop / PC**

<figure><img src="https://media-cdn.seeedstudio.com/media/catalog/product/cache/bb49d3ec4ee05b6f018e93f896b8a25d/1/-/1-102110839-nvidia-jetson-orin-nano-developer-kit.jpg" alt=""><figcaption></figcaption></figure>

### Software & Tools

* Rust + Cargo — [https://www.rust-lang.org/](https://www.rust-lang.org/)
* Kornia-rs: high-performance vision tools in Rust — [https://github.com/kornia/kornia-rs](https://github.com/kornia/kornia-rs)
* Just: command runner — [https://just.systems/](https://just.systems/)
* [Rerun.io](https://rerun.io/) for real-time visualization (optional but recommended)

***

## Set Up Camera Streaming First

{% tabs %}
{% tab title="iPhone" %}
* Download [RTSP Stream](https://apps.apple.com/us/app/rtsp-stream/id6474928937)
* Start a stream and take note of the RTSP URL (e.g. `rtsp://your-ip:8554/live`)
{% endtab %}

{% tab title="Android" %}
* Install [WebCamPro](https://play.google.com/store/apps/details?id=com.shenyaocn.android.WebCamPro\&hl=en)
* Enable RTSP streaming
* Get your stream URL (e.g. `rtsp://192.168.1.x:8554/live`)
{% endtab %}
{% endtabs %}

***

## Step-by-Step Setup

### Clone the Repo

```bash
git clone https://github.com/kornia/bubbaloop.git
cd bubbaloop
```

## Configure Your Camera

Edit `src/cu29/pipelines/cameras_1.ron`:

```ron
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
    ],
)
```

### Install bubbaloop

```bash
sudo ./scripts/install_linux.sh
```

This will install all the necessary dependencies including Rust (if not installed on your computer) and start the system process. You can check the status via

```
systemctl status bubbaloop
```

for real time logs

```
sudo journalctl -u bubbaloop.service -f
```

## Start a Camera Pipeline

```bash
bubbaloop pipeline start --name cameras
```

To stop:

```bash
bubbaloop pipeline stop --name cameras
```

List all pipelines:

```bash
bubbaloop pipeline list
```

***

## Start a recording

```bash
bubbaloop recording start
```

To stop:

```bash
bubbaloop recording stop
```

***

## Visualize with Rerun

```bash
python examples/python-streaming/client.py   --host 0.0.0.0 --port 3000 --cameras 0
```

Or view a recorded `.rrd` file:

```bash
scp your-device:/tmp/1735941642.rrd ./
rerun 1735941642.rrd
```

<figure><img src="https://github.com/kornia/data/blob/main/bubbaloop/bubbaloop_inference.png?raw=true" alt=""><figcaption></figcaption></figure>

***

## Running Paligemma for Object Detection (Experimental)

{% hint style="warning" %}
For now the pipelines are mutually exclusive. This means that before starting the inference you need to stop the `cameras` pipeline.

`bubbaloop pipeline stop --name cameras`
{% endhint %}

Edit `src/cu29/pipelines/inference.ron`:

```ron
(
    tasks: [
        (
            id: "cam0",
            type: "crate::cu29::tasks::VideoCapture",
            config: {
                "source_type": "rtsp",
                "source_uri": "rtsp://192.168.1.141:8554/live",
                "channel_id": 0,
            },
        ),
        (
            id: "inference",
            type: "crate::cu29::tasks::Inference",
            config: {
                "prompt": "What objects are in the scene?",
            },
        ),
    ],
)
```

Now you can start safely the inference engine

```bash
bubbaloop pipeline start --name inference
```

### Customise the prompt

You can change the prompt online with the following command

```bash
bubbaloop inference settings --prompt "Is there any human?"
```

### Request the inference result

The inference result can be obtained using the following command

```
bubbaloop inference result
```

***

## Contribute / Feedback

Join our [Discord server](https://discord.com/invite/HfnywwpBnD) or open issues on [GitHub](https://github.com/kornia/bubbaloop).
