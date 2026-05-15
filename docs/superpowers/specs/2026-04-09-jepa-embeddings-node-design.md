# I-JEPA Embeddings Node Design

## Goal

A Python node that computes I-JEPA (Image Joint Embedding Predictive Architecture) embeddings from camera frames and publishes them as JSON on Zenoh for any downstream consumer (agents, other nodes, dashboard).

## Architecture

Python node using the Bubbaloop Python SDK:

```
[RTSP Camera Node] --SHM/RawImage--> [JEPA Embedder Node] --JSON/global--> [Any Consumer]
```

- **Input:** `RawImage` (RGBA bytes) over local SHM topic from RTSP camera node
- **Model:** Meta's I-JEPA ViT-H/14 loaded via HuggingFace `transformers`
- **Inference:** PyTorch on CUDA (Jetson Orin GPU)
- **Output:** JSON embedding vector on global Zenoh topic
- **Frame dropping:** Latest-frame-wins. When inference is slower than camera FPS, older frames are dropped. No queue buildup.

## Input

Subscribe to `{camera_instance}/raw` on local SHM topic.

The RTSP camera node publishes `RawImage` containing:
- RGBA bytes (width x height x 4 channels)
- Width and height integers
- Header with timestamp

## Output

Publish JSON to global topic `{instance_name}/embeddings` via `publisher_json()`:

```json
{
  "timestamp": "2026-04-09T14:30:00.123Z",
  "embedding": [0.0123, -0.0456, 0.0789, ...],
  "dim": 1280,
  "model": "facebook/ijepa_vith14_1k",
  "width": 224,
  "height": 224,
  "inference_ms": 45.2
}
```

- `embedding`: Flat list of float32 values. ViT-H/14 produces 1280-dimensional vectors.
- `dim`: Embedding dimensionality (1280 for ViT-H/14).
- `model`: HuggingFace model identifier used for inference.
- `width`, `height`: Input resolution after preprocessing (224x224).
- `inference_ms`: Time taken for the forward pass, for telemetry.

## Configuration

`config.yaml`:

```yaml
name: jepa_front
input_topic: rtsp_camera/raw
model: facebook/ijepa_vith14_1k
device: cuda
```

- `name`: Instance name. Determines output topic prefix. Multiple instances can run against different cameras.
- `input_topic`: The camera node's raw image topic suffix to subscribe to.
- `model`: HuggingFace model identifier. Allows swapping to smaller variants (e.g., ViT-L/16) without code changes.
- `device`: `cuda` (default) or `cpu` for testing.

## Node Implementation

Single Python file using the Bubbaloop Python SDK.

### Initialization (`__init__`)

1. Load I-JEPA model via `transformers.AutoModel.from_pretrained(config["model"])`
2. Move model to CUDA device, set to eval mode
3. Create image preprocessor (resize 224x224, normalize to ImageNet stats, RGBA to RGB)

### Main Loop (`run`)

1. Subscribe to raw frames on local SHM
2. On each frame:
   - Convert RGBA bytes to PIL Image using width/height from message
   - Preprocess: resize to 224x224, convert to RGB, normalize, convert to tensor
   - Run `model.forward()` on CUDA
   - Extract embedding vector by averaging patch tokens (1280-dim for ViT-H/14). I-JEPA does not use a CLS token — it mean-pools the patch token outputs.
   - Publish JSON with embedding, metadata, and timing
3. Frame dropping: single-slot buffer. The loop always processes the latest available frame. If a new frame arrives while inference is running, the old frame is discarded.
4. Graceful shutdown via `ctx.is_shutdown()`

### Preprocessing Pipeline

```
RGBA bytes -> PIL.Image.frombytes(width, height, "RGBA")
           -> .convert("RGB")
           -> resize(224, 224)
           -> to_tensor()
           -> normalize(mean=[0.485, 0.456, 0.406], std=[0.229, 0.224, 0.225])
           -> unsqueeze(0)  # batch dim
           -> .to(device)
```

## Dependencies

- `torch` -- PyTorch with CUDA support (already available on Jetson via JetPack)
- `transformers` -- HuggingFace model loading
- `Pillow` -- Image conversion from raw bytes
- `bubbaloop_sdk` -- Bubbaloop Python node SDK

## Directory Structure

```
jepa-embedder/
├── node.yaml          # Node metadata (role: processor, inputs, outputs)
├── config.yaml        # Default runtime config
├── requirements.txt   # torch, transformers, Pillow
└── src/
    └── main.py        # Node implementation
```

## Node Metadata

`node.yaml`:

```yaml
apiVersion: bubbaloop.dev/v1
name: jepa-embedder
version: "0.1.0"
type: python
description: "I-JEPA vision embeddings - encodes raw camera frames to embedding vectors"
author: kornia

role: processor

inputs:
  - name: raw
    topic_suffix: "{instance}/input/raw"
    encoding: "raw/rgba"
    description: "RGBA image frames from camera node (SHM)"

outputs:
  - name: embeddings
    topic_suffix: "{instance}/embeddings"
    encoding: "application/json"
    description: "I-JEPA embedding vectors as JSON"

config_schema:
  type: object
  required: [input_topic, model]
  properties:
    input_topic:
      type: string
      description: "Camera node raw image topic suffix"
    model:
      type: string
      default: "facebook/ijepa_vith14_1k"
      description: "HuggingFace model identifier"
    device:
      type: string
      enum: ["cuda", "cpu"]
      default: "cuda"
```

## Future Considerations

These are explicitly out of scope for v0.1.0 but noted for later:

- **V-JEPA:** Video embeddings across multiple frames for temporal understanding.
- **TensorRT/ONNX optimization:** Export model for 2-4x inference speedup on Jetson.
- **Protobuf output:** Replace JSON with protobuf for lower overhead and schema queryable support.
- **Batching:** Accumulate N frames and run batch inference for better GPU utilization.
